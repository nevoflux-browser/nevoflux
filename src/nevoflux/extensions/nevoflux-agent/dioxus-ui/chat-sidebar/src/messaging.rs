/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! WebExtension messaging layer for Chat Sidebar
//!
//! Handles communication with the background script using the
//! browser.runtime messaging API.

use crate::components::AgentStatus;
use crate::state::{AppState, ChatMessage, ConnectionStatus, MessageStatus, StreamingMessage, TabContext};
use dioxus::prelude::*;
use shared_protocol::{AgentState, ChatMessagePayload, DisplayContentPayload, ExtensionMessage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Browser runtime API
    #[wasm_bindgen(js_namespace = ["browser", "runtime"])]
    fn sendMessage(message: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["browser", "runtime", "onMessage"])]
    fn addListener(callback: &Closure<dyn Fn(JsValue, JsValue, JsValue)>);

    /// Browser tabs API
    #[wasm_bindgen(js_namespace = ["browser", "tabs"])]
    fn query(options: JsValue) -> js_sys::Promise;
}

/// Initialize messaging and set up event listeners
pub async fn init_messaging(
    mut app_state: Signal<AppState>,
    messages: Signal<Vec<ChatMessage>>,
    streaming_message: Signal<Option<StreamingMessage>>,
    tab_context: Signal<TabContext>,
    mut connection_status: Signal<ConnectionStatus>,
    agent_status: Signal<AgentStatus>,
) -> Result<(), JsValue> {
    tracing::info!("Initializing messaging layer");

    // Generate session ID
    let session_id = uuid::Uuid::new_v4().to_string();
    app_state.write().session_id = session_id;

    // Set up message listener using a wrapper that handles the signals
    let messages_clone = messages;
    let streaming_clone = streaming_message;
    let tab_clone = tab_context;
    let conn_clone = connection_status;
    let agent_clone = agent_status;

    let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(move |msg: JsValue, _sender: JsValue, _send_response: JsValue| {
        // Parse the incoming message
        if let Ok(json_str) = js_sys::JSON::stringify(&msg) {
            if let Some(s) = json_str.as_string() {
                match serde_json::from_str::<ExtensionMessage>(&s) {
                    Ok(ext_msg) => {
                        // Use spawn to handle the message asynchronously with mutable signals
                        let messages = messages_clone;
                        let streaming = streaming_clone;
                        let tab = tab_clone;
                        let conn = conn_clone;
                        let agent = agent_clone;
                        handle_incoming_message_sync(ext_msg, messages, streaming, tab, conn, agent);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse message: {}", e);
                    }
                }
            }
        }
    });

    addListener(&closure);

    // Keep closure alive
    closure.forget();

    // Set initial connection status
    connection_status.set(ConnectionStatus::Connecting);

    // Request connection status from background
    let ping = ExtensionMessage::Ping {
        timestamp: js_sys::Date::now() as u64,
    };
    send_to_background(&ping).await?;

    Ok(())
}

/// Handle incoming messages from background script (sync version for use in closures)
fn handle_incoming_message_sync(
    message: ExtensionMessage,
    mut messages: Signal<Vec<ChatMessage>>,
    mut streaming_message: Signal<Option<StreamingMessage>>,
    mut tab_context: Signal<TabContext>,
    mut connection_status: Signal<ConnectionStatus>,
    mut agent_status: Signal<AgentStatus>,
) {
    match message {
        ExtensionMessage::StreamChunk(payload) => {
            // Handle streaming text
            streaming_message.with_mut(|sm| {
                if let Some(ref mut stream) = sm {
                    if stream.stream_id == payload.stream_id {
                        stream.append(&payload.delta);
                    }
                } else {
                    // Start new stream
                    let mut new_stream = StreamingMessage::new(&payload.stream_id);
                    new_stream.append(&payload.delta);
                    *sm = Some(new_stream);
                }
            });
        }

        ExtensionMessage::StreamEnd { stream_id, .. } => {
            // Complete stream and convert to message
            let final_message = streaming_message.with_mut(|sm| {
                if let Some(ref mut stream) = sm {
                    if stream.stream_id == stream_id {
                        stream.complete();
                        return Some(stream.clone().into_message());
                    }
                }
                None
            });

            if let Some(msg) = final_message {
                messages.write().push(msg);
                streaming_message.set(None);
            }
        }

        ExtensionMessage::AgentError(payload) => {
            // Add error message to chat
            let error_msg = ChatMessage::system(format!("Error: {}", payload.message));
            messages.write().push(error_msg);

            // Clear streaming if any
            streaming_message.set(None);

            // Update agent status to error
            agent_status.with_mut(|status| {
                status.state = AgentState::Error;
                status.message = Some(payload.message);
                status.visible = true;
            });
        }

        ExtensionMessage::TabContextUpdate(ctx) => {
            tracing::info!("Tab context updated: tab_id={}, url={}", ctx.tab_id, ctx.url);

            // Auto-inject Content Sidebar when tab context updates
            let tab_id = ctx.tab_id;
            spawn(async move {
                inject_content_sidebar(tab_id).await;
            });

            tab_context.set(TabContext::from(ctx));
        }

        ExtensionMessage::ConnectionStatus(payload) => {
            if payload.connected {
                connection_status.set(ConnectionStatus::Connected);
            } else {
                connection_status.set(ConnectionStatus::Disconnected);
            }
        }

        ExtensionMessage::Pong { .. } => {
            // Connection confirmed
            connection_status.set(ConnectionStatus::Connected);
        }

        // =========================================================================
        // Agent State Updates (Computer Use)
        // =========================================================================
        ExtensionMessage::AgentStateUpdate(payload) => {
            tracing::info!("Agent state update: {:?}", payload.state);

            agent_status.with_mut(|status| {
                status.state = payload.state.clone();
                status.current_tool = payload.current_tool;
                status.step_count = payload.step_count;
                status.max_steps = payload.max_steps;
                status.message = payload.message;

                // Show status when active, hide when complete
                status.visible = !matches!(payload.state, AgentState::Complete);
            });
        }

        _ => {
            tracing::debug!("Unhandled message type: {:?}", message);
        }
    }
}

/// Send a message to the background script
pub async fn send_to_background(message: &ExtensionMessage) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(message)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    let js_obj = js_sys::JSON::parse(&json)?;
    let promise = sendMessage(js_obj);

    wasm_bindgen_futures::JsFuture::from(promise).await
}

/// Send a chat message
pub async fn send_chat_message(
    text: String,
    mut messages: Signal<Vec<ChatMessage>>,
    app_state: Signal<AppState>,
    mut agent_status: Signal<AgentStatus>,
) {
    // Add user message to UI immediately
    let user_msg = ChatMessage::user(&text);
    messages.write().push(user_msg.clone());

    // Show agent thinking state
    agent_status.with_mut(|status| {
        status.state = AgentState::Thinking;
        status.step_count = 0;
        status.current_tool = None;
        status.message = None;
        status.visible = true;
    });

    // Create protocol message
    let chat_msg = ExtensionMessage::ChatMessage(ChatMessagePayload {
        session_id: app_state.read().session_id.clone(),
        message_id: user_msg.id,
        text,
        attachments: Vec::new(),
        include_page_context: false,
        page_context: None,
    });

    // Send to background
    if let Err(e) = send_to_background(&chat_msg).await {
        tracing::error!("Failed to send chat message: {:?}", e);
        // Add error indicator
        let mut msgs = messages.write();
        if let Some(last) = msgs.last_mut() {
            last.status = MessageStatus::Error;
        }

        // Update agent status to error
        agent_status.with_mut(|status| {
            status.state = AgentState::Error;
            status.message = Some("Failed to send message".to_string());
        });
    }
}

/// Request current tab context
pub async fn request_tab_context() {
    let msg = ExtensionMessage::RequestTabContext;
    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to request tab context: {:?}", e);
    }
}

/// Send content to Content Sidebar
pub async fn send_to_content_sidebar(content: DisplayContentPayload) {
    let msg = ExtensionMessage::DisplayContent(content);
    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to send to content sidebar: {:?}", e);
    }
}

/// Request Content Sidebar injection
pub async fn inject_content_sidebar(tab_id: u32) {
    let msg = ExtensionMessage::InjectContentSidebar { tab_id };
    if let Err(e) = send_to_background(&msg).await {
        tracing::error!("Failed to inject content sidebar: {:?}", e);
    }
}
