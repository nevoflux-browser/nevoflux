/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Browser tool execution via browser.nevoflux.* API
//!
//! Uses dynamic JS calls to avoid panics when API is not available.

use shared_protocol::{BrowserToolAction, BrowserToolRequestPayload};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::messaging::bridge::to_js_value;

/// Result from browser tool execution
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserToolResult {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserToolError>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserToolError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub recoverable: bool,
}

/// Get the browser.nevoflux object, returns None if not available
fn get_nevoflux_api() -> Option<js_sys::Object> {
    let global = js_sys::global();

    // Get browser object
    let browser = js_sys::Reflect::get(&global, &JsValue::from_str("browser")).ok()?;
    if browser.is_undefined() || browser.is_null() {
        tracing::warn!("browser object not available");
        return None;
    }

    // Get nevoflux object
    let nevoflux = js_sys::Reflect::get(&browser, &JsValue::from_str("nevoflux")).ok()?;
    if nevoflux.is_undefined() || nevoflux.is_null() {
        tracing::warn!("browser.nevoflux API not available");
        return None;
    }

    Some(nevoflux.dyn_into::<js_sys::Object>().ok()?)
}

/// Call a method on browser.nevoflux dynamically
async fn call_nevoflux_method(method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let nevoflux = get_nevoflux_api()
        .ok_or_else(|| "browser.nevoflux API not available".to_string())?;

    // Get the method
    let func = js_sys::Reflect::get(&nevoflux, &JsValue::from_str(method))
        .map_err(|_| format!("Method {} not found", method))?;

    if func.is_undefined() || func.is_null() {
        return Err(format!("Method {} not found on browser.nevoflux", method));
    }

    let func: js_sys::Function = func.dyn_into()
        .map_err(|_| format!("{} is not a function", method))?;

    // Create arguments array
    let args_array = js_sys::Array::new();
    for arg in args {
        args_array.push(arg);
    }

    // Call the function
    let promise = func.apply(&nevoflux, &args_array)
        .map_err(|e| format!("Failed to call {}: {:?}", method, e))?;

    // Check if result is a Promise
    if !promise.is_instance_of::<js_sys::Promise>() {
        // Not a promise, return directly
        return Ok(promise);
    }

    // Await the promise
    let promise: js_sys::Promise = promise.dyn_into()
        .map_err(|_| "Result is not a Promise".to_string())?;

    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Promise rejected: {:?}", e))
}

/// Execute a browser tool request using browser.nevoflux.* API
pub async fn execute_browser_tool(request: &BrowserToolRequestPayload) -> BrowserToolResult {
    // Check if API is available first
    if get_nevoflux_api().is_none() {
        return BrowserToolResult {
            success: false,
            result: None,
            error: Some(BrowserToolError {
                code: -1,
                message: "browser.nevoflux API not available in this context".to_string(),
                recoverable: false,
            }),
        };
    }

    let tab_id = request.tab_id.map(|id| JsValue::from_f64(id as f64))
        .unwrap_or(JsValue::NULL);

    match &request.action {
        BrowserToolAction::Navigate => execute_navigate(tab_id, &request.params).await,
        // GoBack/GoForward are handled by background.js via browser.nevoflux.back/forward
        BrowserToolAction::GoBack | BrowserToolAction::GoForward => {
            make_error_result("GoBack/GoForward should be forwarded to background.js")
        }
        BrowserToolAction::Click => execute_click(tab_id, &request.params).await,
        BrowserToolAction::Type => execute_type(tab_id, &request.params).await,
        BrowserToolAction::Fill => execute_fill(tab_id, &request.params).await,
        BrowserToolAction::GetContent => execute_get_content(tab_id, &request.params).await,
        BrowserToolAction::Screenshot => execute_screenshot(tab_id, &request.params).await,
        BrowserToolAction::WaitFor => execute_wait_for(tab_id, &request.params).await,
        BrowserToolAction::Scroll => execute_scroll(tab_id, &request.params).await,
        BrowserToolAction::GetElement => execute_get_element(tab_id, &request.params).await,
        BrowserToolAction::QueryAll => execute_query_all(tab_id, &request.params).await,
        BrowserToolAction::EvalJs => execute_eval_js(tab_id, &request.params).await,
        // GetMarkdown action
        BrowserToolAction::GetMarkdown => execute_get_markdown(tab_id, &request.params).await,
        // Snapshot-based actions are executed via content script (forwarded to background.js)
        // They don't use browser.nevoflux.* API directly
        BrowserToolAction::Snapshot |
        BrowserToolAction::ClickById |
        BrowserToolAction::FillById |
        BrowserToolAction::TypeById => {
            make_error_result("Snapshot actions should be forwarded to background.js")
        }
        // WebFetch is executed in background.js (URL fetch + markdown conversion)
        BrowserToolAction::WebFetch => {
            make_error_result("WebFetch should be forwarded to background.js")
        }
        // CacheTabMarkdown is executed in background.js (tab markdown + cache file)
        BrowserToolAction::CacheTabMarkdown => {
            make_error_result("CacheTabMarkdown should be forwarded to background.js")
        }
        // WebSearch is executed in background.js (web search via DuckDuckGo)
        BrowserToolAction::WebSearch => {
            make_error_result("WebSearch should be forwarded to background.js")
        }
        // AskUser is handled via sidebar UI interaction
        BrowserToolAction::AskUser => {
            make_error_result("AskUser should be forwarded to background.js")
        }
        // CacheFile is executed in background.js (saves file to disk, returns path)
        BrowserToolAction::CacheFile => {
            make_error_result("CacheFile should be forwarded to background.js")
        }
        // WaitForStable, KeyPress, ListTabs, QueryTabs, GetElements are handled in background.js
        BrowserToolAction::WaitForStable |
        BrowserToolAction::KeyPress |
        BrowserToolAction::ListTabs |
        BrowserToolAction::QueryTabs |
        BrowserToolAction::GetElements => {
            make_error_result("Action should be forwarded to background.js")
        }
        // ReadArtifact/EditArtifact operate on ContentStore via extension API
        BrowserToolAction::ReadArtifact |
        BrowserToolAction::EditArtifact => {
            make_error_result("Artifact actions should be forwarded to background.js")
        }
    }
}

async fn execute_navigate(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let url_js = JsValue::from_str(url);
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("open", &[tab_id, url_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Navigate failed: {}", e)),
    }
}

async fn execute_click(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let selector_js = JsValue::from_str(selector);
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("click", &[tab_id, selector_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Click failed: {}", e)),
    }
}

async fn execute_type(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let selector_js = JsValue::from_str(selector);
    let text_js = JsValue::from_str(text);
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("type", &[tab_id, selector_js, text_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Type failed: {}", e)),
    }
}

async fn execute_fill(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");
    let selector_js = JsValue::from_str(selector);
    let value_js = JsValue::from_str(value);

    match call_nevoflux_method("fill", &[tab_id, selector_js, value_js]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Fill failed: {}", e)),
    }
}

async fn execute_get_content(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str());

    if let Some(sel) = selector {
        // Get element text
        let selector_js = JsValue::from_str(sel);
        match call_nevoflux_method("getText", &[tab_id, selector_js]).await {
            Ok(result) => {
                let text = result.as_string().unwrap_or_default();
                BrowserToolResult {
                    success: true,
                    result: Some(serde_json::json!({
                        "selector": sel,
                        "text": text,
                    })),
                    error: None,
                }
            }
            Err(e) => make_error_result(&format!("GetText failed: {}", e)),
        }
    } else {
        // Get full page snapshot
        let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);
        match call_nevoflux_method("snapshot", &[tab_id, options]).await {
            Ok(result) => parse_api_result(result),
            Err(e) => make_error_result(&format!("Snapshot failed: {}", e)),
        }
    }
}

async fn execute_screenshot(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("screenshot", &[tab_id, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Screenshot failed: {}", e)),
    }
}

async fn execute_wait_for(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let selector_js = JsValue::from_str(selector);
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("waitForSelector", &[tab_id, selector_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("WaitFor failed: {}", e)),
    }
}

async fn execute_scroll(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let direction = params.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
    let amount = params.get("amount").and_then(|v| v.as_i64()).unwrap_or(300);

    let (delta_x, delta_y) = match direction {
        "up" => (0, -(amount as i32)),
        "down" => (0, amount as i32),
        "left" => (-(amount as i32), 0),
        "right" => (amount as i32, 0),
        _ => (0, amount as i32),
    };

    let wheel_options = serde_json::json!({
        "deltaX": delta_x,
        "deltaY": delta_y,
    });
    let options = to_js_value(&wheel_options).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("wheel", &[tab_id, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("Scroll failed: {}", e)),
    }
}

async fn execute_get_element(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let selector_js = JsValue::from_str(selector);

    // Check if element exists
    match call_nevoflux_method("exists", &[tab_id.clone(), selector_js.clone()]).await {
        Ok(exists_result) => {
            let exists = exists_result.as_bool().unwrap_or(false);
            if !exists {
                return BrowserToolResult {
                    success: false,
                    result: None,
                    error: Some(BrowserToolError {
                        code: -1,
                        message: format!("Element not found: {}", selector),
                        recoverable: true,
                    }),
                };
            }

            // Get visibility
            let visible = match call_nevoflux_method("isVisible", &[tab_id, selector_js]).await {
                Ok(v) => v.as_bool().unwrap_or(false),
                Err(_) => false,
            };

            BrowserToolResult {
                success: true,
                result: Some(serde_json::json!({
                    "selector": selector,
                    "exists": true,
                    "visible": visible,
                })),
                error: None,
            }
        }
        Err(e) => make_error_result(&format!("GetElement failed: {}", e)),
    }
}

async fn execute_query_all(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("");
    let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    let script = format!(
        r#"(function() {{
            const elements = document.querySelectorAll('{}');
            const results = [];
            for (let i = 0; i < Math.min(elements.length, {}); i++) {{
                const el = elements[i];
                const rect = el.getBoundingClientRect();
                results.push({{
                    tag: el.tagName.toLowerCase(),
                    id: el.id || null,
                    text: el.textContent?.substring(0, 100) || '',
                    visible: rect.width > 0 && rect.height > 0,
                }});
            }}
            return {{ count: results.length, elements: results }};
        }})()"#,
        selector.replace('\'', "\\'"),
        limit
    );

    let script_js = JsValue::from_str(&script);
    let options = JsValue::UNDEFINED;

    match call_nevoflux_method("eval", &[tab_id, script_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("QueryAll failed: {}", e)),
    }
}

async fn execute_eval_js(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let script = params.get("script").and_then(|v| v.as_str()).unwrap_or("");
    let script_js = JsValue::from_str(script);
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("eval", &[tab_id, script_js, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("EvalJs failed: {}", e)),
    }
}

async fn execute_get_markdown(tab_id: JsValue, params: &serde_json::Value) -> BrowserToolResult {
    let options = to_js_value(params).unwrap_or(JsValue::UNDEFINED);

    match call_nevoflux_method("getMarkdown", &[tab_id, options]).await {
        Ok(result) => parse_api_result(result),
        Err(e) => make_error_result(&format!("GetMarkdown failed: {}", e)),
    }
}

/// Parse API result that may have success/error fields
fn parse_api_result(result: JsValue) -> BrowserToolResult {
    if let Ok(json_str) = js_sys::JSON::stringify(&result) {
        if let Some(s) = json_str.as_string() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                // Check if it's an error response
                if let Some(error) = parsed.get("error") {
                    if let Some(error_obj) = error.as_object() {
                        return BrowserToolResult {
                            success: false,
                            result: None,
                            error: Some(BrowserToolError {
                                code: error_obj.get("code").and_then(|v| v.as_i64()).unwrap_or(-1) as i32,
                                message: error_obj.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string(),
                                recoverable: error_obj.get("recoverable").and_then(|v| v.as_bool()).unwrap_or(true),
                            }),
                        };
                    }
                }

                // Check explicit success field
                let success = parsed.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                if !success {
                    return BrowserToolResult {
                        success: false,
                        result: Some(parsed),
                        error: None,
                    };
                }

                return BrowserToolResult {
                    success: true,
                    result: Some(parsed),
                    error: None,
                };
            }
        }
    }

    // Fallback: treat as success with raw result
    BrowserToolResult {
        success: true,
        result: Some(serde_json::json!(result.as_string().unwrap_or_default())),
        error: None,
    }
}

fn make_error_result(message: &str) -> BrowserToolResult {
    BrowserToolResult {
        success: false,
        result: None,
        error: Some(BrowserToolError {
            code: -1,
            message: message.to_string(),
            recoverable: true,
        }),
    }
}
