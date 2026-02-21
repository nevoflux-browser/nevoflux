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
    McpConfigState, Message, PendingFilePick, PermissionRequestState, PickedFile, SessionState,
    SkillItem, StreamingState, TabContext,
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
    let chat_mode = use_signal(ChatMode::default);
    let available_skills = use_signal(Vec::<SkillItem>::new);
    let maximize = use_signal(parse_maximize_params);
    let pending_plan = use_signal(|| false);
    let show_history_panel = use_signal(|| false);
    let live_tools = use_signal(Vec::<LiveToolEntry>::new);
    let pending_tool_auth = use_signal(|| None::<ToolAuthRequest>);
    let avatar_url = use_signal(|| None::<String>);

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
        chat_mode,
        available_skills,
        maximize,
        pending_plan,
        show_history_panel,
        live_tools,
        pending_tool_auth,
        avatar_url,
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
