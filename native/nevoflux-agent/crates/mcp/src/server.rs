//! MCP server implementation for stdio communication.
//!
//! This module provides an MCP server that can communicate via stdin/stdout
//! using the JSON-RPC 2.0 protocol.

use crate::error::Result;
use crate::types::{
    InitializeResult, JsonRpcError, JsonRpcRequest, JsonRpcResponse, ServerCapabilities,
    ServerInfo, ToolDefinition, ToolResult, ToolResultContent, ToolsCapability,
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

/// MCP protocol version supported by this server.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Configuration for the MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "nevoflux-agent".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// MCP server that handles JSON-RPC requests.
#[derive(Debug, Clone)]
pub struct McpServer {
    /// Server configuration.
    config: McpServerConfig,
    /// Registered tools.
    tools: Vec<ToolDefinition>,
}

impl McpServer {
    /// Create a new MCP server with default configuration.
    pub fn new() -> Self {
        Self {
            config: McpServerConfig::default(),
            tools: Vec::new(),
        }
    }

    /// Create a new MCP server with custom configuration.
    pub fn with_config(config: McpServerConfig) -> Self {
        Self {
            config,
            tools: Vec::new(),
        }
    }

    /// Register a tool with the server.
    pub fn register_tool(&mut self, tool: ToolDefinition) {
        self.tools.push(tool);
    }

    /// Get server information.
    pub fn server_info(&self) -> ServerInfo {
        ServerInfo {
            name: self.config.name.clone(),
            version: self.config.version.clone(),
            protocol_version: Some(PROTOCOL_VERSION.to_string()),
        }
    }

    /// Get server capabilities.
    pub fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: false,
            }),
            resources: None,
            prompts: None,
        }
    }

    /// Get the list of registered tools.
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    /// Handle a JSON-RPC request and return a response.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        debug!("Handling request: method={}", request.method);

        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tools_call(request),
            _ => self.handle_unknown_method(request),
        }
    }

    /// Handle the initialize request.
    fn handle_initialize(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: self.capabilities(),
            server_info: self.server_info(),
        };

        JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle the tools/list request.
    fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let result = serde_json::json!({
            "tools": self.tools
        });

        JsonRpcResponse::success(request.id, result)
    }

    /// Handle the tools/call request.
    fn handle_tools_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        // Extract tool name from params
        let tool_name = request
            .params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());

        let Some(tool_name) = tool_name else {
            return JsonRpcResponse::error(
                request.id,
                JsonRpcError::new(
                    JsonRpcError::INVALID_PARAMS,
                    "Missing 'name' parameter in tools/call request",
                ),
            );
        };

        // Check if tool exists
        let tool_exists = self.tools.iter().any(|t| t.name == tool_name);
        if !tool_exists {
            let result = ToolResult {
                content: vec![ToolResultContent::Text {
                    text: format!("Tool '{}' not found", tool_name),
                }],
                is_error: true,
            };
            return JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap());
        }

        // Return not implemented error for now
        let result = ToolResult {
            content: vec![ToolResultContent::Text {
                text: format!("Tool execution not yet implemented for '{}'", tool_name),
            }],
            is_error: true,
        };
        JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle unknown method.
    fn handle_unknown_method(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse::error(
            request.id,
            JsonRpcError::new(
                JsonRpcError::METHOD_NOT_FOUND,
                format!("Method not found: {}", request.method),
            ),
        )
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the MCP server using stdio for communication.
///
/// This function reads JSON-RPC requests from stdin (one per line)
/// and writes responses to stdout.
pub async fn run_stdio_server(server: McpServer) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    info!(
        "Starting MCP stdio server: {} v{}",
        server.config.name, server.config.version
    );

    loop {
        line.clear();

        match reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF reached
                info!("EOF received, shutting down server");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                debug!("Received: {}", trimmed);

                // Parse the request
                let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Failed to parse request: {}", e);
                        let error_response = JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: None,
                            result: None,
                            error: Some(JsonRpcError::new(
                                JsonRpcError::PARSE_ERROR,
                                format!("Parse error: {}", e),
                            )),
                        };
                        let response_json = serde_json::to_string(&error_response)?;
                        stdout.write_all(response_json.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                        continue;
                    }
                };

                // Handle the request
                let response = server.handle_request(&request);

                // Send the response
                let response_json = serde_json::to_string(&response)?;
                debug!("Sending: {}", response_json);
                stdout.write_all(response_json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            Err(e) => {
                error!("Error reading from stdin: {}", e);
                return Err(e.into());
            }
        }
    }

    Ok(())
}

/// Request received via JSON-RPC that might be either a request or notification.
/// Used for parsing incoming messages that might not have an id.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcMessage {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Request ID (optional for notifications).
    pub id: Option<u64>,
    /// Method name.
    pub method: String,
    /// Method parameters.
    pub params: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_default() {
        let config = McpServerConfig::default();

        assert_eq!(config.name, "nevoflux-agent");
        assert!(!config.version.is_empty());
    }

    #[test]
    fn test_mcp_server_new() {
        let server = McpServer::new();

        assert!(server.tools.is_empty());
        assert_eq!(server.config.name, "nevoflux-agent");
    }

    #[test]
    fn test_mcp_server_with_config() {
        let config = McpServerConfig {
            name: "test-server".to_string(),
            version: "1.2.3".to_string(),
        };
        let server = McpServer::with_config(config);

        assert_eq!(server.config.name, "test-server");
        assert_eq!(server.config.version, "1.2.3");
    }

    #[test]
    fn test_mcp_server_register_tool() {
        let mut server = McpServer::new();

        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        };

        server.register_tool(tool.clone());

        assert_eq!(server.tools().len(), 1);
        assert_eq!(server.tools()[0].name, "test_tool");
    }

    #[test]
    fn test_mcp_server_server_info() {
        let server = McpServer::new();
        let info = server.server_info();

        assert_eq!(info.name, "nevoflux-agent");
        assert!(info.protocol_version.is_some());
        assert_eq!(info.protocol_version.unwrap(), PROTOCOL_VERSION);
    }

    #[test]
    fn test_mcp_server_capabilities() {
        let server = McpServer::new();
        let caps = server.capabilities();

        assert!(caps.tools.is_some());
        assert!(caps.resources.is_none());
        assert!(caps.prompts.is_none());
    }

    #[test]
    fn test_handle_initialize() {
        let server = McpServer::new();
        let request = JsonRpcRequest::with_id(1, "initialize", None);

        let response = server.handle_request(&request);

        assert!(response.is_success());
        let result = response.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert!(result["serverInfo"]["name"].is_string());
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn test_handle_tools_list() {
        let mut server = McpServer::new();
        server.register_tool(ToolDefinition {
            name: "my_tool".to_string(),
            description: "My tool description".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let request = JsonRpcRequest::with_id(2, "tools/list", None);
        let response = server.handle_request(&request);

        assert!(response.is_success());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "my_tool");
    }

    #[test]
    fn test_handle_tools_call_not_implemented() {
        let mut server = McpServer::new();
        server.register_tool(ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let request = JsonRpcRequest::with_id(
            3,
            "tools/call",
            Some(serde_json::json!({
                "name": "test_tool",
                "arguments": {}
            })),
        );
        let response = server.handle_request(&request);

        // Should return a success response with error result (tool execution not implemented)
        assert!(response.is_success());
        let result = response.result.unwrap();
        assert!(result["isError"].as_bool().unwrap());
    }

    #[test]
    fn test_handle_tools_call_missing_name() {
        let server = McpServer::new();

        let request = JsonRpcRequest::with_id(4, "tools/call", Some(serde_json::json!({})));
        let response = server.handle_request(&request);

        assert!(response.is_error());
        assert_eq!(response.error.unwrap().code, JsonRpcError::INVALID_PARAMS);
    }

    #[test]
    fn test_handle_tools_call_tool_not_found() {
        let server = McpServer::new();

        let request = JsonRpcRequest::with_id(
            5,
            "tools/call",
            Some(serde_json::json!({
                "name": "nonexistent_tool"
            })),
        );
        let response = server.handle_request(&request);

        // Returns success with error in result
        assert!(response.is_success());
        let result = response.result.unwrap();
        assert!(result["isError"].as_bool().unwrap());
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_handle_unknown_method() {
        let server = McpServer::new();
        let request = JsonRpcRequest::with_id(99, "unknown/method", None);

        let response = server.handle_request(&request);

        assert!(response.is_error());
        let error = response.error.unwrap();
        assert_eq!(error.code, JsonRpcError::METHOD_NOT_FOUND);
        assert!(error.message.contains("unknown/method"));
    }

    #[test]
    fn test_mcp_server_default_trait() {
        let server = McpServer::default();
        assert!(server.tools.is_empty());
    }
}
