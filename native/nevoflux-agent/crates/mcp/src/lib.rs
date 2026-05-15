//! NevoFlux MCP - Model Context Protocol client
//!
//! This crate provides an MCP client implementation for connecting to MCP servers
//! via stdio or SSE transport. By default, it uses the official rmcp SDK.
//!
//! # Features
//!
//! - JSON-RPC 2.0 based protocol
//! - Stdio transport for local process communication
//! - SSE transport for HTTP-based servers
//! - Tool discovery and invocation
//! - Resource listing and reading
//! - Official rmcp SDK as default backend
//!
//! # Feature Flags
//!
//! - `legacy-backend`: Use the custom McpClient instead of the official rmcp SDK
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_mcp::{RmcpClient};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Connect to an MCP server using official rmcp SDK
//!     let client = RmcpClient::connect_stdio("npx", &["-y", "@anthropic/mcp-server-filesystem", "~"]).await?;
//!
//!     // List available tools
//!     let tools = client.list_tools().await?;
//!     for tool in &tools {
//!         println!("Tool: {} - {}", tool.name, tool.description);
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod backend;
pub mod client;
pub mod command;
pub mod error;
pub mod manager;
pub mod registry;
pub mod rmcp_adapter;
pub mod search;
pub mod server;
pub mod tools;
pub mod transport;
pub mod types;

pub use backend::McpClientBackend;
pub use client::McpClient;
pub use error::{McpError, Result};
pub use manager::{ManagerConfig, McpManager, ServerStatus};
pub use registry::{McpRegistry, ServerConfig, ServerResource, ServerTool};
pub use rmcp_adapter::RmcpClient;
pub use search::{Bm25Config, SearchResult, ToolSearchIndex};
pub use server::{run_stdio_server, McpServer, McpServerConfig, PROTOCOL_VERSION};
pub use tools::create_tools;
pub use transport::{McpTransport, StdioTransport};
pub use types::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, Resource, ResourceContent, ServerCapabilities,
    ServerInfo, ToolDefinition, ToolResult, ToolResultContent,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_types_tool_definition_serialization() {
        // RED: ToolDefinition should serialize/deserialize correctly
        let tool = ToolDefinition {
            name: "read_file".to_string(),
            description: "Read contents of a file".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let decoded: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(tool.name, decoded.name);
    }

    #[test]
    fn test_types_json_rpc_request() {
        // RED: JsonRpcRequest should be constructable
        let request = JsonRpcRequest::new("tools/list", None);
        assert_eq!(request.method, "tools/list");
        assert_eq!(request.jsonrpc, "2.0");
    }

    #[test]
    fn test_types_json_rpc_response_success() {
        // RED: JsonRpcResponse should parse success responses
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_types_json_rpc_response_error() {
        // RED: JsonRpcResponse should parse error responses
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn test_error_types_exist() {
        // RED: Error types should be usable
        let err = McpError::ConnectionFailed("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
