//! Error types for MCP client.

use thiserror::Error;

/// Result type for MCP operations.
pub type Result<T> = std::result::Result<T, McpError>;

/// Errors that can occur during MCP operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Failed to connect to MCP server.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Failed to spawn MCP server process.
    #[error("Failed to spawn process: {0}")]
    SpawnFailed(String),

    /// Transport error during communication.
    #[error("Transport error: {0}")]
    TransportError(String),

    /// JSON-RPC error returned by server.
    #[error("RPC error ({code}): {message}")]
    RpcError {
        /// Error code.
        code: i32,
        /// Error message.
        message: String,
        /// Additional error data.
        data: Option<serde_json::Value>,
    },

    /// Failed to serialize request.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Failed to deserialize response.
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// Request timed out.
    #[error("Request timed out after {0}ms")]
    Timeout(u64),

    /// Server returned unexpected response.
    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),

    /// Server not initialized.
    #[error("Server not initialized")]
    NotInitialized,

    /// Tool not found.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Resource not found.
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        McpError::DeserializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = McpError::ConnectionFailed("localhost:8080".to_string());
        assert!(err.to_string().contains("localhost:8080"));
    }

    #[test]
    fn test_rpc_error() {
        let err = McpError::RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        };
        assert!(err.to_string().contains("-32601"));
        assert!(err.to_string().contains("Method not found"));
    }

    #[test]
    fn test_timeout_error() {
        let err = McpError::Timeout(5000);
        assert!(err.to_string().contains("5000"));
    }

    #[test]
    fn test_from_serde_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let mcp_err: McpError = json_err.into();
        assert!(matches!(mcp_err, McpError::DeserializationError(_)));
    }
}
