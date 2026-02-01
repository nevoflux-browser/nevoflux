/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles incoming messages from background script
//!
//! Receives ChatMessage from Agent (via background.js) and InternalMessage
//! from background.js itself.

use dioxus::prelude::*;
use crate::context::AppContext;
use crate::messaging::bridge::*;
use crate::messaging::sender::*;
use crate::state::*;
use shared_protocol::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;

/// Combined incoming message type
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    /// Agent protocol message (Chat channel - bidirectional)
    AgentMessage(ChatMessage),
    /// Extension internal message
    Internal(InternalMessage),
}

/// Initialize message listener
pub fn init_message_listener(ctx: AppContext) {
    let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue) -> JsValue>::new(
        move |msg: JsValue, _sender: JsValue, _send_response: JsValue| {
            // Parse incoming message
            match from_js_value::<IncomingMessage>(msg) {
                Ok(incoming) => {
                    handle_incoming(ctx, incoming);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse message: {}", e);
                }
            }

            // Return undefined (no async response)
            JsValue::UNDEFINED
        },
    );

    runtime_add_listener(&closure);

    // Keep closure alive for lifetime of app
    closure.forget();
}

/// Route incoming message to appropriate handler
fn handle_incoming(ctx: AppContext, message: IncomingMessage) {
    match message {
        IncomingMessage::AgentMessage(chat_msg) => {
            handle_chat_message(ctx, chat_msg);
        }
        IncomingMessage::Internal(internal) => {
            handle_internal_message(ctx, internal);
        }
    }
}

/// Handle Chat channel messages (bidirectional Agent <-> Sidebar)
///
/// Most messages from Agent are ToSidebar direction, but we also handle
/// BrowserToolRequest specially by executing via bg:exec_tool and sending response.
fn handle_chat_message(ctx: AppContext, message: ChatMessage) {
    match message {
        // ========== Agent -> Sidebar messages ==========
        ChatMessage::StreamChunk(payload) => {
            handle_stream_chunk(ctx, payload);
        }
        ChatMessage::StreamEnd(payload) => {
            handle_stream_end(ctx, payload);
        }
        ChatMessage::AgentState(payload) => {
            handle_agent_state(ctx, payload);
        }
        ChatMessage::PermissionRequest(payload) => {
            handle_permission_request(ctx, payload);
        }
        ChatMessage::Error(payload) => {
            handle_error(ctx, payload);
        }
        ChatMessage::ContentBlock(payload) => {
            handle_content_block(ctx, payload);
        }
        ChatMessage::AccountStatus(_payload) => {
            // P2: Handle account status
            tracing::debug!("Received account status - not yet implemented");
        }
        ChatMessage::SystemResponse(payload) => {
            handle_system_response(ctx, payload);
        }
        ChatMessage::BrowserToolRequest(payload) => {
            // Execute browser tool via bg:exec_tool and send response back to agent
            handle_browser_tool_request(payload);
        }
        ChatMessage::PickFilesResponse(_payload) => {
            // Not used - file.pick responses come via SystemResponse
            tracing::debug!("Received PickFilesResponse - using SystemResponse instead");
        }

        // ========== Sidebar -> Agent messages (should not be received) ==========
        ChatMessage::ChatMessage(_) |
        ChatMessage::SkillCommand(_) |
        ChatMessage::StopGeneration(_) |
        ChatMessage::Cancel(_) |
        ChatMessage::PermissionResponse(_) |
        ChatMessage::PluginCommand(_) |
        ChatMessage::SystemCommand(_) |
        ChatMessage::BrowserToolResponse(_) |
        ChatMessage::PickFilesRequest(_) => {
            tracing::warn!("Received unexpected ToAgent message in sidebar");
        }
    }
}

/// Handle BrowserToolRequest from Agent
///
/// 1. Call bg:exec_tool to execute the browser tool
/// 2. Build response payload from result
/// 3. Send response via bg:send_to_agent
fn handle_browser_tool_request(payload: BrowserToolRequestPayload) {
    tracing::info!(
        "Executing browser tool: {} (action={:?}, tab_id={:?})",
        payload.request_id,
        payload.action,
        payload.tab_id
    );

    // Use spawn_local for better error handling in WASM context
    // Clone all data needed for the async block upfront
    let request_id = payload.request_id.clone();
    let session_id = payload.session_id.clone();
    let action = payload.action.clone();
    let params = payload.params.clone();
    let tab_id = payload.tab_id;
    let timeout_ms = payload.timeout_ms;

    tracing::debug!("Spawning async task for browser tool...");

    spawn_local(async move {
        tracing::debug!("Async task started for browser tool: {}", request_id);

        // Reconstruct payload inside async block to avoid closure capture issues
        let payload = BrowserToolRequestPayload {
            request_id: request_id.clone(),
            session_id: session_id.clone(),
            action,
            params,
            tab_id,
            timeout_ms,
        };

        execute_browser_tool_and_respond(payload, request_id, session_id).await;
    });

    tracing::debug!("Browser tool task spawned");
}

/// Execute browser tool and send response - isolated to prevent panic propagation
async fn execute_browser_tool_and_respond(
    payload: BrowserToolRequestPayload,
    request_id: String,
    session_id: String,
) {
    tracing::debug!("execute_browser_tool_and_respond starting...");

    // Execute browser tool via bg:exec_tool
    let result = crate::messaging::exec_browser_tool(payload).await;

    tracing::debug!("exec_browser_tool returned");

    match result {
        Ok(response) => {
            tracing::info!(
                "Browser tool {} completed (success={})",
                response.request_id,
                response.success
            );

            // Send response back to agent via bg:send_to_agent
            if let Err(e) = crate::messaging::send_browser_tool_response(response).await {
                tracing::error!("Failed to send browser tool response to agent: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("Browser tool execution failed: {}", e);

            // Send error response back to agent
            let error_response = BrowserToolResponsePayload {
                request_id,
                session_id,
                success: false,
                result: None,
                error: Some(BrowserToolError {
                    code: -1,
                    message: e,
                    recoverable: true,
                }),
            };

            if let Err(e) = crate::messaging::send_browser_tool_response(error_response).await {
                tracing::error!("Failed to send browser tool error response: {}", e);
            }
        }
    }

    tracing::debug!("execute_browser_tool_and_respond completed");
}

// ============================================
// Stream Handlers
// ============================================

fn handle_stream_chunk(mut ctx: AppContext, payload: StreamChunkPayload) {
    // Capture session title if provided (generated from first message)
    if let Some(ref title) = payload.session_title {
        tracing::info!("Received session title: {}", title);
        ctx.session.write().set_title(title.clone());

        // Refresh history to show the new title
        ctx.history.write().set_loading();
        spawn_local(async move {
            let _ = crate::messaging::send_session_list(50, 0).await;
        });
    }

    // New protocol: content + done flag (no stream_id needed)
    if payload.done {
        // Stream complete - finalize accumulated content into a message
        let final_content = {
            let mut streaming = ctx.streaming.write();
            if let Some(mut stream) = streaming.take() {
                // Append any final content from the done payload
                if !payload.content.is_empty() {
                    stream.content.push_str(&payload.content);
                }
                stream.content
            } else {
                // No streaming state - use payload content directly
                payload.content.clone()
            }
        };

        // Only add message if there's actual content
        if !final_content.is_empty() {
            let message = Message::assistant_markdown(final_content);
            ctx.messages.write().push(message);
        }

        // Clear agent status
        ctx.agent_status.write().hide();

        // Handle tool calls if present
        if !payload.tool_calls.is_empty() {
            tracing::debug!("Received {} tool calls", payload.tool_calls.len());
            // TODO: Handle tool calls
        }
    } else {
        // Streaming in progress - accumulate content
        let mut streaming = ctx.streaming.write();
        match &mut *streaming {
            Some(ref mut stream) => {
                stream.content.push_str(&payload.content);
            }
            None => {
                // Start new stream
                *streaming = Some(StreamingState {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: payload.content,
                    format: StreamFormat::Markdown,
                });
            }
        }
    }
}

fn handle_stream_end(mut ctx: AppContext, payload: StreamEndPayload) {
    // Finalize stream into message
    let final_content = {
        let mut streaming = ctx.streaming.write();
        if let Some(stream) = streaming.take() {
            if stream.id == payload.stream_id {
                Some((stream.content, stream.format))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((content, format)) = final_content {
        let message = match format {
            StreamFormat::Markdown => Message::assistant_markdown(content),
            _ => Message::assistant(content),
        };

        ctx.messages.write().push(message);
    }
}

// ============================================
// Agent State Handler
// ============================================

fn handle_agent_state(mut ctx: AppContext, payload: AgentStatePayload) {
    let mut status = ctx.agent_status.write();
    status.state = payload.state.clone();

    // Update tool info
    status.current_tool = payload.tool.map(|t| ToolDisplayInfo {
        name: t.name.clone(),
        icon: get_tool_icon(&t.name),
        description: t.target,
    });

    // Update step info
    status.step = payload.step.map(|s| StepDisplayInfo {
        current: s.current,
        total: s.total,
    });

    // Visibility
    status.visible = !matches!(payload.state, AgentState::Complete);
}

// ============================================
// Permission Request Handler
// ============================================

fn handle_permission_request(mut ctx: AppContext, payload: PermissionRequestPayload) {
    // Set agent to waiting state
    {
        let mut status = ctx.agent_status.write();
        status.state = AgentState::Waiting;
        status.visible = true;
    }

    // Show permission dialog
    ctx.permission_request.set(Some(PermissionRequestState {
        request_id: payload.request_id,
        resource_type: payload.resource_type,
        action: payload.action,
        resource: payload.resource,
        requester: payload.requester.name,
        reason: payload.reason,
        timeout_ms: payload.timeout_ms,
        created_at: js_sys::Date::now() as u64,
    }));
}

// ============================================
// Error Handler
// ============================================

fn handle_error(mut ctx: AppContext, payload: ErrorPayload) {
    // Update agent status
    {
        let mut status = ctx.agent_status.write();
        status.state = AgentState::Error;
        status.error_message = Some(payload.message.clone());
        status.visible = true;
    }

    // Clear any streaming
    ctx.streaming.set(None);

    // Add error message to chat
    ctx.messages.write().push(Message::error(
        payload.code,
        payload.message,
        payload.recoverable,
    ));
}

// ============================================
// Content Block Handler
// ============================================

fn handle_content_block(mut ctx: AppContext, payload: ContentBlockPayload) {
    let message = match payload.content_type {
        ContentType::Text => Message::assistant(
            payload.content.as_str().unwrap_or_default().to_string(),
        ),
        ContentType::Markdown => Message::assistant_markdown(
            payload.content.as_str().unwrap_or_default().to_string(),
        ),
        ContentType::Code => {
            let language = payload
                .metadata
                .as_ref()
                .and_then(|m| m.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("text")
                .to_string();
            Message::code(language, payload.content.as_str().unwrap_or_default())
        }
        ContentType::A2ui | ContentType::Image => {
            // P2: placeholder for now
            Message::assistant("[Content block not yet supported]")
        }
    };

    ctx.messages.write().push(message);
}

// ============================================
// System Response Handler
// ============================================

fn handle_system_response(mut ctx: AppContext, payload: SystemResponsePayload) {
    tracing::debug!("Received system response for command: {}", payload.command);

    match payload.command.as_str() {
        "session.resolve" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_resolve_response(ctx, data);
                }
            } else {
                tracing::error!("session.resolve failed: {:?}", payload.error);
            }
        }
        "session.list" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_list_response(ctx, data);
                }
            } else {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!("session.list failed: {}", error_msg);
                ctx.history.write().set_error(error_msg);
            }
        }
        "session.clone" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_clone_response(ctx, data);
                }
            } else {
                tracing::error!("session.clone failed: {:?}", payload.error);
            }
        }
        // MCP configuration commands
        "mcp.list" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_mcp_list_response(ctx, data);
                }
            } else {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!("mcp.list failed: {}", error_msg);
                ctx.mcp_config.write().set_error(error_msg);
            }
        }
        "mcp.add" | "mcp.update" | "mcp.delete" => {
            handle_mcp_mutation_response(ctx, payload);
        }
        "mcp.test" => {
            if let Some(data) = payload.data {
                handle_mcp_test_response(ctx, data);
            } else if !payload.success {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Test failed".to_string());
                tracing::error!("mcp.test failed: {}", error_msg);
            }
        }
        "mcp.connect" | "mcp.disconnect" => {
            if let Some(data) = payload.data {
                handle_mcp_connection_response(ctx, data);
            } else if !payload.success {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Connection operation failed".to_string());
                tracing::error!("{} failed: {}", payload.command, error_msg);
            }
        }
        // File picker response
        "file.pick" => {
            if let Some(data) = payload.data {
                handle_file_pick_response(ctx, data);
            } else {
                // Cancelled or error
                ctx.pending_file_pick.write().take();
                if !payload.success {
                    let error_msg = payload.error
                        .map(|e| e.message)
                        .unwrap_or_else(|| "File picker failed".to_string());
                    tracing::error!("file.pick failed: {}", error_msg);
                }
            }
        }
        _ => {
            tracing::debug!("Unhandled system response: {}", payload.command);
        }
    }
}

/// Handle file.pick response - add picked files to context
fn handle_file_pick_response(mut ctx: AppContext, data: serde_json::Value) {
    tracing::info!("Received file.pick response: {:?}", data);

    // Check if cancelled
    let cancelled = data.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false);
    if cancelled {
        tracing::debug!("File picker was cancelled by user");
        ctx.pending_file_pick.write().take();
        return;
    }

    // Parse files array
    let files = data.get("files").and_then(|v| v.as_array());
    if let Some(files_arr) = files {
        let picked_files: Vec<crate::state::PickedFile> = files_arr.iter().filter_map(|f| {
            let path = f.get("path").and_then(|v| v.as_str())?;
            Some(crate::state::PickedFile {
                path: path.to_string(),
                is_directory: f.get("is_directory").and_then(|v| v.as_bool()).unwrap_or(false),
                size: f.get("size").and_then(|v| v.as_u64()),
                modified: f.get("modified").and_then(|v| v.as_u64()),
                mime_type: f.get("mime_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
            })
        }).collect();

        tracing::info!("Picked {} files", picked_files.len());
        ctx.picked_files.write().extend(picked_files);
    }

    ctx.pending_file_pick.write().take();
}

/// Handle session.resolve response - load messages into chat
fn handle_session_resolve_response(mut ctx: AppContext, data: serde_json::Value) {
    // Parse messages from response
    let messages_json = data.get("messages").and_then(|m| m.as_array());

    if let Some(messages_arr) = messages_json {
        // Clear current messages and load new ones
        let mut new_messages = Vec::new();

        for msg_json in messages_arr {
            if let (Some(id), Some(role), Some(content)) = (
                msg_json.get("id").and_then(|v| v.as_str()),
                msg_json.get("role").and_then(|v| v.as_str()),
                msg_json.get("content").and_then(|v| v.as_str()),
            ) {
                let message = match role {
                    "user" => Message::user(content.to_string()),
                    "assistant" => Message::assistant_markdown(content.to_string()),
                    _ => continue,
                };
                // Set the correct ID
                let mut msg = message;
                msg.id = id.to_string();
                new_messages.push(msg);
            }
        }

        ctx.messages.set(new_messages);
        tracing::info!("Loaded {} messages from session", ctx.messages.read().len());
    } else {
        // No messages - clear and show welcome screen
        ctx.messages.set(Vec::new());
        tracing::info!("Session has no messages, showing welcome screen");
    }
}

/// Handle session.clone response - load cloned messages into chat
fn handle_session_clone_response(ctx: AppContext, data: serde_json::Value) {
    // Same logic as session.resolve
    handle_session_resolve_response(ctx, data);
}

/// Handle session.list response - populate history state
fn handle_session_list_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::SessionSummary;

    let sessions_json = data.get("sessions").and_then(|s| s.as_array());
    let total = data.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as u32;

    if let Some(sessions_arr) = sessions_json {
        let mut sessions = Vec::new();

        for session_json in sessions_arr {
            let id = session_json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let title = session_json.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
            let updated_at = session_json.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0);
            let message_count = session_json.get("message_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            // Skip sessions with no messages
            if message_count == 0 {
                continue;
            }

            sessions.push(SessionSummary {
                id,
                title,
                updated_at,
                message_count,
            });
        }

        ctx.history.write().set_sessions(sessions, total);
        tracing::info!("Loaded {} sessions into history", ctx.history.read().sessions.len());
    } else {
        ctx.history.write().set_sessions(Vec::new(), 0);
        tracing::info!("No sessions in history");
    }
}

// ============================================
// Internal Message Handler
// ============================================

fn handle_internal_message(mut ctx: AppContext, message: InternalMessage) {
    match message {
        InternalMessage::Pong { .. } => {
            ctx.connection.set(ConnectionState::Connected);
        }
        InternalMessage::ConnectionStatus { connected } => {
            if connected {
                ctx.connection.set(ConnectionState::Connected);
            } else {
                ctx.connection.set(ConnectionState::Disconnected);
            }
        }
        InternalMessage::TabContextUpdate(payload) => {
            let old_zen_sync_id = ctx.tab_context.read().zen_sync_id.clone();
            let new_zen_sync_id = payload.zen_sync_id.clone();

            // Update tab context
            ctx.tab_context.set(TabContext {
                tab_id: payload.tab_id,
                zen_sync_id: payload.zen_sync_id,
                url: payload.url,
                title: payload.title,
                favicon_url: payload.favicon_url,
            });

            // If zen_sync_id changed, resolve the new session
            if new_zen_sync_id != old_zen_sync_id {
                // Reset UI state when switching tabs
                ctx.agent_status.write().hide();
                ctx.streaming.set(None);
                ctx.permission_request.set(None);

                if let Some(ref session_id) = new_zen_sync_id {
                    tracing::info!("Tab changed, resolving session: {}", session_id);
                    let session_id = session_id.clone();
                    spawn_local(async move {
                        if let Err(e) = crate::messaging::sender::send_session_resolve(&session_id).await {
                            tracing::error!("Failed to resolve session: {}", e);
                        }
                    });
                } else {
                    // No session_id - clear messages and show welcome
                    ctx.messages.set(Vec::new());
                }
            }
        }
        InternalMessage::AskUserRequest(payload) => {
            tracing::info!("Received AskUser request: {}", payload.request_id);
            handle_ask_user_request(ctx, payload);
        }
        // Outgoing-only messages (shouldn't receive)
        InternalMessage::Ping { .. } => {
            tracing::warn!("Received unexpected outgoing Ping message");
        }
    }
}

// ============================================
// AskUser Request Handler
// ============================================

fn handle_ask_user_request(mut ctx: AppContext, payload: crate::messaging::sender::AskUserRequestPayload) {
    use crate::state::AskUserState;

    // Set the ask_user state to trigger the dialog
    ctx.ask_user.set(Some(AskUserState::new(
        payload.request_id,
        payload.question,
        payload.options,
        payload.allow_custom,
        payload.timeout_ms,
    )));

    tracing::debug!("AskUser dialog state set, waiting for user response");
}

// ============================================
// MCP Response Handlers
// ============================================

/// Handle mcp.list response - populate MCP config state with servers
fn handle_mcp_list_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::{McpConnectionStatus, McpServer, McpServerConfig};

    let servers_json = data.get("servers").and_then(|s| s.as_array());
    let connected_json = data.get("connected").and_then(|c| c.as_array());

    // Build set of connected server names
    let connected_names: std::collections::HashSet<String> = connected_json
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if let Some(servers_arr) = servers_json {
        let mut servers = Vec::new();

        for server_json in servers_arr {
            let name = server_json.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let command = server_json.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let args: Vec<String> = server_json.get("args")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let enabled = server_json.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let env: Vec<(String, String)> = server_json.get("env")
                .and_then(|e| e.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
                .unwrap_or_default();

            let config = McpServerConfig {
                name: name.clone(),
                command,
                args,
                enabled,
                env,
            };

            let status = if connected_names.contains(&name) {
                McpConnectionStatus::Connected
            } else {
                McpConnectionStatus::Disconnected
            };

            servers.push(McpServer::with_status(config, status));
        }

        ctx.mcp_config.write().set_servers(servers);
        tracing::info!("Loaded {} MCP servers", ctx.mcp_config.read().servers.len());
    } else {
        ctx.mcp_config.write().set_servers(Vec::new());
        tracing::info!("No MCP servers configured");
    }
}

/// Handle mcp.add, mcp.update, mcp.delete responses
fn handle_mcp_mutation_response(mut ctx: AppContext, payload: SystemResponsePayload) {
    if payload.success {
        tracing::info!("{} succeeded", payload.command);
        // Refresh the server list
        ctx.mcp_config.write().set_loading();
        ctx.mcp_config.write().cancel_edit();
        spawn_local(async move {
            let _ = crate::messaging::send_mcp_list().await;
        });
    } else {
        let error_msg = payload.error
            .map(|e| e.message)
            .unwrap_or_else(|| "Operation failed".to_string());
        tracing::error!("{} failed: {}", payload.command, error_msg);
        ctx.mcp_config.write().set_error(error_msg);
    }
}

/// Handle mcp.test response
fn handle_mcp_test_response(mut ctx: AppContext, data: serde_json::Value) {
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let success = data.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let message = data.get("message").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let tools_count = data.get("tools_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let result_message = if success {
        format!("Connection successful! Found {} tools.", tools_count)
    } else {
        message
    };

    ctx.mcp_config.write().set_test_result(name, success, result_message);
    tracing::info!("MCP test result: success={}, tools_count={}", success, tools_count);
}

/// Handle mcp.connect/mcp.disconnect response
fn handle_mcp_connection_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::McpConnectionStatus;

    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let connected = data.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
    let error = data.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

    let status = if let Some(err) = error {
        McpConnectionStatus::Error(err)
    } else if connected {
        McpConnectionStatus::Connected
    } else {
        McpConnectionStatus::Disconnected
    };

    ctx.mcp_config.write().update_status(&name, status);
    tracing::info!("MCP server '{}' connection status updated: connected={}", name, connected);
}
