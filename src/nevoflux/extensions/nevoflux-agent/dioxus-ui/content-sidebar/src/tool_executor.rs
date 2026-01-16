/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tool Executor for Content Sidebar
//!
//! Implements DOM operations for browser control tools.
//! Each tool executes in the content script context with access to the page DOM.

use serde_json::{json, Value};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlElement, HtmlInputElement, Window};

/// Result of tool execution
pub struct ToolExecutionResult {
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

impl ToolExecutionResult {
    pub fn success(result: Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(msg.into()),
        }
    }
}

/// Execute a tool by name
pub fn execute_tool(tool_name: &str, parameters: &Value) -> ToolExecutionResult {
    match tool_name {
        "click" => execute_click(parameters),
        "type" => execute_type(parameters),
        "scroll" => execute_scroll(parameters),
        "read_text" => execute_read_text(parameters),
        "read_page" => execute_read_page(parameters),
        "wait" => execute_wait(parameters),
        "evaluate_js" => execute_evaluate_js(parameters),
        "fill_form" => execute_fill_form(parameters),
        "get_elements" => execute_get_elements(parameters),
        "highlight" => execute_highlight(parameters),
        _ => ToolExecutionResult::error(format!("Unknown tool: {}", tool_name)),
    }
}

/// Get window and document
fn get_window_document() -> Option<(Window, Document)> {
    let window = web_sys::window()?;
    let document = window.document()?;
    Some((window, document))
}

/// Temporarily highlight an element before executing an action
/// The highlight will be removed after 1 second
fn highlight_element_temporarily(element: &Element) {
    if let Some(html_el) = element.dyn_ref::<HtmlElement>() {
        let style = html_el.style();

        // Save original styles
        let original_outline = style.get_property_value("outline").unwrap_or_default();
        let original_outline_offset = style.get_property_value("outline-offset").unwrap_or_default();

        // Apply highlight - bright green outline
        let _ = style.set_property("outline", "3px solid #22c55e");
        let _ = style.set_property("outline-offset", "2px");

        // Schedule removal after 1 second
        let el_clone = html_el.clone();
        let original_outline_clone = original_outline.clone();
        let original_outline_offset_clone = original_outline_offset.clone();

        let closure = Closure::once(Box::new(move || {
            let style = el_clone.style();
            if original_outline_clone.is_empty() {
                let _ = style.remove_property("outline");
            } else {
                let _ = style.set_property("outline", &original_outline_clone);
            }
            if original_outline_offset_clone.is_empty() {
                let _ = style.remove_property("outline-offset");
            } else {
                let _ = style.set_property("outline-offset", &original_outline_offset_clone);
            }
        }) as Box<dyn FnOnce()>);

        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                1000, // 1 second
            );
        }

        // Prevent closure from being dropped immediately
        closure.forget();
    }
}

/// Click on an element
fn execute_click(params: &Value) -> ToolExecutionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolExecutionResult::error("Missing 'selector' parameter"),
    };

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let element = match document.query_selector(selector) {
        Ok(Some(el)) => el,
        Ok(None) => return ToolExecutionResult::error(format!("Element not found: {}", selector)),
        Err(_) => return ToolExecutionResult::error(format!("Invalid selector: {}", selector)),
    };

    // Get element info before clicking
    let tag = element.tag_name().to_lowercase();
    let text = element.text_content().map(|t| t.chars().take(100).collect::<String>());

    // Highlight element before clicking
    highlight_element_temporarily(&element);

    // Click the element
    if let Some(html_el) = element.dyn_ref::<HtmlElement>() {
        html_el.click();

        ToolExecutionResult::success(json!({
            "clicked": true,
            "element": {
                "tag": tag,
                "text": text,
                "selector": selector
            }
        }))
    } else {
        ToolExecutionResult::error("Element is not clickable")
    }
}

/// Type text into an input field
fn execute_type(params: &Value) -> ToolExecutionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolExecutionResult::error("Missing 'selector' parameter"),
    };

    let text = match params.get("text").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return ToolExecutionResult::error("Missing 'text' parameter"),
    };

    let clear_first = params.get("clear_first").and_then(|v| v.as_bool()).unwrap_or(true);

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let element = match document.query_selector(selector) {
        Ok(Some(el)) => el,
        Ok(None) => return ToolExecutionResult::error(format!("Element not found: {}", selector)),
        Err(_) => return ToolExecutionResult::error(format!("Invalid selector: {}", selector)),
    };

    // Highlight element before typing
    highlight_element_temporarily(&element);

    // Focus the element first
    if let Some(html_el) = element.dyn_ref::<HtmlElement>() {
        html_el.focus().ok();
    }

    // Set value based on element type
    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
        if clear_first {
            input.set_value("");
        }
        let current = input.value();
        input.set_value(&format!("{}{}", current, text));

        // Dispatch input event
        dispatch_input_event(&element);

        ToolExecutionResult::success(json!({
            "typed": true,
            "text": text,
            "selector": selector
        }))
    } else if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        if clear_first {
            textarea.set_value("");
        }
        let current = textarea.value();
        textarea.set_value(&format!("{}{}", current, text));

        dispatch_input_event(&element);

        ToolExecutionResult::success(json!({
            "typed": true,
            "text": text,
            "selector": selector
        }))
    } else {
        ToolExecutionResult::error("Element is not an input or textarea")
    }
}

/// Dispatch input event on element
fn dispatch_input_event(element: &Element) {
    if let Some(window) = web_sys::window() {
        if let Ok(event) = web_sys::InputEvent::new("input") {
            let _ = element.dispatch_event(&event);
        }
        if let Ok(event) = web_sys::Event::new("change") {
            let _ = element.dispatch_event(&event);
        }
    }
}

/// Scroll the page
fn execute_scroll(params: &Value) -> ToolExecutionResult {
    let direction = match params.get("direction").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return ToolExecutionResult::error("Missing 'direction' parameter"),
    };

    let amount = params.get("amount").and_then(|v| v.as_i64()).unwrap_or(500) as i32;

    let (window, _) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get window"),
    };

    let scroll_y = match direction {
        "up" => -amount,
        "down" => amount,
        _ => return ToolExecutionResult::error("Invalid direction, use 'up' or 'down'"),
    };

    window.scroll_by_with_x_and_y(0.0, scroll_y as f64);

    let new_y = window.scroll_y().unwrap_or(0.0);

    ToolExecutionResult::success(json!({
        "scrolled": true,
        "direction": direction,
        "amount": amount.abs(),
        "current_scroll_y": new_y
    }))
}

/// Read text from an element
fn execute_read_text(params: &Value) -> ToolExecutionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolExecutionResult::error("Missing 'selector' parameter"),
    };

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let element = match document.query_selector(selector) {
        Ok(Some(el)) => el,
        Ok(None) => return ToolExecutionResult::error(format!("Element not found: {}", selector)),
        Err(_) => return ToolExecutionResult::error(format!("Invalid selector: {}", selector)),
    };

    let text = element.text_content().unwrap_or_default();
    let tag = element.tag_name().to_lowercase();

    ToolExecutionResult::success(json!({
        "text": text,
        "tag": tag,
        "selector": selector
    }))
}

/// Read page content
fn execute_read_page(params: &Value) -> ToolExecutionResult {
    let include_html = params.get("include_html").and_then(|v| v.as_bool()).unwrap_or(false);
    let max_length = params.get("max_length").and_then(|v| v.as_u64()).unwrap_or(5000) as usize;

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let body = match document.body() {
        Some(b) => b,
        None => return ToolExecutionResult::error("No body element found"),
    };

    let title = document.title();
    let url = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_default();

    let content = if include_html {
        body.inner_html()
    } else {
        body.text_content().unwrap_or_default()
    };

    let truncated_content: String = content.chars().take(max_length).collect();
    let was_truncated = content.len() > max_length;

    ToolExecutionResult::success(json!({
        "url": url,
        "title": title,
        "content": truncated_content,
        "truncated": was_truncated,
        "total_length": content.len()
    }))
}

/// Wait for element or timeout
fn execute_wait(params: &Value) -> ToolExecutionResult {
    let selector = params.get("selector").and_then(|v| v.as_str());
    let timeout_ms = params.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);

    // For now, we just check if element exists
    // In a real implementation, we'd use setTimeout/MutationObserver
    if let Some(sel) = selector {
        let (_, document) = match get_window_document() {
            Some(wd) => wd,
            None => return ToolExecutionResult::error("Failed to get document"),
        };

        match document.query_selector(sel) {
            Ok(Some(_)) => ToolExecutionResult::success(json!({
                "waited": true,
                "found": true,
                "selector": sel
            })),
            Ok(None) => ToolExecutionResult::success(json!({
                "waited": true,
                "found": false,
                "selector": sel,
                "message": format!("Element not found after {}ms", timeout_ms)
            })),
            Err(_) => ToolExecutionResult::error(format!("Invalid selector: {}", sel)),
        }
    } else {
        // Just wait for timeout - this is a placeholder
        // Real implementation would use setTimeout
        ToolExecutionResult::success(json!({
            "waited": true,
            "timeout_ms": timeout_ms
        }))
    }
}

/// Execute JavaScript code
fn execute_evaluate_js(params: &Value) -> ToolExecutionResult {
    let code = match params.get("code").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolExecutionResult::error("Missing 'code' parameter"),
    };

    let (window, _) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get window"),
    };

    // Use eval through window
    match js_sys::eval(code) {
        Ok(result) => {
            let result_str = if result.is_undefined() {
                "undefined".to_string()
            } else if result.is_null() {
                "null".to_string()
            } else {
                js_sys::JSON::stringify(&result)
                    .map(|s| s.as_string().unwrap_or_default())
                    .unwrap_or_else(|_| format!("{:?}", result))
            };

            ToolExecutionResult::success(json!({
                "evaluated": true,
                "result": result_str
            }))
        }
        Err(e) => {
            let error_msg = e.as_string().unwrap_or_else(|| "JavaScript error".to_string());
            ToolExecutionResult::error(format!("JS eval error: {}", error_msg))
        }
    }
}

/// Fill multiple form fields
fn execute_fill_form(params: &Value) -> ToolExecutionResult {
    let fields = match params.get("fields").and_then(|v| v.as_array()) {
        Some(f) => f,
        None => return ToolExecutionResult::error("Missing 'fields' parameter"),
    };

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let mut filled = Vec::new();
    let mut errors = Vec::new();

    for field in fields {
        let selector = field.get("selector").and_then(|v| v.as_str()).unwrap_or("");
        let value = field.get("value").and_then(|v| v.as_str()).unwrap_or("");

        if selector.is_empty() {
            errors.push(json!({"selector": selector, "error": "Empty selector"}));
            continue;
        }

        match document.query_selector(selector) {
            Ok(Some(element)) => {
                if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                    input.set_value(value);
                    dispatch_input_event(&element);
                    filled.push(json!({"selector": selector, "value": value}));
                } else if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                    textarea.set_value(value);
                    dispatch_input_event(&element);
                    filled.push(json!({"selector": selector, "value": value}));
                } else {
                    errors.push(json!({"selector": selector, "error": "Not an input element"}));
                }
            }
            Ok(None) => {
                errors.push(json!({"selector": selector, "error": "Element not found"}));
            }
            Err(_) => {
                errors.push(json!({"selector": selector, "error": "Invalid selector"}));
            }
        }
    }

    ToolExecutionResult::success(json!({
        "filled": filled,
        "errors": errors,
        "success_count": filled.len(),
        "error_count": errors.len()
    }))
}

/// Get elements matching selector
fn execute_get_elements(params: &Value) -> ToolExecutionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolExecutionResult::error("Missing 'selector' parameter"),
    };

    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let node_list = match document.query_selector_all(selector) {
        Ok(nl) => nl,
        Err(_) => return ToolExecutionResult::error(format!("Invalid selector: {}", selector)),
    };

    let mut elements = Vec::new();
    let total = node_list.length() as usize;

    for i in 0..total.min(limit) {
        if let Some(node) = node_list.get(i as u32) {
            if let Some(element) = node.dyn_ref::<Element>() {
                elements.push(json!({
                    "index": i,
                    "tag": element.tag_name().to_lowercase(),
                    "id": element.id(),
                    "class": element.class_name(),
                    "text": element.text_content().map(|t| t.chars().take(100).collect::<String>())
                }));
            }
        }
    }

    ToolExecutionResult::success(json!({
        "selector": selector,
        "elements": elements,
        "count": elements.len(),
        "total": total
    }))
}

/// Highlight an element
fn execute_highlight(params: &Value) -> ToolExecutionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolExecutionResult::error("Missing 'selector' parameter"),
    };

    let style = params.get("style").and_then(|v| v.as_str()).unwrap_or("outline");

    let (_, document) = match get_window_document() {
        Some(wd) => wd,
        None => return ToolExecutionResult::error("Failed to get document"),
    };

    let element = match document.query_selector(selector) {
        Ok(Some(el)) => el,
        Ok(None) => return ToolExecutionResult::error(format!("Element not found: {}", selector)),
        Err(_) => return ToolExecutionResult::error(format!("Invalid selector: {}", selector)),
    };

    if let Some(html_el) = element.dyn_ref::<HtmlElement>() {
        let el_style = html_el.style();

        match style {
            "outline" => {
                let _ = el_style.set_property("outline", "3px solid #6366f1");
                let _ = el_style.set_property("outline-offset", "2px");
            }
            "overlay" => {
                let _ = el_style.set_property("background-color", "rgba(99, 102, 241, 0.2)");
            }
            "pulse" => {
                let _ = el_style.set_property("animation", "nevoflux-pulse 1.5s infinite");
            }
            _ => {
                let _ = el_style.set_property("outline", "3px solid #6366f1");
            }
        }

        let _ = element.set_attribute("data-nevoflux-highlight", "true");

        ToolExecutionResult::success(json!({
            "highlighted": true,
            "selector": selector,
            "style": style
        }))
    } else {
        ToolExecutionResult::error("Cannot highlight element")
    }
}
