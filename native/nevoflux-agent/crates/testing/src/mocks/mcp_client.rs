//! Mock MCP client for testing.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A mock MCP client for testing MCP client functionality without real server connections.
///
/// Allows configuring tool responses, resource listings, and error conditions
/// for testing MCP client code paths.
#[derive(Debug, Clone)]
pub struct MockMcpClient {
    /// Configured tool responses.
    tool_responses: Arc<RwLock<HashMap<String, MockToolResponse>>>,
    /// Configured resources.
    resources: Arc<RwLock<Vec<MockResource>>>,
    /// Record of tool calls.
    call_history: Arc<RwLock<Vec<ToolCallRecord>>>,
    /// Whether the client is connected.
    connected: Arc<RwLock<bool>>,
}

/// A mock tool response configuration.
#[derive(Debug, Clone)]
pub enum MockToolResponse {
    /// Return a successful result.
    Success(Value),
    /// Return an error with code and message.
    Error { code: i32, message: String },
}

/// A mock resource.
#[derive(Debug, Clone)]
pub struct MockResource {
    /// Resource URI.
    pub uri: String,
    /// Resource name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// Resource content.
    pub content: String,
}

/// Record of a tool call made to the mock client.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// Tool name that was called.
    pub tool_name: String,
    /// Arguments passed to the tool.
    pub arguments: Value,
}

impl Default for MockMcpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockMcpClient {
    /// Create a new mock MCP client.
    pub fn new() -> Self {
        Self {
            tool_responses: Arc::new(RwLock::new(HashMap::new())),
            resources: Arc::new(RwLock::new(Vec::new())),
            call_history: Arc::new(RwLock::new(Vec::new())),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Configure a tool to return a successful response.
    pub fn with_tool_success(self, tool_name: impl Into<String>, result: Value) -> Self {
        self.tool_responses
            .write()
            .unwrap()
            .insert(tool_name.into(), MockToolResponse::Success(result));
        self
    }

    /// Configure a tool to return an error.
    pub fn with_tool_error(
        self,
        tool_name: impl Into<String>,
        code: i32,
        message: impl Into<String>,
    ) -> Self {
        self.tool_responses.write().unwrap().insert(
            tool_name.into(),
            MockToolResponse::Error {
                code,
                message: message.into(),
            },
        );
        self
    }

    /// Add a mock resource.
    pub fn with_resource(self, resource: MockResource) -> Self {
        self.resources.write().unwrap().push(resource);
        self
    }

    /// Set the connected state.
    pub fn set_connected(&self, connected: bool) {
        *self.connected.write().unwrap() = connected;
    }

    /// Set the connected state (builder pattern).
    pub fn with_connected(self, connected: bool) -> Self {
        self.set_connected(connected);
        self
    }

    /// Check if the client is connected.
    pub fn is_connected(&self) -> bool {
        *self.connected.read().unwrap()
    }

    /// Simulate calling a tool and return the configured response.
    pub fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, MockMcpError> {
        // Record the call
        self.call_history.write().unwrap().push(ToolCallRecord {
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
        });

        // Check if connected
        if !self.is_connected() {
            return Err(MockMcpError::NotConnected);
        }

        // Return configured response
        match self.tool_responses.read().unwrap().get(tool_name) {
            Some(MockToolResponse::Success(value)) => Ok(value.clone()),
            Some(MockToolResponse::Error { code, message }) => Err(MockMcpError::ToolError {
                code: *code,
                message: message.clone(),
            }),
            None => Err(MockMcpError::ToolNotFound(tool_name.to_string())),
        }
    }

    /// List configured resources.
    pub fn list_resources(&self) -> Result<Vec<MockResource>, MockMcpError> {
        if !self.is_connected() {
            return Err(MockMcpError::NotConnected);
        }
        Ok(self.resources.read().unwrap().clone())
    }

    /// Read a resource by URI.
    pub fn read_resource(&self, uri: &str) -> Result<String, MockMcpError> {
        if !self.is_connected() {
            return Err(MockMcpError::NotConnected);
        }
        self.resources
            .read()
            .unwrap()
            .iter()
            .find(|r| r.uri == uri)
            .map(|r| r.content.clone())
            .ok_or_else(|| MockMcpError::ResourceNotFound(uri.to_string()))
    }

    /// Get the call history.
    pub fn call_history(&self) -> Vec<ToolCallRecord> {
        self.call_history.read().unwrap().clone()
    }

    /// Clear the call history.
    pub fn clear_history(&self) {
        self.call_history.write().unwrap().clear();
    }

    /// Get the number of times a tool was called.
    pub fn call_count(&self, tool_name: &str) -> usize {
        self.call_history
            .read()
            .unwrap()
            .iter()
            .filter(|r| r.tool_name == tool_name)
            .count()
    }
}

/// Errors that can occur in the mock MCP client.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MockMcpError {
    #[error("not connected to server")]
    NotConnected,
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("resource not found: {0}")]
    ResourceNotFound(String),
    #[error("tool error ({code}): {message}")]
    ToolError { code: i32, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_mcp_client_default() {
        let client = MockMcpClient::new();
        assert!(!client.is_connected());
    }

    #[test]
    fn test_mock_mcp_client_connect() {
        let client = MockMcpClient::new();
        client.set_connected(true);
        assert!(client.is_connected());
    }

    #[test]
    fn test_mock_mcp_client_tool_success() {
        let client = MockMcpClient::new()
            .with_tool_success("read_file", serde_json::json!({"content": "hello"}));
        client.set_connected(true);

        let result = client.call_tool("read_file", serde_json::json!({"path": "/test"}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["content"], "hello");
    }

    #[test]
    fn test_mock_mcp_client_tool_error() {
        let client =
            MockMcpClient::new().with_tool_error("write_file", -32602, "Permission denied");
        client.set_connected(true);

        let result = client.call_tool("write_file", serde_json::json!({"path": "/test"}));
        assert!(result.is_err());
        match result.unwrap_err() {
            MockMcpError::ToolError { code, message } => {
                assert_eq!(code, -32602);
                assert_eq!(message, "Permission denied");
            }
            _ => panic!("Expected ToolError"),
        }
    }

    #[test]
    fn test_mock_mcp_client_tool_not_found() {
        let client = MockMcpClient::new();
        client.set_connected(true);

        let result = client.call_tool("unknown_tool", serde_json::json!({}));
        assert!(matches!(result.unwrap_err(), MockMcpError::ToolNotFound(_)));
    }

    #[test]
    fn test_mock_mcp_client_not_connected() {
        let client = MockMcpClient::new()
            .with_tool_success("read_file", serde_json::json!({"content": "hello"}));

        let result = client.call_tool("read_file", serde_json::json!({}));
        assert!(matches!(result.unwrap_err(), MockMcpError::NotConnected));
    }

    #[test]
    fn test_mock_mcp_client_call_history() {
        let client = MockMcpClient::new()
            .with_tool_success("tool1", serde_json::json!({}))
            .with_tool_success("tool2", serde_json::json!({}));
        client.set_connected(true);

        client
            .call_tool("tool1", serde_json::json!({"a": 1}))
            .unwrap();
        client
            .call_tool("tool2", serde_json::json!({"b": 2}))
            .unwrap();
        client
            .call_tool("tool1", serde_json::json!({"a": 3}))
            .unwrap();

        let history = client.call_history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].tool_name, "tool1");
        assert_eq!(history[1].tool_name, "tool2");
        assert_eq!(history[2].tool_name, "tool1");

        assert_eq!(client.call_count("tool1"), 2);
        assert_eq!(client.call_count("tool2"), 1);
    }

    #[test]
    fn test_mock_mcp_client_resources() {
        let client = MockMcpClient::new().with_resource(MockResource {
            uri: "file:///test.txt".into(),
            name: "test.txt".into(),
            description: Some("A test file".into()),
            mime_type: Some("text/plain".into()),
            content: "Hello, world!".into(),
        });
        client.set_connected(true);

        let resources = client.list_resources().unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "test.txt");

        let content = client.read_resource("file:///test.txt").unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[test]
    fn test_mock_mcp_client_resource_not_found() {
        let client = MockMcpClient::new();
        client.set_connected(true);

        let result = client.read_resource("file:///unknown.txt");
        assert!(matches!(
            result.unwrap_err(),
            MockMcpError::ResourceNotFound(_)
        ));
    }
}
