//! MCP protocol types.
//!
//! Based on the Model Context Protocol specification (JSON-RPC 2.0).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global request ID counter.
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique request ID.
pub fn next_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Request ID.
    pub id: u64,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: next_request_id(),
            method: method.into(),
            params,
        }
    }

    /// Create a request with specific ID.
    pub fn with_id(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Request ID this is responding to.
    pub id: Option<u64>,
    /// Result (present on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error (present on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: u64, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            result: None,
            error: Some(error),
        }
    }

    /// Create a success response with a JSON Value id.
    pub fn success_with_value_id(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.as_u64(),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response with a JSON Value id and simple error parameters.
    pub fn error_simple(id: serde_json::Value, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.as_u64(),
            result: None,
            error: Some(JsonRpcError::new(code, message)),
        }
    }

    /// Check if this is a success response.
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// Check if this is an error response.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Standard error: Parse error.
    pub const PARSE_ERROR: i32 = -32700;
    /// Standard error: Invalid request.
    pub const INVALID_REQUEST: i32 = -32600;
    /// Standard error: Method not found.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Standard error: Invalid params.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Standard error: Internal error.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// Create a new error.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Create error with data.
    pub fn with_data(code: i32, message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// JSON-RPC 2.0 notification (no id, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Create a new notification.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// MCP server information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
    /// Protocol version supported.
    #[serde(default)]
    pub protocol_version: Option<String>,
}

/// MCP server capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Whether the server supports tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    /// Whether the server supports resources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    /// Whether the server supports prompts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
}

/// Tools capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether tool list can change.
    #[serde(default)]
    pub list_changed: bool,
}

/// Resources capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Whether resource list can change.
    #[serde(default)]
    pub list_changed: bool,
    /// Whether the server supports subscriptions.
    #[serde(default)]
    pub subscribe: bool,
}

/// Prompts capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsCapability {
    /// Whether prompt list can change.
    #[serde(default)]
    pub list_changed: bool,
}

/// MCP tool definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Tool description.
    #[serde(default)]
    pub description: String,
    /// JSON Schema for tool input.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Result of calling a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Content returned by the tool.
    pub content: Vec<ToolResultContent>,
    /// Whether the tool call resulted in an error.
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// Content item in a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResultContent {
    /// Text content.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Image content.
    #[serde(rename = "image")]
    Image {
        /// Base64 encoded image data.
        data: String,
        /// MIME type.
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// Resource reference.
    #[serde(rename = "resource")]
    Resource {
        /// Resource URI.
        uri: String,
        /// Resource MIME type.
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
        /// Resource text content.
        text: Option<String>,
    },
}

/// MCP resource definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource URI.
    pub uri: String,
    /// Resource name.
    pub name: String,
    /// Resource description.
    #[serde(default)]
    pub description: Option<String>,
    /// Resource MIME type.
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Content of a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    /// Resource URI.
    pub uri: String,
    /// Resource MIME type.
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    /// Text content (if text-based).
    pub text: Option<String>,
    /// Binary content (base64 encoded, if binary).
    pub blob: Option<String>,
}

/// Initialize request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Client capabilities.
    pub capabilities: ClientCapabilities,
    /// Client information.
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

impl Default for InitializeParams {
    fn default() -> Self {
        Self {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "nevoflux-agent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }
}

/// Client capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Sampling capability.
    #[serde(default)]
    pub sampling: Option<serde_json::Value>,
    /// Roots capability.
    #[serde(default)]
    pub roots: Option<RootsCapability>,
}

/// Roots capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootsCapability {
    /// Whether roots list can change.
    #[serde(default)]
    pub list_changed: bool,
}

/// Client information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name.
    pub name: String,
    /// Client version.
    pub version: String,
}

/// Initialize response result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Protocol version.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server information.
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_new() {
        let req = JsonRpcRequest::new("test/method", Some(serde_json::json!({"key": "value"})));

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "test/method");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest::with_id(1, "tools/list", None);
        let json = serde_json::to_string(&req).unwrap();

        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse::success(1, serde_json::json!({"tools": []}));

        assert!(resp.is_success());
        assert!(!resp.is_error());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let error = JsonRpcError::new(-32601, "Method not found");
        let resp = JsonRpcResponse::error(1, error);

        assert!(!resp.is_success());
        assert!(resp.is_error());
    }

    #[test]
    fn test_json_rpc_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();

        assert_eq!(resp.id, Some(1));
        assert!(resp.is_success());
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"read_file\""));
        assert!(json.contains("\"inputSchema\""));

        let decoded: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(tool, decoded);
    }

    #[test]
    fn test_tool_result_text() {
        let result = ToolResult {
            content: vec![ToolResultContent::Text {
                text: "Hello, world!".to_string(),
            }],
            is_error: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("Hello, world!"));
    }

    #[test]
    fn test_resource_serialization() {
        let resource = Resource {
            uri: "file:///home/user/test.txt".to_string(),
            name: "test.txt".to_string(),
            description: Some("A test file".to_string()),
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&resource).unwrap();
        let decoded: Resource = serde_json::from_str(&json).unwrap();

        assert_eq!(resource.uri, decoded.uri);
        assert_eq!(resource.name, decoded.name);
    }

    #[test]
    fn test_initialize_params_default() {
        let params = InitializeParams::default();

        assert!(!params.protocol_version.is_empty());
        assert_eq!(params.client_info.name, "nevoflux-agent");
    }

    #[test]
    fn test_server_capabilities_default() {
        let caps = ServerCapabilities::default();

        assert!(caps.tools.is_none());
        assert!(caps.resources.is_none());
    }

    #[test]
    fn test_json_rpc_notification() {
        let notif = JsonRpcNotification::new("initialized", None);

        assert_eq!(notif.jsonrpc, "2.0");
        assert_eq!(notif.method, "initialized");

        let json = serde_json::to_string(&notif).unwrap();
        assert!(!json.contains("\"id\""));
    }
}
