// crates/protocol/src/mcp.rs

//! MCP channel message definitions.
//!
//! Messages for Browser Use API communication via MCP protocol.

use serde::{Deserialize, Serialize};

/// JSON-RPC ID (can be number or string)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    String(String),
}

/// MCP message source
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSource {
    /// Agent identifier
    pub agent: String,
    /// Session ID (optional)
    pub session_id: Option<String>,
}

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,
    /// Request ID
    pub id: JsonRpcId,
    /// Method name
    pub method: String,
    /// Method parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,
    /// Request ID
    pub id: JsonRpcId,
    /// Result (mutually exclusive with error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error (mutually exclusive with result)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// MCP request from Agent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRequest {
    /// Request ID
    pub request_id: String,
    /// Request source
    pub source: McpSource,
    /// JSON-RPC payload
    pub payload: JsonRpcRequest,
}

/// MCP response from Extension
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpResponse {
    /// Request ID
    pub request_id: String,
    /// JSON-RPC payload
    pub payload: JsonRpcResponse,
}

/// All MCP channel messages
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum McpMessage {
    McpRequest(McpRequest),
    McpResponse(McpResponse),
}

/// Standard JSON-RPC error codes
pub mod error_codes {
    /// Parse error - Invalid JSON was received
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid Request - The JSON sent is not a valid Request object
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error
    pub const INTERNAL_ERROR: i32 = -32603;
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    pub fn new(id: impl Into<JsonRpcId>, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    /// Add parameters to the request
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        self.params = Some(params);
        self
    }
}

impl JsonRpcResponse {
    /// Create a success response
    pub fn success(id: JsonRpcId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: JsonRpcId, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Check if the response is successful
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }
}

impl From<i64> for JsonRpcId {
    fn from(value: i64) -> Self {
        JsonRpcId::Number(value)
    }
}

impl From<String> for JsonRpcId {
    fn from(value: String) -> Self {
        JsonRpcId::String(value)
    }
}

impl From<&str> for JsonRpcId {
    fn from(value: &str) -> Self {
        JsonRpcId::String(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_request_serialization() {
        let req = McpRequest {
            request_id: "mcp-001".into(),
            source: McpSource {
                agent: "claude-code".into(),
                session_id: Some("sess-001".into()),
            },
            payload: JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: JsonRpcId::Number(1),
                method: "browser_use/click".into(),
                params: Some(serde_json::json!({"selector": "#btn"})),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"method\":\"browser_use/click\""));

        let decoded: McpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_mcp_response_success() {
        let resp = McpResponse {
            request_id: "mcp-001".into(),
            payload: JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: JsonRpcId::Number(1),
                result: Some(serde_json::json!({"clicked": true})),
                error: None,
            },
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));

        let decoded: McpResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_mcp_response_error() {
        let resp = McpResponse {
            request_id: "mcp-001".into(),
            payload: JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: JsonRpcId::String("req-1".into()),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Method not found".into(),
                    data: None,
                }),
            },
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":-32601"));

        let decoded: McpResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_jsonrpc_id_variants() {
        let num_id = JsonRpcId::Number(42);
        let str_id = JsonRpcId::String("abc".into());

        assert_eq!(serde_json::to_string(&num_id).unwrap(), "42");
        assert_eq!(serde_json::to_string(&str_id).unwrap(), "\"abc\"");

        let decoded_num: JsonRpcId = serde_json::from_str("42").unwrap();
        let decoded_str: JsonRpcId = serde_json::from_str("\"abc\"").unwrap();

        assert_eq!(decoded_num, num_id);
        assert_eq!(decoded_str, str_id);
    }

    #[test]
    fn test_mcp_message_tagged() {
        let msg = McpMessage::McpRequest(McpRequest {
            request_id: "mcp-001".into(),
            source: McpSource {
                agent: "test".into(),
                session_id: None,
            },
            payload: JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: JsonRpcId::Number(1),
                method: "test".into(),
                params: None,
            },
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"mcp_request\""));

        let decoded: McpMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, McpMessage::McpRequest(_)));
    }

    #[test]
    fn test_jsonrpc_request_builder() {
        let req = JsonRpcRequest::new(1i64, "test_method")
            .with_params(serde_json::json!({"key": "value"}));

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "test_method");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_jsonrpc_response_helpers() {
        let success =
            JsonRpcResponse::success(JsonRpcId::Number(1), serde_json::json!({"ok": true}));
        assert!(success.is_success());

        let error = JsonRpcResponse::error(
            JsonRpcId::Number(1),
            error_codes::METHOD_NOT_FOUND,
            "Method not found",
        );
        assert!(!error.is_success());
    }
}
