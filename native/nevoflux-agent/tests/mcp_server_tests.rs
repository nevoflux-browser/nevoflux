//! Integration tests for MCP server.
//!
//! These tests verify the MCP server functionality including
//! initialization, tool registration, and JSON-RPC request handling.

use nevoflux_mcp::{
    JsonRpcError, JsonRpcRequest, McpServer, McpServerConfig, ToolDefinition, PROTOCOL_VERSION,
};

/// Create a test tool definition.
fn create_test_tool(name: &str, description: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
    }
}

/// Create a test tool with parameters.
fn create_tool_with_params(
    name: &str,
    description: &str,
    properties: serde_json::Value,
) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": []
        }),
    }
}

#[test]
fn test_mcp_server_initialize() {
    // Test the initialize request handling
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(1, "initialize", None);

    let response = server.handle_request(&request);

    assert!(response.is_success(), "Expected success response");
    assert!(!response.is_error(), "Expected no error");

    let result = response.result.expect("Expected result");
    assert_eq!(
        result["protocolVersion"], PROTOCOL_VERSION,
        "Protocol version mismatch"
    );
    assert!(
        result["serverInfo"]["name"].is_string(),
        "Expected serverInfo.name to be a string"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "Expected capabilities.tools to be an object"
    );
}

#[test]
fn test_mcp_server_tools_list_empty() {
    // Test tools/list with no registered tools
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(2, "tools/list", None);

    let response = server.handle_request(&request);

    assert!(response.is_success(), "Expected success response");

    let result = response.result.expect("Expected result");
    let tools = result["tools"].as_array().expect("Expected tools array");
    assert!(tools.is_empty(), "Expected empty tools list");
}

#[test]
fn test_mcp_server_tools_list_with_tools() {
    // Test tools/list with registered tools
    let mut server = McpServer::new();

    // Register multiple tools
    server.register_tool(create_test_tool(
        "read_file",
        "Read a file from the filesystem",
    ));
    server.register_tool(create_test_tool("write_file", "Write content to a file"));
    server.register_tool(create_test_tool("list_dir", "List directory contents"));

    let request = JsonRpcRequest::with_id(3, "tools/list", None);
    let response = server.handle_request(&request);

    assert!(response.is_success(), "Expected success response");

    let result = response.result.expect("Expected result");
    let tools = result["tools"].as_array().expect("Expected tools array");
    assert_eq!(tools.len(), 3, "Expected 3 tools");

    // Verify tool names
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"read_file"));
    assert!(tool_names.contains(&"write_file"));
    assert!(tool_names.contains(&"list_dir"));
}

#[test]
fn test_mcp_server_unknown_method() {
    // Test handling of unknown methods
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(99, "unknown/method", None);

    let response = server.handle_request(&request);

    assert!(response.is_error(), "Expected error response");
    assert!(!response.is_success(), "Expected no success");

    let error = response.error.expect("Expected error");
    assert_eq!(
        error.code,
        JsonRpcError::METHOD_NOT_FOUND,
        "Expected METHOD_NOT_FOUND error code"
    );
    assert!(
        error.message.contains("unknown/method"),
        "Expected error message to contain method name"
    );
}

#[test]
fn test_mcp_server_info() {
    // Test server info retrieval
    let server = McpServer::new();
    let info = server.server_info();

    assert_eq!(info.name, "nevoflux-agent", "Expected default server name");
    assert!(!info.version.is_empty(), "Expected non-empty version");
    assert!(
        info.protocol_version.is_some(),
        "Expected protocol version to be set"
    );
    assert_eq!(
        info.protocol_version.as_ref().unwrap(),
        PROTOCOL_VERSION,
        "Protocol version mismatch"
    );
}

#[test]
fn test_mcp_server_capabilities() {
    // Test server capabilities
    let server = McpServer::new();
    let caps = server.capabilities();

    assert!(caps.tools.is_some(), "Expected tools capability");
    assert!(caps.resources.is_none(), "Expected no resources capability");
    assert!(caps.prompts.is_none(), "Expected no prompts capability");

    // Verify tools capability details
    let tools_cap = caps.tools.unwrap();
    assert!(!tools_cap.list_changed, "Expected list_changed to be false");
}

#[test]
fn test_mcp_server_custom_config() {
    // Test creating server with custom configuration
    let config = McpServerConfig {
        name: "custom-server".to_string(),
        version: "2.0.0".to_string(),
    };
    let server = McpServer::with_config(config);

    let info = server.server_info();
    assert_eq!(info.name, "custom-server");
    assert_eq!(info.version, "2.0.0");
}

#[test]
fn test_mcp_server_default_config() {
    // Test the default configuration
    let config = McpServerConfig::default();

    assert_eq!(config.name, "nevoflux-agent");
    assert!(!config.version.is_empty());
}

#[test]
fn test_mcp_server_register_tool() {
    // Test tool registration
    let mut server = McpServer::new();

    assert!(server.tools().is_empty(), "Expected no tools initially");

    let tool = create_tool_with_params(
        "search",
        "Search for content",
        serde_json::json!({
            "query": {"type": "string"},
            "limit": {"type": "integer"}
        }),
    );
    server.register_tool(tool);

    assert_eq!(server.tools().len(), 1, "Expected 1 tool");
    assert_eq!(server.tools()[0].name, "search");
    assert_eq!(server.tools()[0].description, "Search for content");
}

#[test]
fn test_mcp_server_multiple_tool_registration() {
    // Test registering multiple tools
    let mut server = McpServer::new();

    for i in 1..=10 {
        let tool = create_test_tool(&format!("tool_{i}"), &format!("Tool number {i}"));
        server.register_tool(tool);
    }

    assert_eq!(server.tools().len(), 10, "Expected 10 tools");
}

#[test]
fn test_mcp_server_tools_call_missing_name() {
    // Test tools/call without name parameter
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(4, "tools/call", Some(serde_json::json!({})));

    let response = server.handle_request(&request);

    assert!(response.is_error(), "Expected error response");
    let error = response.error.expect("Expected error");
    assert_eq!(
        error.code,
        JsonRpcError::INVALID_PARAMS,
        "Expected INVALID_PARAMS error"
    );
}

#[test]
fn test_mcp_server_tools_call_not_found() {
    // Test tools/call with a tool that doesn't exist
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(
        5,
        "tools/call",
        Some(serde_json::json!({
            "name": "nonexistent_tool"
        })),
    );

    let response = server.handle_request(&request);

    // Returns success with error in result (MCP pattern)
    assert!(response.is_success(), "Expected success response");
    let result = response.result.expect("Expected result");
    assert!(
        result["isError"].as_bool().unwrap(),
        "Expected isError to be true"
    );
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "Expected error message to mention tool not found"
    );
}

#[test]
fn test_mcp_server_tools_call_with_registered_tool() {
    // Test tools/call with a registered tool (not implemented returns error)
    let mut server = McpServer::new();
    server.register_tool(create_test_tool("echo", "Echo back input"));

    let request = JsonRpcRequest::with_id(
        6,
        "tools/call",
        Some(serde_json::json!({
            "name": "echo",
            "arguments": {"message": "hello"}
        })),
    );

    let response = server.handle_request(&request);

    // Tool execution not implemented yet, returns success with error result
    assert!(response.is_success(), "Expected success response");
    let result = response.result.expect("Expected result");
    assert!(
        result["isError"].as_bool().unwrap(),
        "Expected isError to be true"
    );
}

#[test]
fn test_mcp_server_json_rpc_version() {
    // Test that responses include correct JSON-RPC version
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(7, "initialize", None);

    let response = server.handle_request(&request);

    assert_eq!(response.jsonrpc, "2.0", "Expected JSON-RPC version 2.0");
    assert_eq!(
        response.id,
        Some(7),
        "Expected response ID to match request ID"
    );
}

#[test]
fn test_mcp_server_request_id_preserved() {
    // Test that request IDs are preserved in responses
    let server = McpServer::new();

    for id in [1, 42, 999, 12345] {
        let request = JsonRpcRequest::with_id(id, "tools/list", None);
        let response = server.handle_request(&request);

        assert_eq!(
            response.id,
            Some(id),
            "Expected response ID {id} to match request ID"
        );
    }
}

#[test]
fn test_mcp_server_initialize_response_structure() {
    // Test the full structure of initialize response
    let server = McpServer::new();
    let request = JsonRpcRequest::with_id(1, "initialize", None);

    let response = server.handle_request(&request);
    let result = response.result.expect("Expected result");

    // Verify all required fields are present
    assert!(result.get("protocolVersion").is_some());
    assert!(result.get("capabilities").is_some());
    assert!(result.get("serverInfo").is_some());

    // Verify serverInfo structure
    let server_info = &result["serverInfo"];
    assert!(server_info.get("name").is_some());
    assert!(server_info.get("version").is_some());

    // Verify capabilities structure
    let capabilities = &result["capabilities"];
    assert!(capabilities.get("tools").is_some());
}

#[test]
fn test_mcp_server_default_trait() {
    // Test that Default trait works
    let server = McpServer::default();
    assert!(server.tools().is_empty());
}
