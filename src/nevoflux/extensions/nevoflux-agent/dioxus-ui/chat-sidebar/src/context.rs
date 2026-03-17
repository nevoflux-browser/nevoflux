/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Application context and state management
//!
//! Provides a global context using Dioxus signals that can be accessed
//! by any component in the tree.

use dioxus::prelude::*;

use crate::state::{
    AgentStatusState, AskUserState, ConnectionState, HistoryState, LiveToolEntry, MaximizeState,
    McpConfigState, Message, PendingFilePick, PendingModeChoice, PermissionRequestState,
    PickedFile, SessionState, SkillItem, StreamingState, TabContext,
};
use shared_protocol::ToolAuthRequest;
use shared_protocol::ChatMode;

/// Global application context
///
/// Contains all shared state as Dioxus signals that can be read and written
/// from any component using `use_app_context()`.
#[derive(Clone, Copy, PartialEq)]
pub struct AppContext {
    /// Session state
    pub session: Signal<SessionState>,
    /// Chat messages
    pub messages: Signal<Vec<Message>>,
    /// Currently streaming message
    pub streaming: Signal<Option<StreamingState>>,
    /// Agent status
    pub agent_status: Signal<AgentStatusState>,
    /// Connection status
    pub connection: Signal<ConnectionState>,
    /// Active permission request
    pub permission_request: Signal<Option<PermissionRequestState>>,
    /// Current tab context
    pub tab_context: Signal<TabContext>,
    /// Session history list
    pub history: Signal<HistoryState>,
    /// MCP server configuration state
    pub mcp_config: Signal<McpConfigState>,
    /// Whether to show the MCP config modal
    pub show_mcp_config: Signal<bool>,
    /// Pending AskUser request from agent
    pub ask_user: Signal<Option<AskUserState>>,
    /// Files picked via native file dialog
    pub picked_files: Signal<Vec<PickedFile>>,
    /// Pending file pick request
    pub pending_file_pick: Signal<Option<PendingFilePick>>,
    /// Pending mode choice for Linux file picker (choose files vs directories)
    pub pending_mode_choice: Signal<Option<PendingModeChoice>>,
    /// Current chat mode (chat, browser, agent)
    pub chat_mode: Signal<ChatMode>,
    /// Available skills for skill selector
    pub available_skills: Signal<Vec<SkillItem>>,
    /// Maximize state (sidebar <-> tab mode)
    pub maximize: Signal<MaximizeState>,
    /// Whether a plan proposal is pending user response
    pub pending_plan: Signal<bool>,
    /// Whether to show the history panel
    pub show_history_panel: Signal<bool>,
    /// Live tool execution entries (real-time during streaming)
    pub live_tools: Signal<Vec<LiveToolEntry>>,
    /// Pending tool authorization request
    pub pending_tool_auth: Signal<Option<ToolAuthRequest>>,
    /// User avatar data URL (from settings)
    pub avatar_url: Signal<Option<String>>,
    /// Whether the sidebar is in minimized rail mode
    pub minimized: Signal<bool>,
    /// Whether this is the first run (no config file exists)
    pub first_run: Signal<bool>,
    /// Whether a provider is configured in agent settings
    pub has_configured_provider: Signal<bool>,
    /// Whether mock mode is enabled
    pub mock_enabled: bool,
}

/// Context provider component
///
/// Wraps children with the application context, initializing all state
/// and setting up message listeners.
#[component]
pub fn ContextProvider(#[props(default = false)] mock_enabled: bool, children: Element) -> Element {
    // Initialize all state signals
    let session = use_signal(SessionState::new);
    let messages = use_signal(Vec::<Message>::new);
    let streaming = use_signal(|| None::<StreamingState>);
    let agent_status = use_signal(AgentStatusState::default);
    let connection = use_signal(|| ConnectionState::Disconnected);
    let permission_request = use_signal(|| None::<PermissionRequestState>);
    let tab_context = use_signal(TabContext::default);
    let history = use_signal(HistoryState::default);
    let mcp_config = use_signal(McpConfigState::default);
    let show_mcp_config = use_signal(|| false);
    let ask_user = use_signal(|| None::<AskUserState>);
    let picked_files = use_signal(Vec::<PickedFile>::new);
    let pending_file_pick = use_signal(|| None::<PendingFilePick>);
    let pending_mode_choice = use_signal(|| None::<PendingModeChoice>);
    let chat_mode = use_signal(ChatMode::default);
    let available_skills = use_signal(Vec::<SkillItem>::new);
    let maximize = use_signal(parse_maximize_params);
    let pending_plan = use_signal(|| false);
    let show_history_panel = use_signal(|| false);
    let live_tools = use_signal(Vec::<LiveToolEntry>::new);
    let pending_tool_auth = use_signal(|| None::<ToolAuthRequest>);
    let avatar_url = use_signal(|| None::<String>);
    let minimized = use_signal(|| false);
    let mut first_run = use_signal(|| false);
    let mut has_configured_provider = use_signal(|| false);

    // Build context
    let mut ctx = AppContext {
        session,
        messages,
        streaming,
        agent_status,
        connection,
        permission_request,
        tab_context,
        history,
        mcp_config,
        show_mcp_config,
        ask_user,
        picked_files,
        pending_file_pick,
        pending_mode_choice,
        chat_mode,
        available_skills,
        maximize,
        pending_plan,
        show_history_panel,
        live_tools,
        pending_tool_auth,
        avatar_url,
        minimized,
        first_run,
        has_configured_provider,
        mock_enabled,
    };

    // Provide context to children
    use_context_provider(|| ctx);

    // Initialize messaging on mount
    use_effect(move || {
        if mock_enabled {
            // Mock mode: simulate connection
            spawn(async move {
                crate::mock::init_mock_messaging(ctx).await;
            });
        } else {
            // Real mode: set up message listener and connect
            crate::messaging::init_message_listener(ctx);

            // Check if we're in maximized mode and need to request source tab context
            let maximize_state = ctx.maximize.read().clone();

            spawn(async move {
                // Progressive retry for agent status (spec: 1s, 3s, 5s, 10s, 15s)
                let delays: [u32; 5] = [1000, 3000, 5000, 10000, 15000];
                let mut status_ok = false;

                // Try immediately first
                match crate::messaging::query_agent_status().await {
                    Ok(status) => {
                        let is_first_run = status.get("first_run").and_then(|v| v.as_bool()).unwrap_or(false);
                        let has_configured = status.get("has_configured_provider").and_then(|v| v.as_bool()).unwrap_or(false);
                        first_run.set(is_first_run);
                        has_configured_provider.set(has_configured);
                        status_ok = true;
                    }
                    Err(_) => {
                        // Agent not ready, start progressive retry
                        for delay in &delays {
                            crate::messaging::sleep_ms(*delay).await;
                            match crate::messaging::query_agent_status().await {
                                Ok(status) => {
                                    let is_first_run = status.get("first_run").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let has_configured = status.get("has_configured_provider").and_then(|v| v.as_bool()).unwrap_or(false);
                                    first_run.set(is_first_run);
                                    has_configured_provider.set(has_configured);
                                    status_ok = true;
                                    break;
                                }
                                Err(e) => {
                                    tracing::warn!("Status retry failed: {e}");
                                }
                            }
                        }
                    }
                }

                if !status_ok {
                    tracing::warn!("All status retries failed — showing normal UI with connection bar");
                }

                // Send ping
                let _ = crate::messaging::send_ping().await;

                // Resolve window-session binding
                match crate::bindings::nevoflux_api::get_window_session().await {
                    Ok(Some(session_id)) => {
                        tracing::info!("Restored window session: {}", session_id);
                        ctx.session.write().id = session_id.clone();
                        // Load session messages from backend
                        let _ = crate::messaging::send_session_resolve(&session_id).await;
                    }
                    Ok(None) => {
                        tracing::info!("No window session found, using fresh session");
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get window session: {}", e);
                    }
                }

                // Request tab context - use source_tab_id if in maximized mode
                let tab_context_result = if maximize_state.is_maximized {
                    if let Some(source_tab_id) = maximize_state.source_tab_id {
                        tracing::info!("Maximized mode: requesting source tab context for tab {}", source_tab_id);
                        crate::messaging::request_tab_context_for_tab(Some(source_tab_id)).await
                    } else {
                        // Fallback to active tab if no source_tab_id
                        crate::messaging::request_tab_context().await
                    }
                } else {
                    // Normal sidebar mode - get active tab
                    crate::messaging::request_tab_context().await
                };

                // Update tab context from response
                if let Ok(Some(tab_payload)) = tab_context_result {
                    tracing::info!("Got tab context: tab_id={}, url={}", tab_payload.tab_id, tab_payload.url);
                    ctx.tab_context.set(crate::state::TabContext {
                        tab_id: tab_payload.tab_id,
                        zen_sync_id: tab_payload.zen_sync_id,
                        url: tab_payload.url,
                        title: tab_payload.title,
                        favicon_url: tab_payload.favicon_url,
                    });
                }

                // Request session list for history
                ctx.history.write().set_loading();
                let _ = crate::messaging::send_session_list(50, 0).await;

                // Fetch avatar from settings (retry if ContentStore not hydrated yet)
                for attempt in 0..3 {
                    match crate::messaging::fetch_avatar().await {
                        Ok(Some(url)) => {
                            tracing::info!("Loaded avatar from settings (len={}, attempt={})", url.len(), attempt);
                            ctx.avatar_url.set(Some(url));
                            break;
                        }
                        Ok(None) => {
                            if attempt < 2 {
                                tracing::info!("No avatar yet (attempt {}), retrying after delay...", attempt);
                                crate::messaging::sleep_ms(1500).await;
                            } else {
                                tracing::info!("No avatar configured in settings");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to fetch avatar: {}", e);
                            break;
                        }
                    }
                }
            });
        }
    });

    // Sync session_id and target_tab_id to window globals for JS maximize handler
    use_effect(move || {
        let session_id = ctx.session.read().id.clone();
        let target_tab_id = ctx.tab_context.read().tab_id;

        // Set window globals that JS can read
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::set(
                &window,
                &wasm_bindgen::JsValue::from_str("__nevoflux_session_id"),
                &wasm_bindgen::JsValue::from_str(&session_id),
            );
            let _ = js_sys::Reflect::set(
                &window,
                &wasm_bindgen::JsValue::from_str("__nevoflux_target_tab_id"),
                &wasm_bindgen::JsValue::from_f64(target_tab_id as f64),
            );
        }
    });

    // ResizeObserver to detect minimized rail mode (width < 100px)
    use_effect(move || {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::closure::Closure;

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };
        let body = match document.body() {
            Some(b) => b,
            None => return,
        };

        // Use Closure::wrap for FnMut (Signal::set requires &mut self)
        let mut minimized_signal = minimized;
        let mut prev_mini = false;
        let cb = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |entries: JsValue| {
            if let Ok(arr) = entries.dyn_into::<js_sys::Array>() {
                if let Some(entry) = arr.get(0).dyn_into::<js_sys::Object>().ok() {
                    if let Ok(cr) = js_sys::Reflect::get(&entry, &JsValue::from_str("contentRect")) {
                        if let Ok(width) = js_sys::Reflect::get(&cr, &JsValue::from_str("width")) {
                            if let Some(w) = width.as_f64() {
                                let is_mini = w < 100.0;
                                if prev_mini != is_mini {
                                    prev_mini = is_mini;
                                    minimized_signal.set(is_mini);
                                }
                            }
                        }
                    }
                }
            }
        }));

        // Create ResizeObserver via js_sys
        let observer_ctor = js_sys::Reflect::get(
            &js_sys::global(),
            &JsValue::from_str("ResizeObserver"),
        );
        if let Ok(ctor) = observer_ctor {
            if let Ok(ctor_fn) = ctor.dyn_into::<js_sys::Function>() {
                if let Ok(observer) = js_sys::Reflect::construct(&ctor_fn, &js_sys::Array::of1(cb.as_ref())) {
                    if let Ok(observe_fn) = js_sys::Reflect::get(&observer, &JsValue::from_str("observe")) {
                        if let Ok(observe) = observe_fn.dyn_into::<js_sys::Function>() {
                            let _ = observe.call1(&observer, &body);
                        }
                    }
                }
            }
        }

        // Leak the closure so it stays alive for the lifetime of the app
        cb.forget();
    });

    rsx! { {children} }
}

/// Hook to access the application context
///
/// Must be called from within a component that is a descendant of `ContextProvider`.
pub fn use_app_context() -> AppContext {
    use_context::<AppContext>()
}

/// Parse URL parameters for maximize mode
pub fn parse_maximize_params() -> MaximizeState {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();

    let is_maximized = search.contains("mode=maximized");

    let source_tab_id = extract_url_param(&search, "source_tab_id")
        .and_then(|s| s.parse::<i32>().ok());

    let target_tab_id = extract_url_param(&search, "target_tab_id")
        .and_then(|s| s.parse::<i32>().ok());

    MaximizeState {
        is_maximized,
        source_tab_id,
        target_tab_id,
    }
}

/// Extract a single URL parameter value
fn extract_url_param(search: &str, param: &str) -> Option<String> {
    let prefix = format!("{}=", param);
    search
        .trim_start_matches('?')
        .split('&')
        .find(|s| s.starts_with(&prefix))
        .map(|s| s.trim_start_matches(&prefix).to_string())
}

/// Check if mock mode is enabled from URL parameters
pub fn is_mock_mode() -> bool {
    // Check URL parameter first
    let url_mock = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|s| s.contains("mock=true"))
        .unwrap_or(false);

    if url_mock {
        return true;
    }

    // Auto-detect: if browser.runtime API is not available, we're not in extension context
    // Fall back to mock mode for development
    let has_browser_api = js_sys::Reflect::get(
        &js_sys::global(),
        &wasm_bindgen::JsValue::from_str("browser"),
    )
    .ok()
    .map(|b| !b.is_undefined() && !b.is_null())
    .unwrap_or(false);

    if !has_browser_api {
        tracing::warn!("browser.runtime API not available - auto-enabling mock mode");
        tracing::info!("Tip: Add ?mock=true to URL for explicit mock mode");
        return true;
    }

    false
}
