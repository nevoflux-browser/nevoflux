/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Messaging layer for Content Sidebar
//!
//! Handles communication with the background script via
//! browser.runtime messaging API.

use crate::page_context::extract_page_context;
use crate::state::{ContentSidebarState, DisplayContent};
use crate::tool_executor::{execute_tool, ToolExecutionResult};
use dioxus::prelude::*;
use shared_protocol::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    /// Browser runtime API
    #[wasm_bindgen(js_namespace = ["browser", "runtime"])]
    fn sendMessage(message: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["browser", "runtime", "onMessage"])]
    fn addListener(callback: &Closure<dyn Fn(JsValue, JsValue, JsValue)>);
}

/// Initialize messaging for Content Sidebar
pub async fn init_content_messaging(
    sidebar_state: Signal<ContentSidebarState>,
    display_content: Signal<Option<DisplayContent>>,
    is_visible: Signal<bool>,
) -> Result<(), JsValue> {
    tracing::info!("Initializing Content Sidebar messaging");

    // Clone signals for closure
    let state_clone = sidebar_state;
    let content_clone = display_content;
    let visible_clone = is_visible;

    // Set up custom event listener for messages from content-bootstrap.js
    // This is more reliable than browser.runtime.onMessage in WASM context
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;

    web_sys::console::log_1(&"[NevoFlux WASM] Setting up nevoflux-message event listener".into());

    let closure = Closure::<dyn Fn(web_sys::CustomEvent)>::new(move |event: web_sys::CustomEvent| {
        // Get the message from event.detail
        let detail = event.detail();

        web_sys::console::log_1(&"[NevoFlux WASM] Received nevoflux-message event!".into());
        web_sys::console::log_1(&detail);

        // Parse the incoming message
        if let Ok(json_str) = js_sys::JSON::stringify(&detail) {
            if let Some(s) = json_str.as_string() {
                web_sys::console::log_1(&format!("[NevoFlux WASM] Parsing message: {}", &s[..s.len().min(200)]).into());
                match serde_json::from_str::<ExtensionMessage>(&s) {
                    Ok(ext_msg) => {
                        web_sys::console::log_1(&"[NevoFlux WASM] Successfully parsed, handling...".into());
                        handle_message_sync(
                            ext_msg,
                            state_clone,
                            content_clone,
                            visible_clone,
                        );
                    }
                    Err(e) => {
                        web_sys::console::log_1(&format!("[NevoFlux WASM] Parse error: {}", e).into());
                    }
                }
            }
        }
    });

    window.add_event_listener_with_callback(
        "nevoflux-message",
        closure.as_ref().unchecked_ref(),
    )?;
    closure.forget();

    web_sys::console::log_1(&"[NevoFlux WASM] Event listener attached successfully".into());

    Ok(())
}

/// Handle incoming messages (sync version for closures)
fn handle_message_sync(
    message: ExtensionMessage,
    mut sidebar_state: Signal<ContentSidebarState>,
    mut display_content: Signal<Option<DisplayContent>>,
    mut is_visible: Signal<bool>,
) {
    match message {
        ExtensionMessage::DisplayContent(payload) => {
            tracing::info!("Received display content request");

            // Convert to DisplayContent
            let content = DisplayContent {
                content_type: payload.content_type,
                content: payload.content,
                title: payload.title,
                session_id: payload.session_id,
            };

            display_content.set(Some(content));
            sidebar_state.set(ContentSidebarState::DisplayingContent);
            is_visible.set(true);
        }

        ExtensionMessage::ClearContent { .. } => {
            tracing::info!("Clearing content sidebar");
            display_content.set(None);
            sidebar_state.set(ContentSidebarState::Default);
        }

        ExtensionMessage::HighlightElement(payload) => {
            tracing::info!("Highlighting element: {}", payload.selector);
            highlight_element(&payload.selector, &payload.style);
        }

        ExtensionMessage::ClearHighlight { .. } => {
            tracing::info!("Clearing highlights");
            clear_all_highlights();
        }

        ExtensionMessage::Ping { timestamp } => {
            // Respond with pong
            spawn(async move {
                let pong = ExtensionMessage::Pong { timestamp };
                let _ = send_to_background(&pong).await;
            });
        }

        // =========================================================================
        // Page Context (Computer Use)
        // =========================================================================
        ExtensionMessage::RequestPageContext { session_id } => {
            tracing::info!("Received page context request for session {}", session_id);
            spawn(async move {
                handle_page_context_request(&session_id).await;
            });
        }

        // =========================================================================
        // Tool Execution (Computer Use)
        // =========================================================================
        ExtensionMessage::ToolCall(payload) => {
            web_sys::console::log_1(&format!("[NevoFlux WASM] Received tool call: {} ({})", payload.tool_name, payload.call_id).into());
            let call_id = payload.call_id.clone();
            let session_id = payload.session_id.clone();
            let tool_name = payload.tool_name.clone();
            let parameters = payload.parameters.clone();
            let show_feedback = payload.show_feedback;

            spawn(async move {
                handle_tool_call(&call_id, &session_id, &tool_name, &parameters, show_feedback).await;
            });
        }

        _ => {
            tracing::debug!("Unhandled message type in Content Sidebar");
        }
    }
}

/// Handle page context request
async fn handle_page_context_request(session_id: &str) {
    let context = extract_page_context();
    let tab_id = get_current_tab_id();

    let response = ExtensionMessage::PageContextResponse(PageContextPayload {
        session_id: session_id.to_string(),
        tab_id,
        context,
    });

    if let Err(e) = send_to_background(&response).await {
        tracing::error!("Failed to send page context: {:?}", e);
    }
}

/// Handle tool call request
async fn handle_tool_call(
    call_id: &str,
    session_id: &str,
    tool_name: &str,
    parameters: &serde_json::Value,
    show_feedback: bool,
) {
    web_sys::console::log_1(&format!("[NevoFlux WASM] Executing tool: {} with params: {:?}", tool_name, parameters).into());

    // Show visual feedback if requested
    if show_feedback {
        // Highlight target element if selector is present
        if let Some(selector) = parameters.get("selector").and_then(|v| v.as_str()) {
            web_sys::console::log_1(&format!("[NevoFlux WASM] Highlighting selector: {}", selector).into());
            highlight_element(selector, &HighlightStyle::Outline);
        }
    }

    // Execute the tool
    web_sys::console::log_1(&"[NevoFlux WASM] Calling execute_tool...".into());
    let result = execute_tool(tool_name, parameters);
    web_sys::console::log_1(&format!("[NevoFlux WASM] Tool result: success={}, error={:?}", result.success, result.error).into());

    // Clear highlight after execution
    if show_feedback {
        clear_all_highlights();
    }

    // Send result back
    let response = ExtensionMessage::ToolResult(ToolResultPayload {
        call_id: call_id.to_string(),
        session_id: session_id.to_string(),
        success: result.success,
        result: result.result,
        error: result.error,
        screenshot: None,
    });

    web_sys::console::log_1(&"[NevoFlux WASM] Sending tool result back to background...".into());
    if let Err(e) = send_to_background(&response).await {
        tracing::error!("Failed to send tool result: {:?}", e);
    }
}

/// Send message to background script
pub async fn send_to_background(message: &ExtensionMessage) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(message)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    let js_obj = js_sys::JSON::parse(&json)?;
    let promise = sendMessage(js_obj);

    wasm_bindgen_futures::JsFuture::from(promise).await
}

/// Report that Content Sidebar is ready
pub async fn report_ready() {
    let tab_id = get_current_tab_id();
    let msg = ExtensionMessage::ContentSidebarReady { tab_id };

    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to report ready: {:?}", e);
    }
}

/// Report current URL to Chat Sidebar
pub async fn report_current_url() {
    let tab_id = get_current_tab_id();
    let url = get_current_url();
    let title = get_current_title();

    let msg = ExtensionMessage::ContentUrlReport(ContentUrlPayload {
        tab_id,
        url,
        title,
    });

    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to report URL: {:?}", e);
    }
}

/// Report element click to Chat Sidebar
pub async fn report_element_click(
    tag: String,
    id: Option<String>,
    class: Option<String>,
    text: Option<String>,
) {
    let tab_id = get_current_tab_id();

    let msg = ExtensionMessage::ContentElementClick(ContentElementClickPayload {
        tab_id,
        element_tag: tag,
        element_id: id,
        element_class: class,
        element_text: text,
    });

    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to report click: {:?}", e);
    }
}

/// Get current tab ID (stub - will be provided by content script wrapper)
fn get_current_tab_id() -> u32 {
    // In content script context, we don't have direct tab ID access
    // This will be set by the wrapper script
    0
}

/// Get current page URL
fn get_current_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_default()
}

/// Get current page title
fn get_current_title() -> String {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.title())
        .unwrap_or_default()
}

/// Highlight an element on the page
fn highlight_element(selector: &str, style: &HighlightStyle) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Ok(Some(element)) = document.query_selector(selector) {
                if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
                    let el_style = html_el.style();

                    match style {
                        HighlightStyle::Outline => {
                            let _ = el_style.set_property("outline", "3px solid #6366f1");
                            let _ = el_style.set_property("outline-offset", "2px");
                        }
                        HighlightStyle::Overlay => {
                            let _ = el_style.set_property("background-color", "rgba(99, 102, 241, 0.2)");
                        }
                        HighlightStyle::Pulse => {
                            let _ = el_style.set_property("animation", "nevoflux-pulse 1.5s infinite");
                        }
                    }

                    // Add data attribute for tracking
                    let _ = element.set_attribute("data-nevoflux-highlight", "true");
                }
            }
        }
    }
}

/// Clear all element highlights
fn clear_all_highlights() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Ok(elements) = document.query_selector_all("[data-nevoflux-highlight]") {
                for i in 0..elements.length() {
                    if let Some(node) = elements.get(i) {
                        // Cast Node to Element
                        if let Some(element) = node.dyn_ref::<web_sys::Element>() {
                            if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
                                let style = html_el.style();
                                let _ = style.remove_property("outline");
                                let _ = style.remove_property("outline-offset");
                                let _ = style.remove_property("background-color");
                                let _ = style.remove_property("animation");
                            }
                            let _ = element.remove_attribute("data-nevoflux-highlight");
                        }
                    }
                }
            }
        }
    }
}

use shared_protocol::HighlightStyle;
