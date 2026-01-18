/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! WebExtension messaging layer for Chat Sidebar
//!
//! Handles communication with the background script using the
//! browser.runtime messaging API.
//!
//! ## Message Flow
//!
//! - Outgoing (to agent): InputMessage (Channel 1)
//! - Incoming (from agent): OutputMessage (Channel 2)
//! - Extension-internal: ExtensionInternalMessage (sidebar <-> background.js)

use crate::components::AgentStatus;
use crate::state::{AppState, ChatMessage, ConnectionStatus, MessageStatus, StreamingMessage, TabContext};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use shared_protocol::{AgentState, ChatMessagePayload, InputMessage, OutputMessage};
use wasm_bindgen::prelude::*;

// ============================================================================
// Extension-Internal Messages
// ============================================================================
// These messages are for sidebar <-> background.js communication only,
// not part of the agent protocol (Channel 1/2).

/// Messages for extension-internal communication (not agent protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ExtensionInternalMessage {
    /// Ping to check connection
    Ping { timestamp: u64 },
    /// Pong response
    Pong { timestamp: u64 },
    /// Request current tab context
    RequestTabContext,
    /// Tab context update from background
    TabContextUpdate(TabContextPayload),
    /// Connection status update
    ConnectionStatus(ConnectionStatusPayload),
}

/// Tab context from background script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabContextPayload {
    pub tab_id: u32,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub is_loading: bool,
}

/// Connection status payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusPayload {
    pub connected: bool,
}

// ============================================================================
// Unified Message Type for Deserialization
// ============================================================================

/// Combined message type for deserializing incoming messages
/// Background.js may send either agent protocol messages or internal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    /// Agent protocol output messages (Channel 2)
    AgentOutput(OutputMessage),
    /// Extension-internal messages
    Internal(ExtensionInternalMessage),
}

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
                match serde_json::from_str::<IncomingMessage>(&s) {
                    Ok(incoming) => {
                        // Use spawn to handle the message asynchronously with mutable signals
                        let messages = messages_clone;
                        let streaming = streaming_clone;
                        let tab = tab_clone;
                        let conn = conn_clone;
                        let agent = agent_clone;
                        handle_incoming_message_sync(incoming, messages, streaming, tab, conn, agent);
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

    // Request connection status from background (extension-internal message)
    let ping = ExtensionInternalMessage::Ping {
        timestamp: js_sys::Date::now() as u64,
    };
    send_internal_message(&ping).await?;

    Ok(())
}

/// Handle incoming messages from background script (sync version for use in closures)
fn handle_incoming_message_sync(
    message: IncomingMessage,
    messages: Signal<Vec<ChatMessage>>,
    streaming_message: Signal<Option<StreamingMessage>>,
    tab_context: Signal<TabContext>,
    connection_status: Signal<ConnectionStatus>,
    agent_status: Signal<AgentStatus>,
) {
    match message {
        // =========================================================================
        // Agent Protocol Messages (Channel 2: OutputMessage)
        // =========================================================================
        IncomingMessage::AgentOutput(output) => {
            handle_agent_output(output, messages, streaming_message, agent_status);
        }

        // =========================================================================
        // Extension-Internal Messages
        // =========================================================================
        IncomingMessage::Internal(internal) => {
            handle_internal_message(internal, tab_context, connection_status);
        }
    }
}

/// Handle agent protocol output messages (Channel 2)
fn handle_agent_output(
    message: OutputMessage,
    mut messages: Signal<Vec<ChatMessage>>,
    mut streaming_message: Signal<Option<StreamingMessage>>,
    mut agent_status: Signal<AgentStatus>,
) {
    match message {
        OutputMessage::StreamChunk(payload) => {
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

        OutputMessage::StreamEnd(payload) => {
            // Complete stream and convert to message
            let final_message = streaming_message.with_mut(|sm| {
                if let Some(ref mut stream) = sm {
                    if stream.stream_id == payload.stream_id {
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

        OutputMessage::Error(payload) => {
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

        OutputMessage::AgentState(payload) => {
            tracing::info!("Agent state update: {:?}", payload.state);

            agent_status.with_mut(|status| {
                status.state = payload.state.clone();

                // Extract tool name if present
                status.current_tool = payload.tool.as_ref().map(|t| t.name.clone());

                // Extract step info if present
                if let Some(ref step) = payload.step {
                    status.step_count = step.current;
                    status.max_steps = step.total;
                }

                // Show status when active, hide when complete
                status.visible = !matches!(payload.state, AgentState::Complete);
            });
        }

        OutputMessage::ContentBlock(_payload) => {
            // TODO: Handle content blocks (code, images, a2ui, etc.)
            tracing::debug!("Received content block - not yet implemented");
        }

        OutputMessage::PermissionRequest(_payload) => {
            // TODO: Handle permission requests (human-in-the-loop)
            tracing::debug!("Received permission request - not yet implemented");
        }

        OutputMessage::AccountStatus(_payload) => {
            // TODO: Handle account status updates
            tracing::debug!("Received account status - not yet implemented");
        }

        OutputMessage::SystemResponse(_payload) => {
            // TODO: Handle system command responses
            tracing::debug!("Received system response - not yet implemented");
        }
    }
}

/// Handle extension-internal messages (sidebar <-> background.js)
fn handle_internal_message(
    message: ExtensionInternalMessage,
    mut tab_context: Signal<TabContext>,
    mut connection_status: Signal<ConnectionStatus>,
) {
    match message {
        ExtensionInternalMessage::TabContextUpdate(ctx) => {
            tracing::info!("Tab context updated: tab_id={}, url={}", ctx.tab_id, ctx.url);

            // Note: Auto-inject disabled temporarily to avoid WASM panic
            // TODO: Fix JsFuture error handling before re-enabling
            // let tab_id = ctx.tab_id;
            // spawn(async move {
            //     inject_content_sidebar(tab_id).await;
            // });

            tab_context.set(TabContext {
                tab_id: ctx.tab_id,
                url: ctx.url,
                title: ctx.title,
                favicon_url: ctx.favicon_url,
                is_loading: ctx.is_loading,
            });
        }

        ExtensionInternalMessage::ConnectionStatus(payload) => {
            if payload.connected {
                connection_status.set(ConnectionStatus::Connected);
            } else {
                connection_status.set(ConnectionStatus::Disconnected);
            }
        }

        ExtensionInternalMessage::Pong { .. } => {
            // Connection confirmed
            connection_status.set(ConnectionStatus::Connected);
        }

        ExtensionInternalMessage::Ping { .. } | ExtensionInternalMessage::RequestTabContext => {
            // These are outgoing-only messages, shouldn't receive them
            tracing::warn!("Received unexpected outgoing message type");
        }
    }
}

/// Send an agent protocol message to the background script (Channel 1: InputMessage)
pub async fn send_agent_message(message: &InputMessage) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(message)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    let js_obj = js_sys::JSON::parse(&json)?;
    let promise = sendMessage(js_obj);

    wasm_bindgen_futures::JsFuture::from(promise).await
}

/// Send an extension-internal message to the background script
pub async fn send_internal_message(message: &ExtensionInternalMessage) -> Result<JsValue, JsValue> {
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

    // Create protocol message (Channel 1: InputMessage)
    let chat_msg = InputMessage::ChatMessage(ChatMessagePayload {
        session_id: app_state.read().session_id.clone(),
        message_id: user_msg.id,
        text,
        attachments: Vec::new(),
    });

    // Send to background
    if let Err(e) = send_agent_message(&chat_msg).await {
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
    let msg = ExtensionInternalMessage::RequestTabContext;
    if let Err(e) = send_internal_message(&msg).await {
        tracing::error!("Failed to request tab context: {:?}", e);
    }
}
