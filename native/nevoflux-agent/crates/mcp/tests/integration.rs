//! Integration tests for the MCP crate.
//!
//! These tests require external MCP servers to be available.
//! They are ignored by default and can be run with:
//!
//! ```bash
//! cargo test -p nevoflux-mcp --test integration -- --ignored
//! ```

use nevoflux_mcp::{McpClient, McpRegistry, ServerConfig};

/// Test connecting to a simple echo MCP server (if available).
///
/// This test is ignored by default because it requires an MCP server.
#[tokio::test]
#[ignore]
async fn test_connect_to_mcp_server() {
    // This would connect to a real MCP server
    // Example: npx -y @modelcontextprotocol/server-filesystem ~
    let result = McpClient::connect_stdio(
        "npx",
        &["-y", "@modelcontextprotocol/server-filesystem", "~"],
    )
    .await;

    if let Ok(client) = result {
        // Should be able to list tools
        let tools = client.list_tools().await.expect("Failed to list tools");
        assert!(!tools.is_empty(), "Filesystem server should have tools");

        // Check server info
        let info = client.server_info().await;
        assert!(info.is_some(), "Should have server info");

        client.close().await.ok();
    }
}

/// Test registry with mock configuration.
#[tokio::test]
async fn test_registry_configuration() {
    let registry = McpRegistry::new();

    // Add multiple server configurations
    registry
        .add_config(ServerConfig::new("filesystem", "npx").with_args(vec![
            "-y",
            "@modelcontextprotocol/server-filesystem",
            "~",
        ]))
        .await;

    registry
        .add_config(
            ServerConfig::new("github", "npx")
                .with_args(vec!["-y", "@modelcontextprotocol/server-github"])
                .with_env("GITHUB_TOKEN", "test-token"),
        )
        .await;

    registry
        .add_config(ServerConfig::new("disabled", "echo").with_enabled(false))
        .await;

    // Check configured servers
    let servers = registry.configured_servers().await;
    assert_eq!(servers.len(), 3);

    // No connections yet
    let connected = registry.connected_servers().await;
    assert!(connected.is_empty());
}

/// Test JSON-RPC types.
#[test]
fn test_json_rpc_roundtrip() {
    use nevoflux_mcp::{JsonRpcRequest, JsonRpcResponse};

    // Create a request
    let request = JsonRpcRequest::new("tools/list", None);
    let json = serde_json::to_string(&request).unwrap();

    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"tools/list\""));

    // Parse a response
    let response_json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "tools": [
                {
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        },
                        "required": ["path"]
                    }
                }
            ]
        }
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(response_json).unwrap();
    assert!(response.is_success());
    assert!(response.result.is_some());
}

/// Test tool definition parsing.
#[test]
fn test_tool_definition_parsing() {
    use nevoflux_mcp::ToolDefinition;

    let json = r#"{
        "name": "create_directory",
        "description": "Create a new directory or ensure it exists",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the directory"
                }
            },
            "required": ["path"]
        }
    }"#;

    let tool: ToolDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(tool.name, "create_directory");
    assert!(tool.description.contains("directory"));
    assert!(tool.input_schema.get("properties").is_some());
}

/// Test tool result parsing.
#[test]
fn test_tool_result_parsing() {
    use nevoflux_mcp::ToolResult;

    // Text result
    let json = r#"{
        "content": [
            {
                "type": "text",
                "text": "File contents here..."
            }
        ],
        "isError": false
    }"#;

    let result: ToolResult = serde_json::from_str(json).unwrap();
    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);

    // Error result
    let error_json = r#"{
        "content": [
            {
                "type": "text",
                "text": "File not found"
            }
        ],
        "isError": true
    }"#;

    let error_result: ToolResult = serde_json::from_str(error_json).unwrap();
    assert!(error_result.is_error);
}

/// Test resource parsing.
#[test]
fn test_resource_parsing() {
    use nevoflux_mcp::Resource;

    let json = r#"{
        "uri": "file:///home/user/document.txt",
        "name": "document.txt",
        "description": "A text document",
        "mimeType": "text/plain"
    }"#;

    let resource: Resource = serde_json::from_str(json).unwrap();
    assert_eq!(resource.uri, "file:///home/user/document.txt");
    assert_eq!(resource.name, "document.txt");
    assert_eq!(resource.mime_type, Some("text/plain".to_string()));
}

/// Test error handling.
#[test]
fn test_error_types() {
    use nevoflux_mcp::McpError;

    let errors = vec![
        McpError::ConnectionFailed("test".to_string()),
        McpError::SpawnFailed("command not found".to_string()),
        McpError::TransportError("broken pipe".to_string()),
        McpError::RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        },
        McpError::Timeout(30000),
        McpError::NotInitialized,
        McpError::ToolNotFound("unknown_tool".to_string()),
    ];

    for error in errors {
        // All errors should be displayable
        let msg = error.to_string();
        assert!(!msg.is_empty());
    }
}
