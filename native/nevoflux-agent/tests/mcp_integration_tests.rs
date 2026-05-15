//! MCP protocol integration tests.
//!
//! These tests verify the MCP tool definitions and protocol handling
//! across the full stack.

use nevoflux_mcp::{create_tools, JsonRpcRequest, McpServer, McpServerConfig, PROTOCOL_VERSION};

// ============================================================================
// Tool Definition Tests
// ============================================================================

#[test]
fn test_mcp_tools_complete() {
    let tools = create_tools();

    // Should have 29 tools (16 browser + 1 agent + 12 computer)
    assert_eq!(tools.len(), 29, "Expected 29 tools, got {}", tools.len());

    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();

    // Browser tools (16)
    assert!(
        names.contains(&"browser_navigate"),
        "Missing browser_navigate"
    );
    assert!(names.contains(&"browser_click"), "Missing browser_click");
    assert!(
        names.contains(&"browser_screenshot"),
        "Missing browser_screenshot"
    );
    assert!(names.contains(&"browser_type"), "Missing browser_type");
    assert!(names.contains(&"browser_fill"), "Missing browser_fill");
    assert!(
        names.contains(&"browser_get_content"),
        "Missing browser_get_content"
    );
    assert!(
        names.contains(&"browser_eval_js"),
        "Missing browser_eval_js"
    );
    assert!(
        names.contains(&"browser_wait_for"),
        "Missing browser_wait_for"
    );
    assert!(names.contains(&"browser_scroll"), "Missing browser_scroll");
    assert!(
        names.contains(&"browser_get_element"),
        "Missing browser_get_element"
    );
    assert!(
        names.contains(&"browser_query_all"),
        "Missing browser_query_all"
    );
    assert!(
        names.contains(&"browser_click_by_id"),
        "Missing browser_click_by_id"
    );
    assert!(
        names.contains(&"browser_fill_by_id"),
        "Missing browser_fill_by_id"
    );
    assert!(
        names.contains(&"browser_type_by_id"),
        "Missing browser_type_by_id"
    );
    assert!(
        names.contains(&"browser_get_markdown"),
        "Missing browser_get_markdown"
    );

    // Agent tools (1)
    assert!(names.contains(&"agent_chat"), "Missing agent_chat");

    // Computer tools (12)
    assert!(
        names.contains(&"computer_screenshot"),
        "Missing computer_screenshot"
    );
    assert!(
        names.contains(&"computer_mouse_move"),
        "Missing computer_mouse_move"
    );
    assert!(
        names.contains(&"computer_type_text"),
        "Missing computer_type_text"
    );
    assert!(names.contains(&"computer_click"), "Missing computer_click");
    assert!(names.contains(&"computer_key"), "Missing computer_key");
    assert!(
        names.contains(&"computer_scroll"),
        "Missing computer_scroll"
    );
    assert!(names.contains(&"computer_drag"), "Missing computer_drag");
    assert!(
        names.contains(&"computer_cursor_position"),
        "Missing computer_cursor_position"
    );
    assert!(
        names.contains(&"computer_mouse_down"),
        "Missing computer_mouse_down"
    );
    assert!(
        names.contains(&"computer_mouse_up"),
        "Missing computer_mouse_up"
    );
    assert!(
        names.contains(&"computer_hold_key"),
        "Missing computer_hold_key"
    );
    assert!(names.contains(&"computer_wait"), "Missing computer_wait");
}

#[test]
fn test_mcp_tool_schemas() {
    let tools = create_tools();

    for tool in &tools {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(
            !tool.description.is_empty(),
            "Tool {} should have a description",
            tool.name
        );
        assert!(
            tool.input_schema.is_object(),
            "Tool {} input_schema should be an object",
            tool.name
        );
        assert_eq!(
            tool.input_schema["type"], "object",
            "Tool {} input_schema type should be 'object'",
            tool.name
        );
    }
}

#[test]
fn test_mcp_tool_names_are_unique() {
    let tools = create_tools();
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let original_len = names.len();
    names.sort();
    names.dedup();

    assert_eq!(names.len(), original_len, "All tool names should be unique");
}

// ============================================================================
// Browser Tool Schema Tests
// ============================================================================

#[test]
fn test_browser_navigate_schema() {
    let tools = create_tools();
    let tool = tools.iter().find(|t| t.name == "browser_navigate").unwrap();

    assert!(tool.description.contains("Navigate"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["url"].is_object());
    assert_eq!(schema["properties"]["url"]["type"], "string");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("url")));
}

#[test]
fn test_browser_click_schema() {
    let tools = create_tools();
    let tool = tools.iter().find(|t| t.name == "browser_click").unwrap();

    assert!(tool.description.contains("Click"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["selector"].is_object());
    assert_eq!(schema["properties"]["selector"]["type"], "string");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("selector")));
}

#[test]
fn test_browser_screenshot_schema() {
    let tools = create_tools();
    let tool = tools
        .iter()
        .find(|t| t.name == "browser_screenshot")
        .unwrap();

    assert!(tool.description.to_lowercase().contains("image"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["full_page"].is_object());
    assert_eq!(schema["properties"]["full_page"]["type"], "boolean");
}

#[test]
fn test_browser_type_schema() {
    let tools = create_tools();
    let tool = tools.iter().find(|t| t.name == "browser_type").unwrap();

    assert!(tool.description.contains("Type text"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["selector"].is_object());
    assert!(schema["properties"]["text"].is_object());
    assert!(schema["properties"]["clear"].is_object());

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("selector")));
    assert!(required.contains(&serde_json::json!("text")));
}

// ============================================================================
// Agent Tool Schema Tests
// ============================================================================

#[test]
fn test_agent_chat_schema() {
    let tools = create_tools();
    let tool = tools.iter().find(|t| t.name == "agent_chat").unwrap();

    assert!(
        tool.description.contains("agent") || tool.description.contains("AI"),
        "Description should mention agent or AI"
    );

    let schema = &tool.input_schema;
    assert!(schema["properties"]["message"].is_object());
    assert_eq!(schema["properties"]["message"]["type"], "string");

    // Context is optional
    assert!(schema["properties"]["context"].is_object());

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("message")));
}

// ============================================================================
// Computer Tool Schema Tests
// ============================================================================

#[test]
fn test_computer_screenshot_schema() {
    let tools = create_tools();
    let tool = tools
        .iter()
        .find(|t| t.name == "computer_screenshot")
        .unwrap();

    assert!(tool.description.to_lowercase().contains("screenshot"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["monitor"].is_object());
    assert_eq!(schema["properties"]["monitor"]["type"], "integer");
}

#[test]
fn test_computer_mouse_move_schema() {
    let tools = create_tools();
    let tool = tools
        .iter()
        .find(|t| t.name == "computer_mouse_move")
        .unwrap();

    assert!(tool.description.contains("mouse"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["x"].is_object());
    assert!(schema["properties"]["y"].is_object());
    assert_eq!(schema["properties"]["x"]["type"], "integer");
    assert_eq!(schema["properties"]["y"]["type"], "integer");

    // click parameter has been removed (pure movement only)
    assert!(schema["properties"]["click"].is_null());

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("x")));
    assert!(required.contains(&serde_json::json!("y")));
}

#[test]
fn test_computer_type_text_schema() {
    let tools = create_tools();
    let tool = tools
        .iter()
        .find(|t| t.name == "computer_type_text")
        .unwrap();

    assert!(tool.description.contains("Type text"));

    let schema = &tool.input_schema;
    assert!(schema["properties"]["text"].is_object());
    assert_eq!(schema["properties"]["text"]["type"], "string");

    // Optional delay_ms
    assert!(schema["properties"]["delay_ms"].is_object());
    assert_eq!(schema["properties"]["delay_ms"]["type"], "integer");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("text")));
}

// ============================================================================
// MCP Server Integration Tests
// ============================================================================

#[test]
fn test_mcp_server_with_all_tools() {
    let mut server = McpServer::new();
    let tools = create_tools();

    // Register all tools
    for tool in tools {
        server.register_tool(tool);
    }

    // Verify all tools are registered (29 total: 16 browser + 1 agent + 12 computer)
    assert_eq!(server.tools().len(), 29);

    // List tools via JSON-RPC
    let request = JsonRpcRequest::with_id(1, "tools/list", None);
    let response = server.handle_request(&request);

    assert!(response.is_success());
    let result = response.result.unwrap();
    let listed_tools = result["tools"].as_array().unwrap();
    assert_eq!(listed_tools.len(), 29);
}

#[test]
fn test_mcp_server_tool_categories() {
    let tools = create_tools();

    let browser_tools: Vec<_> = tools
        .iter()
        .filter(|t| t.name.starts_with("browser_"))
        .collect();
    let agent_tools: Vec<_> = tools
        .iter()
        .filter(|t| t.name.starts_with("agent_"))
        .collect();
    let computer_tools: Vec<_> = tools
        .iter()
        .filter(|t| t.name.starts_with("computer_"))
        .collect();

    assert_eq!(browser_tools.len(), 16, "Expected 16 browser tools");
    assert_eq!(agent_tools.len(), 1, "Expected 1 agent tool");
    assert_eq!(computer_tools.len(), 12, "Expected 12 computer tools");
}

#[test]
fn test_mcp_protocol_version() {
    // Verify protocol version is set correctly
    assert!(!PROTOCOL_VERSION.is_empty());

    let server = McpServer::new();
    let info = server.server_info();
    assert_eq!(info.protocol_version.as_ref().unwrap(), PROTOCOL_VERSION);
}

#[test]
fn test_mcp_server_initialize_with_tools() {
    let mut server = McpServer::new();
    let tools = create_tools();
    for tool in tools {
        server.register_tool(tool);
    }

    // Send initialize request
    let request = JsonRpcRequest::with_id(1, "initialize", None);
    let response = server.handle_request(&request);

    assert!(response.is_success());
    let result = response.result.unwrap();

    // Verify capabilities indicate tools are available
    let capabilities = &result["capabilities"];
    assert!(capabilities["tools"].is_object());
}

#[test]
fn test_mcp_tools_call_browser_navigate() {
    let mut server = McpServer::new();
    server.register_tool(
        create_tools()
            .into_iter()
            .find(|t| t.name == "browser_navigate")
            .unwrap(),
    );

    // Call the tool
    let request = JsonRpcRequest::with_id(
        2,
        "tools/call",
        Some(serde_json::json!({
            "name": "browser_navigate",
            "arguments": {
                "url": "https://example.com"
            }
        })),
    );
    let response = server.handle_request(&request);

    // Tool execution returns success (with isError since not implemented)
    assert!(response.is_success());
    let result = response.result.unwrap();
    // Tool exists but execution is not implemented
    assert!(result["isError"].as_bool().unwrap());
}

#[test]
fn test_mcp_tools_call_agent_chat() {
    let mut server = McpServer::new();
    server.register_tool(
        create_tools()
            .into_iter()
            .find(|t| t.name == "agent_chat")
            .unwrap(),
    );

    // Call the tool
    let request = JsonRpcRequest::with_id(
        3,
        "tools/call",
        Some(serde_json::json!({
            "name": "agent_chat",
            "arguments": {
                "message": "Hello, agent!"
            }
        })),
    );
    let response = server.handle_request(&request);

    assert!(response.is_success());
}

#[test]
fn test_mcp_server_custom_config() {
    let config = McpServerConfig {
        name: "test-mcp-server".to_string(),
        version: "1.0.0-test".to_string(),
    };
    let server = McpServer::with_config(config);

    let info = server.server_info();
    assert_eq!(info.name, "test-mcp-server");
    assert_eq!(info.version, "1.0.0-test");
}

// ============================================================================
// Tool Definition Validation Tests
// ============================================================================

#[test]
fn test_all_tools_have_properties() {
    let tools = create_tools();

    for tool in &tools {
        let properties = tool.input_schema["properties"].as_object();
        assert!(
            properties.is_some(),
            "Tool {} should have properties object",
            tool.name
        );
    }
}

#[test]
fn test_required_fields_are_valid_properties() {
    let tools = create_tools();

    for tool in &tools {
        if let Some(required) = tool.input_schema["required"].as_array() {
            let properties = tool.input_schema["properties"].as_object().unwrap();

            for req in required {
                let req_name = req.as_str().unwrap();
                assert!(
                    properties.contains_key(req_name),
                    "Tool {} has required field '{}' not in properties",
                    tool.name,
                    req_name
                );
            }
        }
    }
}

#[test]
fn test_tool_descriptions_are_meaningful() {
    let tools = create_tools();

    for tool in &tools {
        assert!(
            tool.description.len() >= 10,
            "Tool {} description is too short: '{}'",
            tool.name,
            tool.description
        );
        // Description should not be a placeholder
        assert!(
            !tool.description.contains("TODO"),
            "Tool {} has TODO in description",
            tool.name
        );
        assert!(
            !tool.description.contains("placeholder"),
            "Tool {} has placeholder in description",
            tool.name
        );
    }
}

// ============================================================================
// Integration with Full Tool Set
// ============================================================================

#[test]
fn test_full_mcp_workflow() {
    // Create server and register all tools
    let mut server = McpServer::new();
    for tool in create_tools() {
        server.register_tool(tool);
    }

    // Step 1: Initialize
    let init_request = JsonRpcRequest::with_id(1, "initialize", None);
    let init_response = server.handle_request(&init_request);
    assert!(init_response.is_success());

    // Step 2: List tools
    let list_request = JsonRpcRequest::with_id(2, "tools/list", None);
    let list_response = server.handle_request(&list_request);
    assert!(list_response.is_success());

    let result = list_response.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 29);

    // Step 3: Verify each tool is callable (though not implemented)
    let tool_names = [
        // Browser tools (16)
        "browser_navigate",
        "browser_click",
        "browser_screenshot",
        "browser_type",
        "browser_fill",
        "browser_get_content",
        "browser_eval_js",
        "browser_wait_for",
        "browser_scroll",
        "browser_get_element",
        "browser_query_all",
        "browser_snapshot",
        "browser_click_by_id",
        "browser_fill_by_id",
        "browser_type_by_id",
        "browser_get_markdown",
        // Agent tools (1)
        "agent_chat",
        // Computer tools (12)
        "computer_screenshot",
        "computer_mouse_move",
        "computer_type_text",
        "computer_click",
        "computer_key",
        "computer_scroll",
        "computer_drag",
        "computer_cursor_position",
        "computer_mouse_down",
        "computer_mouse_up",
        "computer_hold_key",
        "computer_wait",
    ];

    for (id, name) in tool_names.iter().enumerate() {
        let call_request = JsonRpcRequest::with_id(
            (id + 10) as u64,
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": {}
            })),
        );
        let call_response = server.handle_request(&call_request);
        assert!(
            call_response.is_success(),
            "Tool {} should be callable",
            name
        );
    }
}

#[test]
fn test_mcp_error_handling() {
    let server = McpServer::new();

    // Unknown method
    let request = JsonRpcRequest::with_id(1, "unknown/method", None);
    let response = server.handle_request(&request);
    assert!(response.is_error());

    // Tool not found
    let request = JsonRpcRequest::with_id(
        2,
        "tools/call",
        Some(serde_json::json!({"name": "nonexistent_tool"})),
    );
    let response = server.handle_request(&request);
    assert!(response.is_success()); // Returns success with error in result
    let result = response.result.unwrap();
    assert!(result["isError"].as_bool().unwrap());
}
