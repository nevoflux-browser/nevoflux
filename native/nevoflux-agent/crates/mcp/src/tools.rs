//! MCP tool definitions for NevoFlux Agent.
//!
//! This module provides all tool definitions that the NevoFlux MCP server
//! exposes to Claude Code and other MCP clients.
//!
//! # Tool Categories
//!
//! - **Browser tools**: Interact with web browsers (navigate, click, type, screenshot)
//! - **Agent tools**: Interact with the AI agent (chat)
//! - **Computer tools**: Interact with the local computer (screenshot, mouse, keyboard)

use crate::types::ToolDefinition;

/// Create all available MCP tools.
///
/// Returns a vector of `ToolDefinition` containing all browser, agent,
/// and computer tools that the NevoFlux MCP server supports.
///
/// # Example
///
/// ```rust
/// use nevoflux_mcp::tools::create_tools;
///
/// let tools = create_tools();
/// assert!(tools.iter().any(|t| t.name == "browser_navigate"));
/// ```
pub fn create_tools() -> Vec<ToolDefinition> {
    let mut tools = Vec::new();

    // Browser tools
    tools.extend(create_browser_tools());

    // Agent tools
    tools.extend(create_agent_tools());

    // Computer tools
    tools.extend(create_computer_tools());

    tools
}

/// Create browser automation tools.
fn create_browser_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "browser_navigate".to_string(),
            description: "Navigate the browser to a specified URL".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "browser_click".to_string(),
            description: "Click an element on the page identified by a CSS selector".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to click"
                    }
                },
                "required": ["selector"]
            }),
        },
        ToolDefinition {
            name: "browser_screenshot".to_string(),
            description: "Take a visual IMAGE of the page. For viewing only, NOT for interaction. To interact with elements (click/type/fill), use browser_get_elements instead to get element IDs.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "full_page": {
                        "type": "boolean",
                        "description": "Whether to capture the full page or just the viewport",
                        "default": false
                    }
                }
            }),
        },
        ToolDefinition {
            name: "browser_type".to_string(),
            description: "Type text into an element on the page identified by a CSS selector"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to type into"
                    },
                    "text": {
                        "type": "string",
                        "description": "The text to type"
                    },
                    "clear": {
                        "type": "boolean",
                        "description": "Whether to clear the element before typing",
                        "default": false
                    }
                },
                "required": ["selector", "text"]
            }),
        },
        ToolDefinition {
            name: "browser_fill".to_string(),
            description:
                "Fill a form field identified by a CSS selector (clears existing content first)"
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the form field to fill"
                    },
                    "value": {
                        "type": "string",
                        "description": "The value to fill into the field"
                    }
                },
                "required": ["selector", "value"]
            }),
        },
        ToolDefinition {
            name: "browser_get_content".to_string(),
            description: "Get text/HTML content for READING only. NOT for interaction - use browser_get_elements to get element IDs for clicking/typing.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector for a specific element. If not provided, returns entire page content."
                    },
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Whether to include hidden elements",
                        "default": false
                    }
                }
            }),
        },
        ToolDefinition {
            name: "browser_eval_js".to_string(),
            description: "Execute JavaScript code in the browser context and return the result. \
                WARNING: Many sites block eval() via Content Security Policy (CSP). \
                For reading page content, prefer browser_get_markdown. \
                Only use eval_js for DOM interactions that other browser tools cannot handle."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "JavaScript code to execute"
                    },
                    "args": {
                        "type": "array",
                        "description": "Optional arguments to pass to the script",
                        "items": {
                            "type": "object"
                        }
                    }
                },
                "required": ["script"]
            }),
        },
        ToolDefinition {
            name: "browser_wait_for".to_string(),
            description: "Wait for an element to appear on the page".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to wait for"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Maximum time to wait in milliseconds",
                        "default": 30000,
                        "minimum": 0,
                        "maximum": 60000
                    },
                    "state": {
                        "type": "string",
                        "description": "Desired element state",
                        "enum": ["visible", "hidden", "attached", "detached"],
                        "default": "visible"
                    }
                },
                "required": ["selector"]
            }),
        },
        ToolDefinition {
            name: "browser_scroll".to_string(),
            description: "Scroll page. direction: 'up' or 'down', amount: 'page', 'half', or pixel count. Returns updated page state.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "description": "Direction to scroll",
                        "enum": ["up", "down", "left", "right"]
                    },
                    "amount": {
                        "type": "integer",
                        "description": "Amount to scroll in pixels",
                        "default": 300,
                        "minimum": 0
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector for a scrollable element. If not provided, scrolls the page."
                    }
                },
                "required": ["direction"]
            }),
        },
        ToolDefinition {
            name: "browser_get_element".to_string(),
            description: "Get detailed information about an element on the page".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element"
                    },
                    "include_styles": {
                        "type": "boolean",
                        "description": "Whether to include computed styles",
                        "default": false
                    }
                },
                "required": ["selector"]
            }),
        },
        ToolDefinition {
            name: "browser_query_all".to_string(),
            description: "Query all elements matching a CSS selector and return their information"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of elements to return",
                        "default": 100,
                        "minimum": 1,
                        "maximum": 1000
                    }
                },
                "required": ["selector"]
            }),
        },
        ToolDefinition {
            name: "browser_get_elements".to_string(),
            description: "Get full accessibility tree (usually not needed — page state is auto-injected). Takes an accessibility tree snapshot with element IDs.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Whether to include hidden elements in the snapshot",
                        "default": false
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum depth of the accessibility tree",
                        "default": 10,
                        "minimum": 1,
                        "maximum": 50
                    }
                }
            }),
        },
        ToolDefinition {
            name: "browser_click_by_id".to_string(),
            description: "Click an element by its ref ID from the page state (e.g., 'e3'). Returns action result + updated page state."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The element ID from the accessibility snapshot"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to use",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["element_id"]
            }),
        },
        ToolDefinition {
            name: "browser_fill_by_id".to_string(),
            description: "Fill a form field by ref ID. Returns action result + updated page state."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The element ID from the accessibility snapshot"
                    },
                    "value": {
                        "type": "string",
                        "description": "The value to fill into the field"
                    }
                },
                "required": ["element_id", "value"]
            }),
        },
        ToolDefinition {
            name: "browser_type_by_id".to_string(),
            description: "Type text into element by ref ID. Returns action result + updated page state."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The element ID from the accessibility snapshot"
                    },
                    "text": {
                        "type": "string",
                        "description": "The text to type"
                    },
                    "clear": {
                        "type": "boolean",
                        "description": "Whether to clear the element before typing",
                        "default": false
                    }
                },
                "required": ["element_id", "text"]
            }),
        },
        ToolDefinition {
            name: "browser_get_markdown".to_string(),
            description: "Extract page content as Markdown for READING/UNDERSTANDING only. Use for: summarizing articles, extracting text, getting page information. DO NOT use for page interactions - use browser_get_elements instead when you need to click, type, or fill.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector for a specific element. If not provided, converts entire page."
                    },
                    "include_links": {
                        "type": "boolean",
                        "description": "Whether to include hyperlinks in the Markdown output",
                        "default": true
                    },
                    "include_images": {
                        "type": "boolean",
                        "description": "Whether to include image references in the Markdown output",
                        "default": false
                    }
                }
            }),
        },
    ]
}

/// Create agent interaction tools.
fn create_agent_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "agent_chat".to_string(),
        description: "Send a message to the AI agent and receive a response".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to send to the agent"
                },
                "context": {
                    "type": "object",
                    "description": "Optional context to include with the message",
                    "properties": {
                        "page_content": {
                            "type": "string",
                            "description": "Current page content or relevant text"
                        },
                        "page_url": {
                            "type": "string",
                            "description": "Current page URL"
                        }
                    }
                }
            },
            "required": ["message"]
        }),
    }]
}

/// Create computer interaction tools.
fn create_computer_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "computer_screenshot".to_string(),
            description: "Take a screenshot of the entire screen or a specific monitor".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "monitor": {
                        "type": "integer",
                        "description": "Monitor index (0-based). If not specified, captures primary monitor.",
                        "minimum": 0
                    }
                }
            }),
        },
        ToolDefinition {
            name: "computer_mouse_move".to_string(),
            description: "Move the mouse cursor to a specified position on screen without clicking"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    }
                },
                "required": ["x", "y"]
            }),
        },
        ToolDefinition {
            name: "computer_type_text".to_string(),
            description: "Type text using the keyboard at the current cursor position".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The text to type"
                    },
                    "delay_ms": {
                        "type": "integer",
                        "description": "Delay between keystrokes in milliseconds",
                        "default": 0,
                        "minimum": 0
                    }
                },
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "computer_click".to_string(),
            description: "Click at a specific screen position".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to click",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    },
                    "click_type": {
                        "type": "string",
                        "description": "Type of click to perform",
                        "enum": ["single", "double", "triple"],
                        "default": "single"
                    }
                },
                "required": ["x", "y"]
            }),
        },
        ToolDefinition {
            name: "computer_key".to_string(),
            description: "Press keyboard keys or key combinations".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key to press (e.g., 'Enter', 'Tab', 'Escape', 'a', 'F1')"
                    },
                    "modifiers": {
                        "type": "array",
                        "description": "Modifier keys to hold while pressing the key",
                        "items": {
                            "type": "string",
                            "enum": ["ctrl", "alt", "shift", "meta", "super"]
                        },
                        "default": []
                    },
                    "repeat": {
                        "type": "integer",
                        "description": "Number of times to repeat the key press",
                        "default": 1,
                        "minimum": 1,
                        "maximum": 100
                    }
                },
                "required": ["key"]
            }),
        },
        ToolDefinition {
            name: "computer_scroll".to_string(),
            description: "Scroll at a specific screen position".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels for scroll position"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels for scroll position"
                    },
                    "direction": {
                        "type": "string",
                        "description": "Direction to scroll",
                        "enum": ["up", "down", "left", "right"]
                    },
                    "amount": {
                        "type": "integer",
                        "description": "Number of scroll units",
                        "default": 3,
                        "minimum": 1,
                        "maximum": 100
                    }
                },
                "required": ["x", "y", "direction"]
            }),
        },
        ToolDefinition {
            name: "computer_drag".to_string(),
            description: "Drag from one screen position to another (press, move, release)"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "start_x": {
                        "type": "integer",
                        "description": "Starting X coordinate in pixels"
                    },
                    "start_y": {
                        "type": "integer",
                        "description": "Starting Y coordinate in pixels"
                    },
                    "end_x": {
                        "type": "integer",
                        "description": "Ending X coordinate in pixels"
                    },
                    "end_y": {
                        "type": "integer",
                        "description": "Ending Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to use for dragging",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["start_x", "start_y", "end_x", "end_y"]
            }),
        },
        ToolDefinition {
            name: "computer_cursor_position".to_string(),
            description: "Get the current mouse cursor position on screen".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "computer_mouse_down".to_string(),
            description:
                "Press and hold a mouse button at a position (use computer_mouse_up to release)"
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to press",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["x", "y"]
            }),
        },
        ToolDefinition {
            name: "computer_mouse_up".to_string(),
            description:
                "Release a mouse button at a position (use after computer_mouse_down)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to release",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["x", "y"]
            }),
        },
        ToolDefinition {
            name: "computer_hold_key".to_string(),
            description: "Hold a key down for a specified duration, then release".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key to hold (e.g., 'Shift', 'Control', 'Alt', 'a')"
                    },
                    "duration_ms": {
                        "type": "integer",
                        "description": "Duration to hold the key in milliseconds",
                        "minimum": 100,
                        "maximum": 10000,
                        "default": 500
                    },
                    "modifiers": {
                        "type": "array",
                        "description": "Modifier keys to hold simultaneously",
                        "items": {
                            "type": "string",
                            "enum": ["ctrl", "alt", "shift", "meta", "super"]
                        },
                        "default": []
                    }
                },
                "required": ["key", "duration_ms"]
            }),
        },
        ToolDefinition {
            name: "computer_wait".to_string(),
            description: "Wait for a specified duration before continuing. Use to wait for animations, loading, or transitions.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ms": {
                        "type": "integer",
                        "description": "Duration to wait in milliseconds",
                        "minimum": 100,
                        "maximum": 10000,
                        "default": 1000
                    }
                },
                "required": ["ms"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_tools_returns_all_tools() {
        let tools = create_tools();

        // Should have 29 tools total (16 browser + 1 agent + 12 computer)
        assert_eq!(tools.len(), 29);

        // Verify all expected tools are present
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Browser tools (16 total)
        assert!(tool_names.contains(&"browser_navigate"));
        assert!(tool_names.contains(&"browser_click"));
        assert!(tool_names.contains(&"browser_screenshot"));
        assert!(tool_names.contains(&"browser_type"));
        assert!(tool_names.contains(&"browser_fill"));
        assert!(tool_names.contains(&"browser_get_content"));
        assert!(tool_names.contains(&"browser_eval_js"));
        assert!(tool_names.contains(&"browser_wait_for"));
        assert!(tool_names.contains(&"browser_scroll"));
        assert!(tool_names.contains(&"browser_get_element"));
        assert!(tool_names.contains(&"browser_query_all"));
        assert!(tool_names.contains(&"browser_get_elements"));
        assert!(tool_names.contains(&"browser_click_by_id"));
        assert!(tool_names.contains(&"browser_fill_by_id"));
        assert!(tool_names.contains(&"browser_type_by_id"));
        assert!(tool_names.contains(&"browser_get_markdown"));

        // Agent tools (1 total)
        assert!(tool_names.contains(&"agent_chat"));

        // Computer tools (12 total)
        assert!(tool_names.contains(&"computer_screenshot"));
        assert!(tool_names.contains(&"computer_mouse_move"));
        assert!(tool_names.contains(&"computer_type_text"));
        assert!(tool_names.contains(&"computer_click"));
        assert!(tool_names.contains(&"computer_key"));
        assert!(tool_names.contains(&"computer_scroll"));
        assert!(tool_names.contains(&"computer_drag"));
        assert!(tool_names.contains(&"computer_cursor_position"));
        assert!(tool_names.contains(&"computer_mouse_down"));
        assert!(tool_names.contains(&"computer_mouse_up"));
        assert!(tool_names.contains(&"computer_hold_key"));
        assert!(tool_names.contains(&"computer_wait"));
    }

    #[test]
    fn test_browser_navigate_tool_schema() {
        let tools = create_browser_tools();
        let navigate = tools.iter().find(|t| t.name == "browser_navigate").unwrap();

        assert_eq!(
            navigate.description,
            "Navigate the browser to a specified URL"
        );

        let schema = &navigate.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["url"]));
    }

    #[test]
    fn test_browser_click_tool_schema() {
        let tools = create_browser_tools();
        let click = tools.iter().find(|t| t.name == "browser_click").unwrap();

        assert!(click.description.contains("Click an element on the page"));

        let schema = &click.input_schema;
        assert_eq!(schema["properties"]["selector"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["selector"]));
    }

    #[test]
    fn test_browser_screenshot_tool_schema() {
        let tools = create_browser_tools();
        let screenshot = tools
            .iter()
            .find(|t| t.name == "browser_screenshot")
            .unwrap();

        assert!(screenshot.description.contains("IMAGE"));

        let schema = &screenshot.input_schema;
        assert_eq!(schema["properties"]["full_page"]["type"], "boolean");
        // full_page is optional, so no required field or empty array
    }

    #[test]
    fn test_browser_type_tool_schema() {
        let tools = create_browser_tools();
        let type_tool = tools.iter().find(|t| t.name == "browser_type").unwrap();

        assert!(type_tool.description.contains("Type text"));

        let schema = &type_tool.input_schema;
        assert_eq!(schema["properties"]["selector"]["type"], "string");
        assert_eq!(schema["properties"]["text"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["selector", "text"]));
    }

    #[test]
    fn test_agent_chat_tool_schema() {
        let tools = create_agent_tools();
        let chat = tools.iter().find(|t| t.name == "agent_chat").unwrap();

        assert!(chat.description.contains("AI agent"));

        let schema = &chat.input_schema;
        assert_eq!(schema["properties"]["message"]["type"], "string");
        assert!(schema["properties"]["context"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["message"]));
    }

    #[test]
    fn test_computer_screenshot_tool_schema() {
        let tools = create_computer_tools();
        let screenshot = tools
            .iter()
            .find(|t| t.name == "computer_screenshot")
            .unwrap();

        assert!(screenshot.description.contains("screenshot"));

        let schema = &screenshot.input_schema;
        assert_eq!(schema["properties"]["monitor"]["type"], "integer");
    }

    #[test]
    fn test_computer_mouse_move_tool_schema() {
        let tools = create_computer_tools();
        let mouse_move = tools
            .iter()
            .find(|t| t.name == "computer_mouse_move")
            .unwrap();

        assert!(mouse_move.description.contains("mouse cursor"));

        let schema = &mouse_move.input_schema;
        assert_eq!(schema["properties"]["x"]["type"], "integer");
        assert_eq!(schema["properties"]["y"]["type"], "integer");
        assert_eq!(schema["required"], serde_json::json!(["x", "y"]));

        // click parameter has been removed (pure movement only)
        assert!(schema["properties"]["click"].is_null());
    }

    #[test]
    fn test_computer_type_text_tool_schema() {
        let tools = create_computer_tools();
        let type_text = tools
            .iter()
            .find(|t| t.name == "computer_type_text")
            .unwrap();

        assert!(type_text.description.contains("Type text"));

        let schema = &type_text.input_schema;
        assert_eq!(schema["properties"]["text"]["type"], "string");
        assert_eq!(schema["properties"]["delay_ms"]["type"], "integer");
        assert_eq!(schema["required"], serde_json::json!(["text"]));
    }

    #[test]
    fn test_all_tools_have_valid_schemas() {
        let tools = create_tools();

        for tool in &tools {
            // Every tool should have a non-empty name
            assert!(!tool.name.is_empty(), "Tool name should not be empty");

            // Every tool should have a non-empty description
            assert!(
                !tool.description.is_empty(),
                "Tool {} should have a description",
                tool.name
            );

            // Every tool schema should be an object type
            assert_eq!(
                tool.input_schema["type"], "object",
                "Tool {} schema should be object type",
                tool.name
            );

            // Every tool should have a properties field
            assert!(
                tool.input_schema["properties"].is_object(),
                "Tool {} should have properties",
                tool.name
            );
        }
    }

    #[test]
    fn test_tool_names_are_unique() {
        let tools = create_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();

        assert_eq!(names.len(), original_len, "All tool names should be unique");
    }

    // Tests for new browser tools

    #[test]
    fn test_browser_fill_tool_schema() {
        let tools = create_browser_tools();
        let fill = tools.iter().find(|t| t.name == "browser_fill").unwrap();

        assert!(fill.description.contains("Fill a form field"));

        let schema = &fill.input_schema;
        assert_eq!(schema["properties"]["selector"]["type"], "string");
        assert_eq!(schema["properties"]["value"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["selector", "value"]));
    }

    #[test]
    fn test_browser_get_content_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "browser_get_content")
            .unwrap();

        assert!(tool.description.contains("READING only"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["selector"]["type"], "string");
        assert_eq!(schema["properties"]["include_hidden"]["type"], "boolean");
    }

    #[test]
    fn test_browser_eval_js_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools.iter().find(|t| t.name == "browser_eval_js").unwrap();

        assert!(tool.description.contains("JavaScript"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["script"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["script"]));
    }

    #[test]
    fn test_browser_wait_for_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools.iter().find(|t| t.name == "browser_wait_for").unwrap();

        assert!(tool.description.contains("Wait for"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["selector"]["type"], "string");
        assert_eq!(schema["properties"]["timeout_ms"]["type"], "integer");
        assert!(schema["properties"]["state"]["enum"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("visible")));
    }

    #[test]
    fn test_browser_scroll_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools.iter().find(|t| t.name == "browser_scroll").unwrap();

        assert!(tool.description.contains("Scroll"));

        let schema = &tool.input_schema;
        let direction_enum = &schema["properties"]["direction"]["enum"];
        assert!(direction_enum
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("up")));
        assert!(direction_enum
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("down")));
    }

    #[test]
    fn test_browser_get_elements_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "browser_get_elements")
            .unwrap();

        assert!(tool.description.contains("accessibility tree"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["include_hidden"]["type"], "boolean");
        assert_eq!(schema["properties"]["max_depth"]["type"], "integer");
    }

    #[test]
    fn test_browser_click_by_id_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "browser_click_by_id")
            .unwrap();

        assert!(tool.description.contains("ref ID"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["element_id"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["element_id"]));
    }

    #[test]
    fn test_browser_get_markdown_tool_schema() {
        let tools = create_browser_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == "browser_get_markdown")
            .unwrap();

        assert!(tool.description.contains("Markdown"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["include_links"]["type"], "boolean");
        assert_eq!(schema["properties"]["include_images"]["type"], "boolean");
    }

    // Tests for new computer tools

    #[test]
    fn test_computer_click_tool_schema() {
        let tools = create_computer_tools();
        let tool = tools.iter().find(|t| t.name == "computer_click").unwrap();

        assert!(tool.description.contains("Click at"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["x"]["type"], "integer");
        assert_eq!(schema["properties"]["y"]["type"], "integer");
        assert_eq!(schema["required"], serde_json::json!(["x", "y"]));

        let button_enum = &schema["properties"]["button"]["enum"];
        assert!(button_enum
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("left")));
    }

    #[test]
    fn test_computer_key_tool_schema() {
        let tools = create_computer_tools();
        let tool = tools.iter().find(|t| t.name == "computer_key").unwrap();

        assert!(tool.description.contains("keyboard keys"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["key"]["type"], "string");
        assert_eq!(schema["required"], serde_json::json!(["key"]));

        let modifiers_items = &schema["properties"]["modifiers"]["items"]["enum"];
        assert!(modifiers_items
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("ctrl")));
    }

    #[test]
    fn test_computer_scroll_tool_schema() {
        let tools = create_computer_tools();
        let tool = tools.iter().find(|t| t.name == "computer_scroll").unwrap();

        assert!(tool.description.contains("Scroll at"));

        let schema = &tool.input_schema;
        assert_eq!(schema["properties"]["x"]["type"], "integer");
        assert_eq!(schema["properties"]["y"]["type"], "integer");
        assert_eq!(schema["properties"]["direction"]["type"], "string");
        assert_eq!(
            schema["required"],
            serde_json::json!(["x", "y", "direction"])
        );
    }

    #[test]
    fn test_browser_tools_count() {
        let tools = create_browser_tools();
        assert_eq!(tools.len(), 16);
    }

    #[test]
    fn test_computer_tools_count() {
        let tools = create_computer_tools();
        assert_eq!(tools.len(), 12);
    }

    #[test]
    fn test_agent_tools_count() {
        let tools = create_agent_tools();
        assert_eq!(tools.len(), 1);
    }
}
