//! Wire client for the kimi-agent CLI subprocess.
//!
//! Manages the lifecycle of a kimi-agent subprocess, communicating over
//! stdin/stdout using a JSON-RPC 2.0 protocol. The wire client handles
//! spawning, initialization, prompt sending, tool result replies, and
//! event parsing.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::client::KimiAgentClient;
use super::types::{
    ExtractedToolCall, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, KimiUsage,
    WireExternalTool, WireMessageEnvelope,
};

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// State machine tracking the lifecycle of the wire connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireState {
    /// No subprocess has been spawned yet.
    NoProcess,
    /// The subprocess is starting up.
    Initializing,
    /// The subprocess is initialized and ready to accept prompts.
    Ready,
    /// A prompt has been sent; waiting for events.
    InTurn,
    /// A ToolCallRequest was received; waiting for tool results.
    WaitingForToolResults,
    /// The session has ended (TurnEnd received or process exited).
    Finished,
}

// ---------------------------------------------------------------------------
// Parsed events
// ---------------------------------------------------------------------------

/// A parsed event from the kimi-agent wire protocol.
#[derive(Debug, Clone)]
pub enum WireEvent {
    /// A new turn has begun.
    TurnBegin,
    /// The current turn has ended.
    TurnEnd,
    /// A new step has begun within a turn.
    StepBegin {
        /// Step number.
        n: i64,
    },
    /// The current step was interrupted.
    StepInterrupted,
    /// A content part (text output).
    ContentPart {
        /// The text content.
        text: String,
    },
    /// A thinking part (reasoning output).
    ThinkingPart {
        /// The thinking text.
        text: String,
    },
    /// A built-in tool call (notification, handled by agent internally).
    ToolCall {
        /// Tool call identifier.
        id: String,
        /// Function name.
        name: String,
        /// JSON-encoded arguments string.
        arguments: String,
    },
    /// Streaming tool call arguments part.
    ToolCallPart {
        /// Partial arguments string.
        arguments_part: String,
    },
    /// An external tool call request (requires response).
    ToolCallRequest {
        /// Unique identifier for this tool call.
        id: String,
        /// Tool name to invoke.
        name: String,
        /// Arguments as a JSON value.
        arguments: Value,
    },
    /// A tool result (notification, for display purposes).
    ToolResult {
        /// Tool call identifier.
        tool_call_id: String,
        /// Whether the result is an error.
        is_error: bool,
        /// Output text.
        output: String,
    },
    /// A status update with token usage.
    StatusUpdate {
        /// Number of input tokens.
        input_tokens: u64,
        /// Number of output tokens.
        output_tokens: u64,
    },
    /// Context compaction has begun.
    CompactionBegin,
    /// Context compaction has ended.
    CompactionEnd,
    /// A subagent event.
    SubagentEvent,
    /// An unknown event type.
    Unknown(String),
}

// ---------------------------------------------------------------------------
// WireClient
// ---------------------------------------------------------------------------

/// Client managing a kimi-agent subprocess over JSON-RPC 2.0.
pub struct WireClient {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    reader: Option<BufReader<std::process::ChildStdout>>,
    state: WireState,
    request_id: AtomicU64,
    /// JSON-RPC request ID from the last ToolCallRequest, needed for
    /// sending the ToolResult response.
    last_request_id: Option<Value>,
}

impl WireClient {
    /// Spawn a new kimi-agent subprocess with the given configuration and model.
    ///
    /// The subprocess is started with `--yolo` mode and configured based on
    /// the client settings (model, working directory, thinking mode).
    pub fn spawn(config: &KimiAgentClient, model: &str) -> Result<Self, String> {
        let mut cmd = Command::new(config.command());

        cmd.arg("--yolo");

        if !model.is_empty() {
            cmd.arg("--model").arg(model);
        } else if let Some(default_model) = config.model() {
            cmd.arg("--model").arg(default_model);
        }

        if let Some(dir) = config.working_dir() {
            cmd.arg("--work-dir").arg(dir);
        }

        match config.thinking() {
            Some(true) => {
                cmd.arg("--thinking");
            }
            Some(false) => {
                cmd.arg("--no-thinking");
            }
            None => {}
        }

        // Pass API key via environment variable if set
        if let Some(api_key) = config.api_key() {
            cmd.env("KIMI_API_KEY", api_key);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // On Windows, hide the console window for the subprocess
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn kimi-agent: {}", e))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();

        let reader = stdout.map(BufReader::new);

        Ok(Self {
            child: Some(child),
            stdin,
            reader,
            state: WireState::NoProcess,
            request_id: AtomicU64::new(1),
            last_request_id: None,
        })
    }

    /// Send the `initialize` JSON-RPC request with external tool definitions.
    ///
    /// Waits up to 30 seconds for the initialization response.
    pub fn initialize(
        &mut self,
        tools: &[rig::completion::ToolDefinition],
    ) -> Result<Value, String> {
        self.state = WireState::Initializing;

        let params = build_initialize_params(tools);
        let id = self.next_request_id();
        let request = JsonRpcRequest::new(id, "initialize", Some(params));
        self.send_message(&serde_json::to_value(&request).map_err(|e| e.to_string())?)?;

        // Wait for the response with a 30-second timeout
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err("Timeout waiting for initialize response".into());
            }

            match self.read_line() {
                Some(JsonRpcMessage::Response { result, .. }) => {
                    self.state = WireState::Ready;
                    return Ok(result);
                }
                Some(JsonRpcMessage::Error { error, .. }) => {
                    return Err(format!(
                        "Initialize error ({}): {}",
                        error.code, error.message
                    ));
                }
                Some(_) => {
                    // Skip notifications/requests during initialization
                    continue;
                }
                None => {
                    return Err("Connection closed during initialization".into());
                }
            }
        }
    }

    /// Send a prompt to the agent.
    pub fn send_prompt(&mut self, user_input: &str) -> Result<(), String> {
        let params = build_prompt_params(user_input);
        let id = self.next_request_id();
        let request = JsonRpcRequest::new(id, "prompt", Some(params));
        self.send_message(&serde_json::to_value(&request).map_err(|e| e.to_string())?)?;
        self.state = WireState::InTurn;
        Ok(())
    }

    /// Send a multimodal prompt (text + images) to the agent.
    ///
    /// Uses the `ContentPart[]` format for `user_input` as defined in wire protocol v1.4.
    pub fn send_prompt_multimodal(&mut self, user_input: Value) -> Result<(), String> {
        let params = serde_json::json!({ "user_input": user_input });
        let id = self.next_request_id();
        let request = JsonRpcRequest::new(id, "prompt", Some(params));
        self.send_message(&serde_json::to_value(&request).map_err(|e| e.to_string())?)?;
        self.state = WireState::InTurn;
        Ok(())
    }

    /// Send a tool result back to the agent in response to a ToolCallRequest.
    ///
    /// `request_id` is the JSON-RPC id from the incoming Request message.
    pub fn send_tool_result(
        &mut self,
        request_id: Value,
        tool_call_id: &str,
        is_error: bool,
        output: &str,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        let result = build_tool_result_envelope(tool_call_id, is_error, output, error_message);
        let response = JsonRpcResponse::new(request_id, result);
        self.send_message(&serde_json::to_value(&response).map_err(|e| e.to_string())?)?;
        self.state = WireState::InTurn;
        Ok(())
    }

    /// Read events until a TurnEnd or ToolCallRequest is received.
    ///
    /// Returns accumulated text, any tool calls, and token usage.
    /// When a ToolCallRequest is encountered, the state transitions to
    /// `WaitingForToolResults` and the caller should supply results via
    /// `send_tool_result()`.
    pub fn read_until_pause(
        &mut self,
    ) -> Result<(String, Vec<ExtractedToolCall>, KimiUsage), String> {
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = KimiUsage::default();

        loop {
            let msg = match self.read_line() {
                Some(m) => m,
                None => {
                    self.state = WireState::Finished;
                    break;
                }
            };

            match msg {
                // Server-side notification (events without id)
                JsonRpcMessage::Notification { params, .. } => {
                    if let Some(params) = params {
                        if let Some(event) = parse_wire_event(&params) {
                            match event {
                                WireEvent::ContentPart { text: t } => {
                                    text.push_str(&t);
                                }
                                WireEvent::ThinkingPart { .. } => {
                                    // Thinking parts are informational; not included in output
                                }
                                WireEvent::TurnEnd => {
                                    self.state = WireState::Finished;
                                    break;
                                }
                                WireEvent::StatusUpdate {
                                    input_tokens,
                                    output_tokens,
                                } => {
                                    usage.input_tokens = input_tokens;
                                    usage.output_tokens = output_tokens;
                                }
                                WireEvent::TurnBegin
                                | WireEvent::StepBegin { .. }
                                | WireEvent::StepInterrupted
                                | WireEvent::ToolCall { .. }
                                | WireEvent::ToolCallPart { .. }
                                | WireEvent::ToolResult { .. }
                                | WireEvent::CompactionBegin
                                | WireEvent::CompactionEnd
                                | WireEvent::SubagentEvent => {
                                    // Informational events; no action needed in batch mode
                                }
                                WireEvent::ToolCallRequest { .. } => {
                                    // ToolCallRequest should come as a Request, not Notification.
                                    // Handle it here defensively.
                                }
                                WireEvent::Unknown(_) => {}
                            }
                        }
                    }
                }
                // Server-side request (ToolCallRequest comes with an id for reply)
                JsonRpcMessage::Request { id, params, .. } => {
                    if let Some(params) = params {
                        if let Some(event) = parse_wire_event(&params) {
                            if let WireEvent::ToolCallRequest {
                                id: tc_id,
                                name,
                                arguments,
                            } = event
                            {
                                tool_calls.push(ExtractedToolCall {
                                    id: tc_id,
                                    name,
                                    arguments,
                                });
                                self.state = WireState::WaitingForToolResults;
                                self.last_request_id = Some(id);
                                break;
                            }
                        }
                    }
                }
                JsonRpcMessage::Error { error, .. } => {
                    // The agent sent a JSON-RPC error (e.g., 401 Unauthorized).
                    // Propagate it so callers don't hang.
                    self.state = WireState::Finished;
                    return Err(format!(
                        "kimi-agent error ({}): {}",
                        error.code, error.message
                    ));
                }
                JsonRpcMessage::Response { .. } => {
                    // Responses to our own requests; generally unexpected here
                }
            }
        }

        Ok((text, tool_calls, usage))
    }

    /// Read a single event from the wire protocol.
    ///
    /// Returns `None` if the stream is closed. Automatically updates internal
    /// state for terminal events (TurnEnd, ToolCallRequest).
    pub fn read_next_event(&mut self) -> Option<WireEvent> {
        loop {
            let msg = self.read_line()?;
            match msg {
                JsonRpcMessage::Notification { params, .. } => {
                    if let Some(params) = params {
                        if let Some(event) = parse_wire_event(&params) {
                            if matches!(event, WireEvent::TurnEnd) {
                                self.state = WireState::Finished;
                            }
                            return Some(event);
                        }
                    }
                }
                JsonRpcMessage::Request { id, params, .. } => {
                    if let Some(params) = params {
                        if let Some(event) = parse_wire_event(&params) {
                            if matches!(event, WireEvent::ToolCallRequest { .. }) {
                                self.state = WireState::WaitingForToolResults;
                                self.last_request_id = Some(id);
                            }
                            return Some(event);
                        }
                    }
                }
                JsonRpcMessage::Error { error, .. } => {
                    self.state = WireState::Finished;
                    return Some(WireEvent::Unknown(format!(
                        "error({}): {}",
                        error.code, error.message
                    )));
                }
                JsonRpcMessage::Response { .. } => {
                    // Skip responses to our own requests
                    continue;
                }
            }
        }
    }

    /// Kill the subprocess.
    pub fn kill(&mut self) {
        // Drop stdin to signal EOF
        self.stdin.take();
        self.reader.take();
        self.last_request_id = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.state = WireState::Finished;
    }

    /// Get the current wire state.
    pub fn state(&self) -> WireState {
        self.state
    }

    /// Get the JSON-RPC request ID from the last ToolCallRequest.
    ///
    /// This ID must be used when sending the ToolResult response back.
    pub fn last_request_id(&self) -> Option<&Value> {
        self.last_request_id.as_ref()
    }

    // -- Private helpers --

    /// Send a JSON-RPC message to the subprocess stdin.
    fn send_message(&mut self, value: &Value) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("No stdin available (process not running)")?;

        let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
        stdin
            .write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| format!("Failed to write newline: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush stdin: {}", e))?;
        Ok(())
    }

    /// Read a single JSON-RPC message from the subprocess stdout.
    ///
    /// Returns `None` if the stream is closed or a read error occurs.
    fn read_line(&mut self) -> Option<JsonRpcMessage> {
        let reader = self.reader.as_mut()?;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<JsonRpcMessage>(trimmed) {
                        Ok(msg) => return Some(msg),
                        Err(_) => continue, // Skip non-JSON lines
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Generate the next request id.
    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Drop for WireClient {
    fn drop(&mut self) {
        self.kill();
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build the `params` object for the `initialize` JSON-RPC request.
pub fn build_initialize_params(tools: &[rig::completion::ToolDefinition]) -> Value {
    let wire_tools: Vec<WireExternalTool> = tools.iter().map(WireExternalTool::from).collect();
    serde_json::json!({
        "protocol_version": "1.2",
        "client": {
            "name": "nevoflux",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "external_tools": wire_tools,
    })
}

/// Build the `params` object for the `prompt` JSON-RPC request.
pub fn build_prompt_params(user_input: &str) -> Value {
    serde_json::json!({
        "user_input": user_input
    })
}

/// Build the JSON-RPC response `result` envelope for a tool result.
pub fn build_tool_result_envelope(
    tool_call_id: &str,
    is_error: bool,
    output: &str,
    error_message: Option<&str>,
) -> Value {
    serde_json::json!({
        "type": "ToolResult",
        "payload": {
            "tool_call_id": tool_call_id,
            "return_value": {
                "is_error": is_error,
                "output": output,
                "message": error_message.unwrap_or(""),
            }
        }
    })
}

/// Parse a wire event from JSON-RPC `params`.
///
/// The params are expected to be a `WireMessageEnvelope` with a `type` and
/// `payload` field.
pub fn parse_wire_event(params: &Value) -> Option<WireEvent> {
    let envelope: WireMessageEnvelope = serde_json::from_value(params.clone()).ok()?;

    match envelope.msg_type.as_str() {
        "TurnBegin" => Some(WireEvent::TurnBegin),
        "TurnEnd" => Some(WireEvent::TurnEnd),
        "StepBegin" => {
            let n = envelope
                .payload
                .get("n")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Some(WireEvent::StepBegin { n })
        }
        "StepInterrupted" => Some(WireEvent::StepInterrupted),
        "ContentPart" => {
            let content_type = envelope
                .payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("text");
            match content_type {
                "think" | "thinking" => {
                    // Thinking parts store text in the "think" field
                    let text = envelope
                        .payload
                        .get("think")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(WireEvent::ThinkingPart { text })
                }
                _ => {
                    let text = envelope
                        .payload
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(WireEvent::ContentPart { text })
                }
            }
        }
        "ToolCall" => {
            // Built-in tool call (notification). Structure:
            // { type: "function", id, function: { name, arguments } }
            let id = envelope
                .payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let func = envelope.payload.get("function");
            let name = func
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = func
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(WireEvent::ToolCall {
                id,
                name,
                arguments,
            })
        }
        "ToolCallPart" => {
            let arguments_part = envelope
                .payload
                .get("arguments_part")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(WireEvent::ToolCallPart { arguments_part })
        }
        "ToolCallRequest" => {
            // External tool call request. Arguments is a JSON-encoded string.
            let id = envelope
                .payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = envelope
                .payload
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Arguments may be a JSON string or null; parse string into Value
            let arguments = match envelope.payload.get("arguments") {
                Some(Value::String(s)) => {
                    serde_json::from_str(s).unwrap_or(Value::Object(serde_json::Map::new()))
                }
                Some(other) => other.clone(),
                None => Value::Object(serde_json::Map::new()),
            };
            Some(WireEvent::ToolCallRequest {
                id,
                name,
                arguments,
            })
        }
        "ToolResult" => {
            let tool_call_id = envelope
                .payload
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let rv = envelope.payload.get("return_value");
            let is_error = rv
                .and_then(|r| r.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let output = rv
                .and_then(|r| r.get("output"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(WireEvent::ToolResult {
                tool_call_id,
                is_error,
                output,
            })
        }
        "StatusUpdate" => {
            let token_usage = envelope.payload.get("token_usage");
            let input_tokens = token_usage
                .and_then(|t| t.get("input_other"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = token_usage
                .and_then(|t| t.get("output"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(WireEvent::StatusUpdate {
                input_tokens,
                output_tokens,
            })
        }
        "CompactionBegin" => Some(WireEvent::CompactionBegin),
        "CompactionEnd" => Some(WireEvent::CompactionEnd),
        "SubagentEvent" => Some(WireEvent::SubagentEvent),
        other => Some(WireEvent::Unknown(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_initialize_request() {
        let tools = vec![rig::completion::ToolDefinition {
            name: "bash".to_string(),
            description: "Run a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }];

        let params = build_initialize_params(&tools);
        assert_eq!(params["protocol_version"], "1.2");
        assert_eq!(params["client"]["name"], "nevoflux");

        let wire_tools = params["external_tools"].as_array().unwrap();
        assert_eq!(wire_tools.len(), 1);
        assert_eq!(wire_tools[0]["name"], "bash");
        assert_eq!(wire_tools[0]["description"], "Run a shell command");
        assert_eq!(wire_tools[0]["parameters"]["required"][0], "command");
    }

    #[test]
    fn test_build_prompt_params_text() {
        let params = build_prompt_params("What is 2+2?");
        assert_eq!(params["user_input"], "What is 2+2?");
    }

    #[test]
    fn test_build_tool_result_response() {
        let result = build_tool_result_envelope("tc_1", false, "file contents here", None);
        assert_eq!(result["type"], "ToolResult");
        assert_eq!(result["payload"]["tool_call_id"], "tc_1");
        assert_eq!(result["payload"]["return_value"]["is_error"], false);
        assert_eq!(
            result["payload"]["return_value"]["output"],
            "file contents here"
        );
    }

    #[test]
    fn test_build_tool_result_error_response() {
        let result = build_tool_result_envelope("tc_2", true, "", Some("Permission denied"));
        assert_eq!(result["type"], "ToolResult");
        assert_eq!(result["payload"]["tool_call_id"], "tc_2");
        assert_eq!(result["payload"]["return_value"]["is_error"], true);
        assert_eq!(
            result["payload"]["return_value"]["message"],
            "Permission denied"
        );
    }

    #[test]
    fn test_parse_event_content_part() {
        let params = serde_json::json!({
            "type": "ContentPart",
            "payload": { "text": "Hello, world!" }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ContentPart { text } => {
                assert_eq!(text, "Hello, world!");
            }
            other => panic!("Expected ContentPart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_turn_end() {
        let params = serde_json::json!({
            "type": "TurnEnd",
            "payload": { "reason": "end_turn" }
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::TurnEnd));
    }

    #[test]
    fn test_parse_event_tool_call_request() {
        let params = serde_json::json!({
            "type": "ToolCallRequest",
            "payload": {
                "id": "tc_5",
                "name": "read_file",
                "arguments": { "path": "/etc/hosts" }
            }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ToolCallRequest {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "tc_5");
                assert_eq!(name, "read_file");
                assert_eq!(arguments["path"], "/etc/hosts");
            }
            other => panic!("Expected ToolCallRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_status_update() {
        let params = serde_json::json!({
            "type": "StatusUpdate",
            "payload": {
                "token_usage": {
                    "input_other": 150,
                    "output": 42,
                    "input_cache_read": 0,
                    "input_cache_creation": 0
                }
            }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::StatusUpdate {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 150);
                assert_eq!(output_tokens, 42);
            }
            other => panic!("Expected StatusUpdate, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_unknown_type() {
        let params = serde_json::json!({
            "type": "FutureEvent",
            "payload": { "data": 42 }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::Unknown(name) => {
                assert_eq!(name, "FutureEvent");
            }
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_thinking_part() {
        // Per wire protocol docs: think content is in the "think" field, not "text"
        let params = serde_json::json!({
            "type": "ContentPart",
            "payload": { "type": "think", "think": "Let me think about this..." }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ThinkingPart { text } => {
                assert_eq!(text, "Let me think about this...");
            }
            other => panic!("Expected ThinkingPart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_turn_begin() {
        let params = serde_json::json!({
            "type": "TurnBegin",
            "payload": {}
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::TurnBegin));
    }

    #[test]
    fn test_parse_event_step_begin() {
        let params = serde_json::json!({
            "type": "StepBegin",
            "payload": { "n": 3 }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::StepBegin { n } => assert_eq!(n, 3),
            other => panic!("Expected StepBegin, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_step_interrupted() {
        let params = serde_json::json!({
            "type": "StepInterrupted",
            "payload": {}
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::StepInterrupted));
    }

    #[test]
    fn test_parse_event_invalid_json() {
        let params = serde_json::json!("not an object");
        let event = parse_wire_event(&params);
        assert!(event.is_none());
    }

    #[test]
    fn test_wire_state_default() {
        assert_eq!(WireState::NoProcess, WireState::NoProcess);
        assert_ne!(WireState::NoProcess, WireState::Ready);
    }

    #[test]
    fn test_build_initialize_params_empty_tools() {
        let params = build_initialize_params(&[]);
        let wire_tools = params["external_tools"].as_array().unwrap();
        assert!(wire_tools.is_empty());
    }

    #[test]
    fn test_build_prompt_params_empty_input() {
        let params = build_prompt_params("");
        assert_eq!(params["user_input"], "");
    }

    #[test]
    fn test_build_tool_result_envelope_special_characters() {
        let result =
            build_tool_result_envelope("tc_3", false, "Line 1\nLine 2\tTabbed \"quoted\"", None);
        assert_eq!(
            result["payload"]["return_value"]["output"],
            "Line 1\nLine 2\tTabbed \"quoted\""
        );
    }

    #[test]
    fn test_parse_event_tool_call() {
        let params = serde_json::json!({
            "type": "ToolCall",
            "payload": {
                "type": "function",
                "id": "tc_10",
                "function": {
                    "name": "Read",
                    "arguments": "{\"path\":\"/etc/hosts\"}"
                }
            }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "tc_10");
                assert_eq!(name, "Read");
                assert_eq!(arguments, "{\"path\":\"/etc/hosts\"}");
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_tool_call_part() {
        let params = serde_json::json!({
            "type": "ToolCallPart",
            "payload": { "arguments_part": "{\"path\":" }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ToolCallPart { arguments_part } => {
                assert_eq!(arguments_part, "{\"path\":");
            }
            other => panic!("Expected ToolCallPart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_tool_call_request_string_arguments() {
        // Per wire protocol docs: arguments is a JSON-encoded string
        let params = serde_json::json!({
            "type": "ToolCallRequest",
            "payload": {
                "id": "tc_5",
                "name": "read_file",
                "arguments": "{\"path\":\"/etc/hosts\"}"
            }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ToolCallRequest {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "tc_5");
                assert_eq!(name, "read_file");
                assert_eq!(arguments["path"], "/etc/hosts");
            }
            other => panic!("Expected ToolCallRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_tool_result() {
        let params = serde_json::json!({
            "type": "ToolResult",
            "payload": {
                "tool_call_id": "tc_10",
                "return_value": {
                    "is_error": false,
                    "output": "file contents here",
                    "message": ""
                }
            }
        });
        let event = parse_wire_event(&params).unwrap();
        match event {
            WireEvent::ToolResult {
                tool_call_id,
                is_error,
                output,
            } => {
                assert_eq!(tool_call_id, "tc_10");
                assert!(!is_error);
                assert_eq!(output, "file contents here");
            }
            other => panic!("Expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_event_compaction_begin() {
        let params = serde_json::json!({
            "type": "CompactionBegin",
            "payload": {}
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::CompactionBegin));
    }

    #[test]
    fn test_parse_event_compaction_end() {
        let params = serde_json::json!({
            "type": "CompactionEnd",
            "payload": {}
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::CompactionEnd));
    }

    #[test]
    fn test_parse_event_subagent_event() {
        let params = serde_json::json!({
            "type": "SubagentEvent",
            "payload": {
                "task_tool_call_id": "tc_sub",
                "event": { "type": "ContentPart", "payload": { "text": "sub output" } }
            }
        });
        let event = parse_wire_event(&params).unwrap();
        assert!(matches!(event, WireEvent::SubagentEvent));
    }
}
