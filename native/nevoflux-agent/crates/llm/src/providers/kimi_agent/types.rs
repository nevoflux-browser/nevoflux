//! Wire protocol types for the kimi-agent CLI provider.
//!
//! The kimi-agent wire protocol uses JSON-RPC 2.0. Messages are sent as envelopes
//! with `{type, payload}` inside JSON-RPC params. Server events arrive as JSON-RPC
//! notifications (`method: "event"`) or requests (`method: "request"`). Client
//! messages are JSON-RPC requests (for `initialize`, `prompt`) or responses (for
//! `ToolCallRequest` replies).

use rig::completion::{
    AssistantContent, CompletionError, CompletionResponse, ToolDefinition, Usage,
};
use rig::OneOrMany;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wire envelope
// ---------------------------------------------------------------------------

/// Envelope wrapping every message inside JSON-RPC `params`.
///
/// ```json
/// { "type": "ContentPart", "payload": { "text": "Hello" } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessageEnvelope {
    /// Discriminator tag, e.g. `"ContentPart"`, `"ToolCallRequest"`, `"TurnEnd"`.
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Arbitrary JSON payload whose shape depends on `msg_type`.
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 primitives
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Union of all JSON-RPC 2.0 message shapes we encounter on the wire.
///
/// Deserialization uses serde's untagged enum strategy since JSON-RPC messages
/// are distinguished by field presence rather than a tag field.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// An error response (has `id` and `error`).
    /// Checked first because `error` field is the most specific discriminator.
    Error {
        jsonrpc: String,
        id: serde_json::Value,
        error: JsonRpcError,
    },
    /// A successful response (has `id` and `result`).
    Response {
        jsonrpc: String,
        id: serde_json::Value,
        result: serde_json::Value,
    },
    /// A request from the server (has both `method` and `id`).
    /// Must come before `Notification` since both have `method`, but only
    /// `Request` requires `id`.
    Request {
        jsonrpc: String,
        id: serde_json::Value,
        method: String,
        #[serde(default)]
        params: Option<serde_json::Value>,
    },
    /// A notification from the server (has `method` but no `id`).
    Notification {
        jsonrpc: String,
        method: String,
        #[serde(default)]
        params: Option<serde_json::Value>,
    },
}

/// An outbound JSON-RPC 2.0 request (client -> server).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC 2.0 request with the given id, method, and params.
    pub fn new(id: u64, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::String(id.to_string()),
            method: method.into(),
            params,
        }
    }
}

/// An outbound JSON-RPC 2.0 response (client -> server, e.g. for ToolCallRequest).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: serde_json::Value,
}

impl JsonRpcResponse {
    /// Create a new JSON-RPC 2.0 response for the given request id.
    pub fn new(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool definitions for the initialize handshake
// ---------------------------------------------------------------------------

/// External tool definition sent during the `initialize` handshake.
///
/// This is the kimi-agent wire format for advertising available tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireExternalTool {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub parameters: serde_json::Value,
}

impl From<&ToolDefinition> for WireExternalTool {
    fn from(td: &ToolDefinition) -> Self {
        Self {
            name: td.name.clone(),
            description: td.description.clone(),
            parameters: td.parameters.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Extracted data from wire events
// ---------------------------------------------------------------------------

/// A tool call extracted from wire `ToolCallRequest` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedToolCall {
    /// Unique identifier for this tool call.
    pub id: String,
    /// Tool name to invoke.
    pub name: String,
    /// Arguments as a JSON value.
    pub arguments: serde_json::Value,
}

/// Token usage from a `StatusUpdate` event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KimiUsage {
    /// Number of input (prompt) tokens.
    #[serde(default)]
    pub input_tokens: u64,
    /// Number of output (completion) tokens.
    #[serde(default)]
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------
// Completion response wrapper
// ---------------------------------------------------------------------------

/// Wrapper that accumulates wire events into a structure convertible to
/// rig's `CompletionResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiAgentCompletionResponse {
    /// Accumulated text content from `ContentPart` events.
    pub content: String,
    /// Token usage from `StatusUpdate` events.
    pub usage: KimiUsage,
    /// Tool calls extracted from `ToolCallRequest` events.
    #[serde(default)]
    pub tool_calls: Vec<ExtractedToolCall>,
}

impl TryFrom<KimiAgentCompletionResponse> for CompletionResponse<KimiAgentCompletionResponse> {
    type Error = CompletionError;

    fn try_from(value: KimiAgentCompletionResponse) -> Result<Self, Self::Error> {
        let usage = Usage {
            input_tokens: value.usage.input_tokens,
            output_tokens: value.usage.output_tokens,
            total_tokens: value.usage.input_tokens + value.usage.output_tokens,
        };

        if value.content.is_empty() && value.tool_calls.is_empty() {
            return Err(CompletionError::ResponseError(
                "Empty response from kimi-agent".into(),
            ));
        }

        // Build content list: text + tool calls
        let mut contents: Vec<AssistantContent> = Vec::new();
        if !value.content.is_empty() {
            contents.push(AssistantContent::text(&value.content));
        }
        for tc in &value.tool_calls {
            contents.push(AssistantContent::ToolCall(rig::message::ToolCall::new(
                tc.id.clone(),
                rig::message::ToolFunction::new(tc.name.clone(), tc.arguments.clone()),
            )));
        }

        let choice = OneOrMany::many(contents)
            .map_err(|_| CompletionError::ResponseError("Empty response from kimi-agent".into()))?;

        Ok(CompletionResponse {
            choice,
            usage,
            raw_response: value,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- WireMessageEnvelope deserialization --

    #[test]
    fn test_wire_envelope_content_part() {
        let json = r#"{
            "type": "ContentPart",
            "payload": { "text": "Hello, world!" }
        }"#;
        let env: WireMessageEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.msg_type, "ContentPart");
        assert_eq!(env.payload["text"], "Hello, world!");
    }

    #[test]
    fn test_wire_envelope_tool_call_request() {
        let json = r#"{
            "type": "ToolCallRequest",
            "payload": {
                "id": "tc_1",
                "name": "read_file",
                "arguments": { "path": "/etc/hosts" }
            }
        }"#;
        let env: WireMessageEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.msg_type, "ToolCallRequest");
        assert_eq!(env.payload["id"], "tc_1");
        assert_eq!(env.payload["name"], "read_file");
        assert_eq!(env.payload["arguments"]["path"], "/etc/hosts");
    }

    #[test]
    fn test_wire_envelope_turn_end() {
        let json = r#"{
            "type": "TurnEnd",
            "payload": { "reason": "end_turn" }
        }"#;
        let env: WireMessageEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.msg_type, "TurnEnd");
        assert_eq!(env.payload["reason"], "end_turn");
    }

    #[test]
    fn test_wire_envelope_status_update() {
        let json = r#"{
            "type": "StatusUpdate",
            "payload": {
                "input_tokens": 150,
                "output_tokens": 42
            }
        }"#;
        let env: WireMessageEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.msg_type, "StatusUpdate");
        assert_eq!(env.payload["input_tokens"], 150);
        assert_eq!(env.payload["output_tokens"], 42);
    }

    // -- JsonRpcMessage deserialization --

    #[test]
    fn test_jsonrpc_notification_event() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "ContentPart",
                "payload": { "text": "hi" }
            }
        }"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Notification { method, params, .. } => {
                assert_eq!(method, "event");
                let params = params.unwrap();
                let env: WireMessageEnvelope = serde_json::from_value(params).unwrap();
                assert_eq!(env.msg_type, "ContentPart");
            }
            other => panic!("Expected Notification, got {:?}", other),
        }
    }

    #[test]
    fn test_jsonrpc_response() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "status": "ok" }
        }"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Response { id, result, .. } => {
                assert_eq!(id, serde_json::json!(1));
                assert_eq!(result["status"], "ok");
            }
            other => panic!("Expected Response, got {:?}", other),
        }
    }

    #[test]
    fn test_jsonrpc_request_from_server() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 42,
            "method": "request",
            "params": {
                "type": "ToolCallRequest",
                "payload": {
                    "id": "tc_5",
                    "name": "screenshot",
                    "arguments": {}
                }
            }
        }"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Request {
                id, method, params, ..
            } => {
                assert_eq!(id, serde_json::json!(42));
                assert_eq!(method, "request");
                let env: WireMessageEnvelope = serde_json::from_value(params.unwrap()).unwrap();
                assert_eq!(env.msg_type, "ToolCallRequest");
                assert_eq!(env.payload["name"], "screenshot");
            }
            other => panic!("Expected Request, got {:?}", other),
        }
    }

    #[test]
    fn test_jsonrpc_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32600,
                "message": "Invalid Request",
                "data": null
            }
        }"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Error { id, error, .. } => {
                assert_eq!(id, serde_json::json!(1));
                assert_eq!(error.code, -32600);
                assert_eq!(error.message, "Invalid Request");
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    // -- JsonRpcRequest serialization --

    #[test]
    fn test_jsonrpc_request_serialize_initialize() {
        let tools = vec![WireExternalTool {
            name: "bash".into(),
            description: "Run a shell command".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }];

        let req = JsonRpcRequest::new(
            1,
            "initialize",
            Some(serde_json::json!({
                "tools": tools,
                "system_prompt": "You are a helpful assistant."
            })),
        );

        let serialized = serde_json::to_value(&req).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], "1");
        assert_eq!(serialized["method"], "initialize");
        assert_eq!(serialized["params"]["tools"][0]["name"], "bash");
        assert_eq!(
            serialized["params"]["system_prompt"],
            "You are a helpful assistant."
        );
    }

    #[test]
    fn test_jsonrpc_request_serialize_prompt() {
        let req = JsonRpcRequest::new(
            2,
            "prompt",
            Some(serde_json::json!({
                "messages": [
                    { "role": "user", "content": "What is 2+2?" }
                ]
            })),
        );

        let serialized = serde_json::to_value(&req).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], "2");
        assert_eq!(serialized["method"], "prompt");
        assert_eq!(
            serialized["params"]["messages"][0]["content"],
            "What is 2+2?"
        );
    }

    // -- JsonRpcResponse serialization --

    #[test]
    fn test_jsonrpc_response_serialize_tool_result() {
        let resp = JsonRpcResponse::new(
            serde_json::json!(42),
            serde_json::json!({
                "type": "ToolResult",
                "payload": {
                    "id": "tc_5",
                    "content": "screenshot taken successfully"
                }
            }),
        );

        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], 42);
        assert_eq!(serialized["result"]["type"], "ToolResult");
        assert_eq!(serialized["result"]["payload"]["id"], "tc_5");
        assert_eq!(
            serialized["result"]["payload"]["content"],
            "screenshot taken successfully"
        );
    }

    // -- WireExternalTool From<&ToolDefinition> --

    #[test]
    fn test_wire_external_tool_from_tool_definition() {
        let td = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from disk".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };

        let wire: WireExternalTool = (&td).into();
        assert_eq!(wire.name, "read_file");
        assert_eq!(wire.description, "Read a file from disk");
        assert_eq!(wire.parameters["required"][0], "path");
    }

    // -- KimiAgentCompletionResponse TryFrom --

    #[test]
    fn test_completion_response_text_only() {
        let resp = KimiAgentCompletionResponse {
            content: "The answer is 42.".into(),
            usage: KimiUsage {
                input_tokens: 10,
                output_tokens: 8,
            },
            tool_calls: Vec::new(),
        };

        let rig_resp: CompletionResponse<KimiAgentCompletionResponse> = resp.try_into().unwrap();
        let first = rig_resp.choice.first();
        assert!(matches!(first, AssistantContent::Text(_)));
        assert_eq!(rig_resp.usage.input_tokens, 10);
        assert_eq!(rig_resp.usage.output_tokens, 8);
        assert_eq!(rig_resp.usage.total_tokens, 18);
    }

    #[test]
    fn test_completion_response_with_tool_calls() {
        let resp = KimiAgentCompletionResponse {
            content: "Let me check that file.".into(),
            usage: KimiUsage {
                input_tokens: 20,
                output_tokens: 15,
            },
            tool_calls: vec![ExtractedToolCall {
                id: "tc_1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "/etc/hosts"}),
            }],
        };

        let rig_resp: CompletionResponse<KimiAgentCompletionResponse> = resp.try_into().unwrap();
        let items: Vec<_> = rig_resp.choice.iter().collect();
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], AssistantContent::Text(_)));
        assert!(matches!(items[1], AssistantContent::ToolCall(_)));
        assert_eq!(rig_resp.usage.total_tokens, 35);
    }

    #[test]
    fn test_completion_response_tool_calls_only() {
        let resp = KimiAgentCompletionResponse {
            content: String::new(),
            usage: KimiUsage {
                input_tokens: 5,
                output_tokens: 10,
            },
            tool_calls: vec![ExtractedToolCall {
                id: "tc_1".into(),
                name: "screenshot".into(),
                arguments: serde_json::json!({}),
            }],
        };

        let rig_resp: CompletionResponse<KimiAgentCompletionResponse> = resp.try_into().unwrap();
        let first = rig_resp.choice.first();
        assert!(matches!(first, AssistantContent::ToolCall(_)));
    }

    #[test]
    fn test_completion_response_empty_error() {
        let resp = KimiAgentCompletionResponse {
            content: String::new(),
            usage: KimiUsage::default(),
            tool_calls: Vec::new(),
        };

        let result: Result<CompletionResponse<KimiAgentCompletionResponse>, _> = resp.try_into();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Empty response"),
            "Error should mention empty response, got: {}",
            err
        );
    }

    // -- KimiUsage defaults --

    #[test]
    fn test_kimi_usage_default() {
        let usage = KimiUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    // -- Round-trip serialization --

    #[test]
    fn test_wire_envelope_round_trip() {
        let original = WireMessageEnvelope {
            msg_type: "ContentPart".into(),
            payload: serde_json::json!({"text": "hello"}),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: WireMessageEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.msg_type, original.msg_type);
        assert_eq!(restored.payload, original.payload);
    }

    #[test]
    fn test_extracted_tool_call_round_trip() {
        let original = ExtractedToolCall {
            id: "tc_99".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "a.txt", "content": "hello"}),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: ExtractedToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "tc_99");
        assert_eq!(restored.name, "write_file");
        assert_eq!(restored.arguments["content"], "hello");
    }
}
