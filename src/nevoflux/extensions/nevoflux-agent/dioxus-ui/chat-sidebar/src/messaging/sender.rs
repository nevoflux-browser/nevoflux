/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Functions for sending messages to background script
//!
//! Uses the `bg:` API namespace for communication with background.js:
//! - `bg:connect` - Establish connection to native agent
//! - `bg:send_to_agent` - Send ChatMessage to native agent
//! - `bg:exec_tool` - Execute browser tool via background.js
//! - `bg:get_tab_context` - Get current tab context

use crate::messaging::bridge::*;
use shared_protocol::{*, chat::TabReference};
use wasm_bindgen_futures::JsFuture;

// ============================================
// Background API Request Types
// ============================================

/// Background API message wrapper using `bg:` namespace
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackgroundRequest {
    /// Establish connection to native agent
    #[serde(rename = "bg:connect")]
    Connect,
    /// Send ChatMessage to native agent
    #[serde(rename = "bg:send_to_agent")]
    SendToAgent { payload: serde_json::Value },
    /// Execute browser tool via background.js
    #[serde(rename = "bg:exec_tool")]
    ExecTool { payload: BrowserToolRequestPayload },
    /// Get current tab context
    #[serde(rename = "bg:get_tab_context")]
    GetTabContext,
    /// Open artifact in canvas tab
    #[serde(rename = "bg:open_artifact")]
    OpenArtifact { id: String },
}

/// Response from bg:exec_tool
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecToolResponse {
    pub success: bool,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<BrowserToolError>,
}

// ============================================
// Agent Protocol Messages (Chat Channel)
// ============================================

/// Send ChatMessage to agent via bg:send_to_agent
pub async fn send_to_agent(message: ChatMessage) -> Result<(), String> {
    // Log the message being sent for debugging
    let message_type = match &message {
        ChatMessage::ChatMessage(_) => "chat_message",
        ChatMessage::StopGeneration(_) => "stop_generation",
        ChatMessage::Cancel(_) => "cancel",
        ChatMessage::PermissionResponse(_) => "permission_response",
        ChatMessage::SkillCommand(_) => "skill_command",
        ChatMessage::PluginCommand(_) => "plugin_command",
        ChatMessage::SystemCommand(_) => "system_command",
        ChatMessage::BrowserToolResponse(_) => "browser_tool_response",
        ChatMessage::PickFilesRequest(_) => "pick_files_request",
        ChatMessage::PlanResponse(_) => "plan_response",
        _ => "other",
    };

    let payload = serde_json::to_value(&message)
        .map_err(|e| format!("Serialize ChatMessage error: {}", e))?;

    // Log the full message payload
    tracing::info!("[Sidebar] Sending to agent: {} - {}", message_type, payload);

    let request = BackgroundRequest::SendToAgent { payload };
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize request error: {:?}", e))?;

    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    tracing::debug!("[Sidebar] Message sent successfully: {}", message_type);
    Ok(())
}

/// Send chat message to agent
pub async fn send_chat_message(
    session_id: &str,
    content: String,
    mode: ChatMode,
    attachments: Vec<Attachment>,
    local_files: Vec<FileInfo>,
    tab_id: Option<u32>,
    tab_ids: Vec<TabReference>,
) -> Result<(), String> {
    let message = ChatMessage::ChatMessage(ChatMessagePayload {
        session_id: session_id.to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        content,
        mode,
        attachments,
        local_files,
        tab_id: tab_id.map(|id| id as i64),
        tab_ids,
    });

    send_to_agent(message).await
}

/// Send stop generation command
pub async fn send_stop_generation(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::StopGeneration(StopGenerationPayload {
        session_id: session_id.to_string(),
    });

    send_to_agent(message).await
}

/// Send cancel command (user interruption)
pub async fn send_cancel(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::Cancel(CancelPayload {
        session_id: session_id.to_string(),
    });

    send_to_agent(message).await
}

/// Send permission response
pub async fn send_permission_response(
    request_id: String,
    granted: bool,
    scope: Option<PermissionScope>,
) -> Result<(), String> {
    let message = ChatMessage::PermissionResponse(PermissionResponsePayload {
        request_id,
        granted,
        scope,
    });

    send_to_agent(message).await
}

/// Send skill command
pub async fn send_skill_command(
    session_id: &str,
    skill_name: String,
    args: Option<serde_json::Value>,
) -> Result<(), String> {
    let message = ChatMessage::SkillCommand(SkillCommandPayload {
        session_id: session_id.to_string(),
        skill_name,
        mode: ChatMode::default(),
        args,
    });

    send_to_agent(message).await
}

/// Send browser tool response back to agent via bg:send_to_agent
pub async fn send_browser_tool_response(response: BrowserToolResponsePayload) -> Result<(), String> {
    let message = ChatMessage::BrowserToolResponse(response);
    send_to_agent(message).await
}

// ============================================
// Plan Response Messages
// ============================================

/// Send plan confirmed response to agent
pub async fn send_plan_confirmed(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::PlanResponse(PlanResponsePayload {
        session_id: session_id.to_string(),
        response: PlanResponse::Confirmed,
    });
    send_to_agent(message).await
}

/// Send plan cancelled response to agent
pub async fn send_plan_cancelled(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::PlanResponse(PlanResponsePayload {
        session_id: session_id.to_string(),
        response: PlanResponse::Cancelled,
    });
    send_to_agent(message).await
}

// ============================================
// /loop Skill Messages (spec §2.6)
// ============================================

/// Send a LoopCancelCommand to the daemon.
pub async fn send_loop_cancel(session_id: &str, loop_id: &str, force: bool) -> Result<(), String> {
    let message = ChatMessage::LoopCancelCommand(LoopCancelCommandPayload {
        session_id: session_id.to_string(),
        loop_id: loop_id.to_string(),
        force,
    });
    send_to_agent(message).await
}

/// Result of parsing a `/loop <trigger> <prompt|/skill ...>` user input.
pub struct ParsedLoop {
    pub trigger_expr: String,
    pub prompt_text: Option<String>,
    pub wrapped_skill: Option<serde_json::Value>,
}

/// Send a `/loop` skill command (parsed from user input) to the daemon.
/// `mode` is the session's current chat mode and drives the loop's iteration
/// tool catalog on the daemon side.
pub async fn send_loop_create(
    session_id: &str,
    mode: ChatMode,
    parsed: ParsedLoop,
) -> Result<(), String> {
    let mut args = serde_json::json!({
        "trigger_expr": parsed.trigger_expr,
    });
    if let Some(p) = parsed.prompt_text {
        args["prompt_text"] = serde_json::Value::String(p);
    }
    if let Some(s) = parsed.wrapped_skill {
        args["wrapped_skill"] = s;
    }
    let payload = SkillCommandPayload {
        session_id: session_id.to_string(),
        skill_name: "loop".into(),
        mode,
        args: Some(args),
    };
    send_to_agent(ChatMessage::SkillCommand(payload)).await
}

/// Parse "/loop <trigger> <prompt>" into the create-loop arg shape.
/// Returns `None` if the input doesn't look like a /loop command.
///
/// Trigger may itself be a multi-token expression (e.g. `AND(time:5m,event:foo)`);
/// we split on the first whitespace at paren-depth 0. Trigger that starts with
/// a digit gets the canonical `time:` prefix prepended.
pub fn parse_loop_command(input: &str) -> Option<ParsedLoop> {
    let rest = input.strip_prefix("/loop ")?.trim();
    if rest.is_empty() {
        return None;
    }
    let (trigger_raw, prompt) = split_trigger_and_rest(rest)?;

    let trigger_expr = if trigger_raw
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("time:{trigger_raw}")
    } else {
        trigger_raw.to_string()
    };

    if let Some(skill_rest) = prompt.strip_prefix('/') {
        let (skill_name, skill_args) = skill_rest.split_once(' ').unwrap_or((skill_rest, ""));
        return Some(ParsedLoop {
            trigger_expr,
            prompt_text: None,
            wrapped_skill: Some(serde_json::json!({ "name": skill_name, "args": skill_args })),
        });
    }
    Some(ParsedLoop {
        trigger_expr,
        prompt_text: Some(prompt.to_string()),
        wrapped_skill: None,
    })
}

fn split_trigger_and_rest(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ' ' | '\t' if depth == 0 => return Some((&s[..i], s[i + 1..].trim_start())),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod loop_parser_tests {
    use super::*;

    #[test]
    fn simple_time_with_prompt() {
        let p = parse_loop_command("/loop 5m check the PR").unwrap();
        assert_eq!(p.trigger_expr, "time:5m");
        assert_eq!(p.prompt_text.as_deref(), Some("check the PR"));
        assert!(p.wrapped_skill.is_none());
    }

    #[test]
    fn nested_combinator_keeps_prompt() {
        let p = parse_loop_command("/loop AND(time:5m,event:foo) check the PR").unwrap();
        assert_eq!(p.trigger_expr, "AND(time:5m,event:foo)");
        assert_eq!(p.prompt_text.as_deref(), Some("check the PR"));
    }

    #[test]
    fn wrapped_skill_form() {
        let p = parse_loop_command("/loop 5m /video render demo.md").unwrap();
        assert!(p.prompt_text.is_none());
        let ws = p.wrapped_skill.unwrap();
        assert_eq!(ws.get("name").unwrap().as_str().unwrap(), "video");
        assert_eq!(ws.get("args").unwrap().as_str().unwrap(), "render demo.md");
    }

    #[test]
    fn rejects_non_loop_input() {
        assert!(parse_loop_command("hello world").is_none());
    }

    #[test]
    fn rejects_loop_without_args() {
        assert!(parse_loop_command("/loop ").is_none());
    }
}

// ============================================
// File Picker (Native Dialog via Agent)
// ============================================

/// Send pick files request to agent to open native file dialog
/// Uses system_command with "file.pick" command
/// Returns request_id for tracking the response
pub async fn send_pick_files_request(
    mode: &str,  // "files", "directories", or "both"
    multiple: bool,
    title: Option<String>,
) -> Result<String, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: request_id.clone(),
        command: "file.pick".to_string(),
        params: Some(serde_json::json!({
            "mode": mode,
            "multiple": multiple,
            "title": title,
        })),
    });

    send_to_agent(message).await?;
    Ok(request_id)
}

// ============================================
// Session Management Commands
// ============================================

/// Send session.resolve command to agent
/// Returns the session info and messages for the given session_id (zen_sync_id)
pub async fn send_session_resolve(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.resolve".to_string(),
        params: Some(serde_json::json!({
            "session_id": session_id
        })),
    });
    send_to_agent(message).await
}

/// Send session.list command to agent
/// Returns list of historical sessions
pub async fn send_session_list(limit: u32, offset: u32) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.list".to_string(),
        params: Some(serde_json::json!({
            "limit": limit,
            "offset": offset
        })),
    });
    send_to_agent(message).await
}

/// Send session.clone command to agent
/// Copies messages from source session to target session
pub async fn send_session_clone(source_id: &str, target_id: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.clone".to_string(),
        params: Some(serde_json::json!({
            "source_id": source_id,
            "target_id": target_id
        })),
    });
    send_to_agent(message).await
}

/// Send session.delete command to agent
pub async fn send_session_delete(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.delete".to_string(),
        params: Some(serde_json::json!({
            "session_id": session_id
        })),
    });
    send_to_agent(message).await
}

/// Send session.rename command to agent
pub async fn send_session_rename(session_id: &str, title: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.rename".to_string(),
        params: Some(serde_json::json!({
            "session_id": session_id,
            "title": title
        })),
    });
    send_to_agent(message).await
}

/// Send session.pin command to agent
pub async fn send_session_pin(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.pin".to_string(),
        params: Some(serde_json::json!({
            "session_id": session_id
        })),
    });
    send_to_agent(message).await
}

/// Send session.unpin command to agent
pub async fn send_session_unpin(session_id: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "session.unpin".to_string(),
        params: Some(serde_json::json!({
            "session_id": session_id
        })),
    });
    send_to_agent(message).await
}

// ============================================
// MCP Configuration Commands
// ============================================

/// Send mcp.list command to agent
/// Returns list of configured MCP servers and their connection status
pub async fn send_mcp_list() -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.list".to_string(),
        params: None,
    });
    send_to_agent(message).await
}

/// Send skill.list command to agent
/// Returns list of available skills
pub async fn send_skill_list() -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "skill.list".to_string(),
        params: None,
    });
    send_to_agent(message).await
}

/// Send mcp.add command to agent
/// Adds a new MCP server configuration
pub async fn send_mcp_add(
    name: &str,
    command: &str,
    args: Vec<String>,
    enabled: bool,
    env: Vec<(String, String)>,
) -> Result<(), String> {
    let env_map: std::collections::HashMap<String, String> = env.into_iter().collect();
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.add".to_string(),
        params: Some(serde_json::json!({
            "server": {
                "name": name,
                "command": command,
                "args": args,
                "enabled": enabled,
                "env": env_map
            }
        })),
    });
    send_to_agent(message).await
}

/// Send mcp.update command to agent
/// Updates an existing MCP server configuration
pub async fn send_mcp_update(
    name: &str,
    command: &str,
    args: Vec<String>,
    enabled: bool,
    env: Vec<(String, String)>,
) -> Result<(), String> {
    let env_map: std::collections::HashMap<String, String> = env.into_iter().collect();
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.update".to_string(),
        params: Some(serde_json::json!({
            "name": name,
            "server": {
                "name": name,
                "command": command,
                "args": args,
                "enabled": enabled,
                "env": env_map
            }
        })),
    });
    send_to_agent(message).await
}

/// Send mcp.delete command to agent
/// Deletes an MCP server configuration
pub async fn send_mcp_delete(name: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.delete".to_string(),
        params: Some(serde_json::json!({
            "name": name
        })),
    });
    send_to_agent(message).await
}

/// Send mcp.test command to agent
/// Tests connection to an MCP server
pub async fn send_mcp_test(name: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.test".to_string(),
        params: Some(serde_json::json!({
            "name": name
        })),
    });
    send_to_agent(message).await
}

/// Send mcp.connect command to agent
/// Connects to an MCP server
pub async fn send_mcp_connect(name: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.connect".to_string(),
        params: Some(serde_json::json!({
            "name": name
        })),
    });
    send_to_agent(message).await
}

/// Send mcp.disconnect command to agent
/// Disconnects from an MCP server
pub async fn send_mcp_disconnect(name: &str) -> Result<(), String> {
    let message = ChatMessage::SystemCommand(SystemCommandPayload {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: "mcp.disconnect".to_string(),
        params: Some(serde_json::json!({
            "name": name
        })),
    });
    send_to_agent(message).await
}

// ============================================
// Browser Tool Execution (bg:exec_tool)
// ============================================

/// Execute browser tool via bg:exec_tool and return result
///
/// This sends the request to background.js which has access to
/// browser.nevoflux.* API and can execute browser tools.
pub async fn exec_browser_tool(request: BrowserToolRequestPayload) -> Result<BrowserToolResponsePayload, String> {
    let bg_request = BackgroundRequest::ExecTool { payload: request.clone() };
    let js_value = to_js_value(&bg_request)
        .map_err(|e| format!("Serialize request error: {:?}", e))?;

    // Use safe error handling to avoid Xray wrapper issues in Firefox extensions
    let response_js = match JsFuture::from(runtime_send_message(js_value)).await {
        Ok(v) => v,
        Err(e) => {
            // Avoid accessing properties on JsValue error objects directly
            // as they may be Xray-wrapped and trigger security errors
            let error_msg = if e.is_string() {
                e.as_string().unwrap_or_else(|| "Unknown error".to_string())
            } else if e.is_object() {
                // Try to safely convert to string without accessing .message directly
                e.as_string()
                    .or_else(|| {
                        js_sys::JSON::stringify(&e)
                            .ok()
                            .and_then(|s| s.as_string())
                    })
                    .unwrap_or_else(|| "bg:exec_tool failed (object error)".to_string())
            } else {
                "bg:exec_tool failed".to_string()
            };
            return Err(error_msg);
        }
    };

    // Parse response - handle undefined/null responses
    if response_js.is_undefined() || response_js.is_null() {
        return Err("bg:exec_tool returned undefined/null".to_string());
    }

    // Parse response
    let response: ExecToolResponse = from_js_value(response_js)
        .map_err(|e| format!("Parse exec_tool response error: {}", e))?;

    Ok(BrowserToolResponsePayload {
        request_id: request.request_id,
        session_id: request.session_id,
        success: response.success,
        result: response.result,
        error: response.error,
    })
}

// ============================================
// Extension Internal Messages
// ============================================

/// Extension-internal message types (not agent protocol)
/// These are messages between sidebar and background.js only
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum InternalMessage {
    /// Ping to check connection
    Ping { timestamp: u64 },
    /// Pong response
    Pong { timestamp: u64 },
    /// Tab context update from background
    TabContextUpdate(TabContextPayload),
    /// Connection status update
    ConnectionStatus { connected: bool },
    /// AskUser request from agent (via background.js)
    AskUserRequest(AskUserRequestPayload),
    /// Artifact streaming started
    ArtifactStart(ArtifactStartPayload),
    /// Artifact content delta
    ArtifactDelta(ArtifactDeltaPayload),
    /// Artifact streaming complete
    ArtifactComplete(ArtifactCompletePayload),
    /// Inject a chat message from canvas/external source
    CanvasChatInject(CanvasChatInjectPayload),
}

/// Canvas chat inject payload — external source wants to send a chat message
/// that appears in sidebar as a user message and triggers agent response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasChatInjectPayload {
    pub session_id: String,
    pub message_id: String,
    pub content: String,
    /// Image attachments from SDK (base64 encoded)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<shared_protocol::Attachment>,
    /// Local files/directories from SDK
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_files: Vec<shared_protocol::FileInfo>,
}

/// Artifact start payload (from background.js broadcast)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactStartPayload {
    pub id: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Whether this artifact is already saved to My Canvas
    #[serde(default)]
    pub is_persistent: bool,
}

/// Artifact delta payload
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactDeltaPayload {
    pub id: String,
    #[serde(default)]
    pub delta: Option<String>,
}

/// Artifact complete payload
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactCompletePayload {
    pub id: String,
    #[serde(default)]
    pub final_code: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Whether this artifact is already saved to My Canvas
    #[serde(default)]
    pub is_persistent: bool,
}

/// AskUser request payload from background.js
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AskUserRequestPayload {
    pub request_id: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub allow_custom: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    60000
}

/// Tab context from background script
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TabContextPayload {
    pub tab_id: u32,
    /// Zen Sync ID (persistent across restarts, used as session_id)
    #[serde(default)]
    pub zen_sync_id: Option<String>,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon_url: Option<String>,
}

/// Send internal message to background
pub async fn send_internal(message: InternalMessage) -> Result<(), String> {
    let js_value = to_js_value(&message)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    Ok(())
}

/// Request tab context via bg:get_tab_context and return the response
/// If tab_id is None, gets the active tab. If specified, gets that specific tab.
pub async fn request_tab_context_for_tab(tab_id: Option<i32>) -> Result<Option<TabContextPayload>, String> {
    // Build request with optional tab_id
    let request = if let Some(id) = tab_id {
        serde_json::json!({
            "type": "bg:get_tab_context",
            "tab_id": id
        })
    } else {
        serde_json::json!({
            "type": "bg:get_tab_context"
        })
    };

    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    let response = JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    // Parse the response
    if response.is_null() || response.is_undefined() {
        return Ok(None);
    }

    let tab_context: TabContextPayload = from_js_value(response)
        .map_err(|e| format!("Parse tab context error: {:?}", e))?;

    Ok(Some(tab_context))
}

/// Request current active tab context via bg:get_tab_context
pub async fn request_tab_context() -> Result<Option<TabContextPayload>, String> {
    request_tab_context_for_tab(None).await
}

/// Query fresh tab context and build (tab_id, tab_ids) for chat messages.
/// Ensures the current tab's URL/title are always included.
pub async fn build_current_tab_ids() -> (Option<u32>, Vec<shared_protocol::TabReference>) {
    match request_tab_context().await {
        Ok(Some(tc)) => {
            let tab_ids = vec![shared_protocol::TabReference {
                space: String::new(),
                tab_id: tc.tab_id as i64,
                tab_title: tc.title,
                url: tc.url,
            }];
            (Some(tc.tab_id), tab_ids)
        }
        _ => (None, vec![])
    }
}

/// Request connection to native agent via bg:connect
pub async fn request_connect() -> Result<(), String> {
    let request = BackgroundRequest::Connect;
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    Ok(())
}

/// Send ping to check connection
pub async fn send_ping() -> Result<(), String> {
    send_internal(InternalMessage::Ping {
        timestamp: js_sys::Date::now() as u64,
    })
    .await
}

/// Request background.js to open artifact in a new canvas tab
pub async fn send_open_artifact(id: &str) -> Result<(), String> {
    let request = BackgroundRequest::OpenArtifact { id: id.to_string() };
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

// ============================================
// Canvas Persist Save (pin to My Canvas)
// ============================================

/// Response from bg:canvas_persist_save
#[derive(serde::Deserialize)]
struct CanvasPersistSaveResponse {
    success: bool,
    #[serde(default)]
    persisted_at: Option<i64>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

/// Send canvas.persist.save via background.js and handle the response.
///
/// On success: flips is_persistent = true on the matching ArtifactData in the
/// messages signal (via mark_artifact_persistent).
/// On not_found error: logs a console warning telling the user the canvas is gone.
/// On other error: logs a console error.
pub async fn send_save_to_my_canvas(canvas_id: String, messages: dioxus::prelude::Signal<Vec<crate::state::Message>>) {
    use dioxus::prelude::WritableExt;
    let mut messages = messages;
    let request = serde_json::json!({
        "type": "bg:canvas_persist_save",
        "payload": { "canvas_id": canvas_id }
    });

    let js_value = match to_js_value(&request) {
        Ok(v) => v,
        Err(e) => {
            web_sys::console::error_1(
                &format!("[Sidebar] Failed to serialize canvas.persist.save: {:?}", e).into(),
            );
            return;
        }
    };

    let response_js = match JsFuture::from(runtime_send_message(js_value)).await {
        Ok(v) => v,
        Err(e) => {
            web_sys::console::error_1(
                &format!("[Sidebar] canvas.persist.save send failed: {:?}", e).into(),
            );
            return;
        }
    };

    // Parse response
    let response: CanvasPersistSaveResponse = match from_js_value(response_js) {
        Ok(r) => r,
        Err(e) => {
            web_sys::console::error_1(
                &format!("[Sidebar] canvas.persist.save bad response: {}", e).into(),
            );
            return;
        }
    };

    if response.success {
        tracing::info!("[Sidebar] Canvas {} saved to My Canvas (persisted_at={:?})", canvas_id, response.persisted_at);
        // Flip local state so the pin button becomes filled immediately.
        messages.with_mut(|msgs| {
            crate::state::Message::mark_artifact_persistent(msgs, &canvas_id);
        });
    } else {
        // Inspect error code to distinguish not_found from other failures.
        let code = response.error.as_ref()
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str().map(String::from)
                .or_else(|| c.as_i64().map(|n| n.to_string())));
        let msg = response.error.as_ref()
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");

        if code.as_deref() == Some("not_found") {
            web_sys::console::warn_1(
                &format!("[Sidebar] Canvas {} no longer exists — cannot pin to My Canvas", canvas_id).into(),
            );
        } else {
            web_sys::console::error_1(
                &format!("[Sidebar] canvas.persist.save failed (code={:?}): {}", code, msg).into(),
            );
        }
    }
}

// ============================================
// Sidebar Layout Messages
// ============================================

/// Set the sidebar width via background → browser.nevoflux.setSidebarWidth
pub async fn send_set_sidebar_width(width: u32) -> Result<(), String> {
    crate::bindings::nevoflux_api::set_sidebar_width(width).await
}

// ============================================
// AskUser Response Messages
// ============================================

/// AskUser response message for sending back to background.js
#[derive(Debug, Clone, serde::Serialize)]
struct AskUserResponseMessage {
    r#type: &'static str,
    payload: AskUserResponsePayload,
}

#[derive(Debug, Clone, serde::Serialize)]
struct AskUserResponsePayload {
    request_id: String,
    answer: String,
    is_custom: bool,
    selected_index: i32,
    cancelled: bool,
}

/// Send AskUser response back to background.js
pub async fn send_ask_user_response(
    request_id: &str,
    answer: &str,
    is_custom: bool,
    selected_index: Option<i32>,
) -> Result<(), String> {
    let message = AskUserResponseMessage {
        r#type: "ask_user_response",
        payload: AskUserResponsePayload {
            request_id: request_id.to_string(),
            answer: answer.to_string(),
            is_custom,
            selected_index: selected_index.unwrap_or(-1),
            cancelled: false,
        },
    };

    let js_value = to_js_value(&message)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    Ok(())
}

/// Send AskUser cancel back to background.js
pub async fn send_ask_user_cancel(request_id: &str) -> Result<(), String> {
    let message = AskUserResponseMessage {
        r#type: "ask_user_response",
        payload: AskUserResponsePayload {
            request_id: request_id.to_string(),
            answer: String::new(),
            is_custom: false,
            selected_index: -1,
            cancelled: true,
        },
    };

    let js_value = to_js_value(&message)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    Ok(())
}

// ============================================
// Tool Authorization Response
// ============================================

/// Send tool authorization response to agent
pub async fn send_tool_auth_response(
    tool_id: String,
    option_index: u32,
    scope: shared_protocol::AuthScope,
) -> Result<(), String> {
    let msg = ChatMessage::ToolAuthResponse(shared_protocol::ToolAuthResponsePayload {
        tool_id,
        option_index,
        scope,
    });

    send_to_agent(msg).await
}

// ============================================
// Settings / Avatar
// ============================================

/// Fetch the user avatar data URL from settings (via background.js → contentStore)
pub async fn fetch_avatar() -> Result<Option<String>, String> {
    let request = serde_json::json!({
        "type": "bg:get_settings",
        "key": "settings"
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    let response = JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Fetch settings failed: {:?}", e))?;

    let response_obj: serde_json::Value = from_js_value(response)
        .map_err(|e| format!("Parse settings response error: {}", e))?;

    if response_obj.get("success").and_then(|s| s.as_bool()) != Some(true) {
        return Ok(None);
    }

    let avatar = response_obj
        .get("data")
        .and_then(|d| d.get("identity"))
        .and_then(|i| i.get("avatar"))
        .and_then(|a| a.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(avatar)
}

// ============================================
// System Command Queries (Sidebar → Background → Agent with async response)
// ============================================

/// Query agent status (first_run, has_configured_provider).
///
/// Sends a `bg:system_command` with command "status" to background.js,
/// which forwards it to the native agent and returns the response.
pub async fn query_agent_status() -> Result<serde_json::Value, String> {
    let request = serde_json::json!({
        "type": "bg:system_command",
        "command": "status",
        "params": {}
    });

    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    let response = JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;

    if response.is_undefined() || response.is_null() {
        return Err("bg:system_command returned undefined/null".to_string());
    }

    let response_obj: serde_json::Value = from_js_value(response)
        .map_err(|e| format!("Parse status response error: {}", e))?;

    if response_obj.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(response_obj.get("data").cloned().unwrap_or_default())
    } else {
        Err(response_obj
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error")
            .to_string())
    }
}

// ============================================
// Browser Tool Messages (Legacy - Deprecated)
// ============================================

// Note: Browser tool execution now uses bg:exec_tool via exec_browser_tool()
// These legacy functions are kept for backward compatibility but should not be used

/// Browser tool request message for forwarding to background.js
/// @deprecated Use exec_browser_tool() instead
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BrowserToolMessage {
    /// Forward browser tool request to content script
    BrowserToolRequest(shared_protocol::BrowserToolRequestPayload),
}

/// Forward browser tool request to background.js for execution (async version)
/// @deprecated Use exec_browser_tool() instead
pub async fn forward_browser_tool_request(
    payload: shared_protocol::BrowserToolRequestPayload,
) -> Result<(), String> {
    // Use the new bg:exec_tool API
    let _ = exec_browser_tool(payload).await?;
    Ok(())
}

/// Forward browser tool request to background.js for execution (sync, fire-and-forget)
/// @deprecated Use exec_browser_tool() with async spawn instead
pub fn forward_browser_tool_request_sync(
    payload: &shared_protocol::BrowserToolRequestPayload,
) -> Result<(), String> {
    // Keep legacy behavior for backward compatibility
    let message = BrowserToolMessage::BrowserToolRequest(payload.clone());
    crate::messaging::bridge::send_message_sync(&message)
}

// ============================================
// EventBus Messages
// ============================================

pub async fn send_events_subscribe(
    patterns: Vec<String>,
    replay_sticky: bool,
    buffer_size: usize,
) -> Result<(), String> {
    let request = serde_json::json!({
        "type": "bg:events_subscribe",
        "patterns": patterns,
        "replay_sticky": replay_sticky,
        "buffer_size": buffer_size,
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

pub async fn send_events_unsubscribe(subscription_id: &str) -> Result<(), String> {
    let request = serde_json::json!({
        "type": "bg:events_unsubscribe",
        "subscription_id": subscription_id,
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

pub async fn send_events_publish(
    topic: &str,
    data: serde_json::Value,
    delivery: &str,
) -> Result<(), String> {
    let request = serde_json::json!({
        "type": "bg:events_publish",
        "topic": topic,
        "data": data,
        "delivery": delivery,
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

/// Request cancellation of a running canvas_video render job. Wraps a
/// `canvas_video_render_cancel` envelope and sends via the existing
/// bg:send_to_agent path — no new bridge type required.
pub async fn send_canvas_video_cancel(job_id: &str) -> Result<(), String> {
    let request = serde_json::json!({
        "type": "bg:send_to_agent",
        "payload": {
            "type": "canvas_video_render_cancel",
            "payload": { "job_id": job_id }
        }
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

/// Ask the daemon to play or reveal a rendered MP4 via the OS default app.
/// `action` is "play" (opens in default video player) or "reveal"
/// (opens containing folder, selecting the file on macOS/Windows).
pub async fn send_canvas_video_reveal_path(path: &str, action: &str) -> Result<(), String> {
    let request = serde_json::json!({
        "type": "bg:send_to_agent",
        "payload": {
            "type": "canvas_video_reveal_path",
            "payload": { "path": path, "action": action }
        }
    });
    let js_value = to_js_value(&request)
        .map_err(|e| format!("Serialize error: {:?}", e))?;
    JsFuture::from(runtime_send_message(js_value))
        .await
        .map_err(|e| format!("Send failed: {:?}", e))?;
    Ok(())
}

/// Async sleep using JavaScript setTimeout
pub async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("no window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
    });
    let _ = JsFuture::from(promise).await;
}
