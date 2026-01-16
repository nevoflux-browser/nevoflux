/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tool Definitions for Claude API
//!
//! This module defines tool schemas in the format expected by the Claude API.
//! Tools are used for Computer Use - browser automation via natural language.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// Tool definition for Claude API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Tool invocation from Claude API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Tool result to send back to Claude
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolResultBlock {
    pub fn success(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            block_type: "tool_result".to_string(),
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: None,
        }
    }

    pub fn error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            block_type: "tool_result".to_string(),
            tool_use_id: tool_use_id.into(),
            content: error.into(),
            is_error: Some(true),
        }
    }
}

/// Get all browser control tool definitions
pub fn get_browser_tools() -> Vec<ToolDefinition> {
    vec![
        click_tool(),
        type_tool(),
        scroll_tool(),
        read_text_tool(),
        read_page_tool(),
        screenshot_tool(),
        navigate_tool(),
        wait_tool(),
        evaluate_js_tool(),
        fill_form_tool(),
        get_elements_tool(),
        highlight_tool(),
    ]
}

/// Click tool - click on a page element
fn click_tool() -> ToolDefinition {
    ToolDefinition {
        name: "click".to_string(),
        description: "Click on a page element specified by CSS selector. Use this to interact with buttons, links, and other clickable elements.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to click (e.g., '#login-btn', 'button.submit')"
                }
            },
            "required": ["selector"]
        }),
    }
}

/// Type tool - type text into an input field
fn type_tool() -> ToolDefinition {
    ToolDefinition {
        name: "type".to_string(),
        description: "Type text into an input field or text area. The field is identified by CSS selector.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the input element"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type into the field"
                },
                "clear_first": {
                    "type": "boolean",
                    "description": "Whether to clear the field before typing (default: true)",
                    "default": true
                }
            },
            "required": ["selector", "text"]
        }),
    }
}

/// Scroll tool - scroll the page
fn scroll_tool() -> ToolDefinition {
    ToolDefinition {
        name: "scroll".to_string(),
        description: "Scroll the page up or down by a specified amount.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "direction": {
                    "type": "string",
                    "enum": ["up", "down"],
                    "description": "Direction to scroll"
                },
                "amount": {
                    "type": "integer",
                    "description": "Number of pixels to scroll (default: 500)",
                    "default": 500
                }
            },
            "required": ["direction"]
        }),
    }
}

/// Read text tool - read text content of an element
fn read_text_tool() -> ToolDefinition {
    ToolDefinition {
        name: "read_text".to_string(),
        description: "Read the text content of a specific element on the page.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to read"
                }
            },
            "required": ["selector"]
        }),
    }
}

/// Read page tool - read main page content
fn read_page_tool() -> ToolDefinition {
    ToolDefinition {
        name: "read_page".to_string(),
        description: "Read the main text content of the current page. Useful for understanding page context.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "include_html": {
                    "type": "boolean",
                    "description": "Whether to include HTML structure (default: false)",
                    "default": false
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum length of content to return (default: 5000)",
                    "default": 5000
                }
            }
        }),
    }
}

/// Screenshot tool - capture page screenshot
fn screenshot_tool() -> ToolDefinition {
    ToolDefinition {
        name: "screenshot".to_string(),
        description: "Capture a screenshot of the current browser viewport. Use when you need visual confirmation of the page state.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "full_page": {
                    "type": "boolean",
                    "description": "Whether to capture the full page (default: false, captures viewport only)",
                    "default": false
                },
                "quality": {
                    "type": "integer",
                    "description": "JPEG quality 0-100 (default: 80)",
                    "default": 80,
                    "minimum": 0,
                    "maximum": 100
                }
            }
        }),
    }
}

/// Navigate tool - navigate to URL
fn navigate_tool() -> ToolDefinition {
    ToolDefinition {
        name: "navigate".to_string(),
        description: "Navigate the browser to a new URL. Use this to go to a different page.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                }
            },
            "required": ["url"]
        }),
    }
}

/// Wait tool - wait for element or time
fn wait_tool() -> ToolDefinition {
    ToolDefinition {
        name: "wait".to_string(),
        description: "Wait for an element to appear or for a specified amount of time.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector to wait for (optional - if not provided, waits for timeout)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Maximum time to wait in milliseconds (default: 5000)",
                    "default": 5000
                }
            }
        }),
    }
}

/// Evaluate JS tool - execute JavaScript
fn evaluate_js_tool() -> ToolDefinition {
    ToolDefinition {
        name: "evaluate_js".to_string(),
        description: "Execute JavaScript code on the page. Use with caution - only for advanced operations that cannot be done with other tools.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "JavaScript code to execute"
                }
            },
            "required": ["code"]
        }),
    }
}

/// Fill form tool - fill multiple form fields
fn fill_form_tool() -> ToolDefinition {
    ToolDefinition {
        name: "fill_form".to_string(),
        description: "Fill multiple form fields at once. More efficient than calling 'type' multiple times.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "fields": {
                    "type": "array",
                    "description": "Array of field selectors and values",
                    "items": {
                        "type": "object",
                        "properties": {
                            "selector": {
                                "type": "string",
                                "description": "CSS selector for the field"
                            },
                            "value": {
                                "type": "string",
                                "description": "Value to fill"
                            }
                        },
                        "required": ["selector", "value"]
                    }
                }
            },
            "required": ["fields"]
        }),
    }
}

/// Get elements tool - find matching elements
fn get_elements_tool() -> ToolDefinition {
    ToolDefinition {
        name: "get_elements".to_string(),
        description: "Get a list of elements matching a CSS selector. Returns information about each element.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector to match elements"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of elements to return (default: 10)",
                    "default": 10
                }
            },
            "required": ["selector"]
        }),
    }
}

/// Highlight tool - highlight element for debugging
fn highlight_tool() -> ToolDefinition {
    ToolDefinition {
        name: "highlight".to_string(),
        description: "Highlight an element on the page for visual debugging. The highlight is temporary.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to highlight"
                },
                "style": {
                    "type": "string",
                    "enum": ["outline", "overlay", "pulse"],
                    "description": "Highlight style (default: outline)",
                    "default": "outline"
                }
            },
            "required": ["selector"]
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions() {
        let tools = get_browser_tools();
        assert_eq!(tools.len(), 12);

        // Verify click tool
        let click = tools.iter().find(|t| t.name == "click").unwrap();
        assert!(click.description.contains("Click"));
        let schema = &click.input_schema;
        assert!(schema["required"].as_array().unwrap().contains(&json!("selector")));
    }

    #[test]
    fn test_tool_result_block() {
        let success = ToolResultBlock::success("tool-123", "Operation completed");
        assert_eq!(success.block_type, "tool_result");
        assert!(success.is_error.is_none());

        let error = ToolResultBlock::error("tool-456", "Failed to click element");
        assert_eq!(error.is_error, Some(true));
    }
}
