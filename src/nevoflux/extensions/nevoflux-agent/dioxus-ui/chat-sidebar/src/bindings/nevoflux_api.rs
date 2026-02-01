/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Rust bindings for browser.nevoflux API via direct js_sys calls
//!
//! Uses the same dynamic JavaScript call approach as browser_tools.rs,
//! eliminating the need for bridge.js.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ==================== Type Definitions ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabContent {
    #[serde(rename = "tabId")]
    pub tab_id: u32,
    pub url: String,
    pub title: String,
    pub content: String,
    pub format: String,
    #[serde(rename = "extractedAt")]
    pub extracted_at: u64,
    #[serde(rename = "wasDiscarded")]
    pub was_discarded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub discarded: bool,
    pub status: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: u32,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub discarded: bool,
    #[serde(rename = "favIconUrl")]
    pub fav_icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickerResult {
    pub selector: String,
    pub xpath: String,
    #[serde(rename = "tagName")]
    pub tag_name: String,
    pub id: Option<String>,
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub attributes: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub rect: ElementRect,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElementRect {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub left: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionData {
    pub text: String,
    pub html: String,
    #[serde(default)]
    pub rect: ElementRect,
    #[serde(rename = "anchorNode")]
    pub anchor_node: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContentOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoRestore")]
    pub auto_restore: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "keepRestored")]
    pub keep_restored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

// ==================== Helper Functions ====================

/// Get the browser.nevoflux object
fn get_nevoflux_api() -> Option<js_sys::Object> {
    let global = js_sys::global();

    let browser = js_sys::Reflect::get(&global, &JsValue::from_str("browser")).ok()?;
    if browser.is_undefined() || browser.is_null() {
        return None;
    }

    let nevoflux = js_sys::Reflect::get(&browser, &JsValue::from_str("nevoflux")).ok()?;
    if nevoflux.is_undefined() || nevoflux.is_null() {
        return None;
    }

    nevoflux.dyn_into::<js_sys::Object>().ok()
}

/// Get the browser.tabs object
fn get_tabs_api() -> Option<js_sys::Object> {
    let global = js_sys::global();

    let browser = js_sys::Reflect::get(&global, &JsValue::from_str("browser")).ok()?;
    if browser.is_undefined() || browser.is_null() {
        return None;
    }

    let tabs = js_sys::Reflect::get(&browser, &JsValue::from_str("tabs")).ok()?;
    if tabs.is_undefined() || tabs.is_null() {
        return None;
    }

    tabs.dyn_into::<js_sys::Object>().ok()
}

/// Call a method on browser.nevoflux dynamically
async fn call_nevoflux_method(method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let nevoflux = get_nevoflux_api()
        .ok_or_else(|| "browser.nevoflux API not available".to_string())?;

    let func = js_sys::Reflect::get(&nevoflux, &JsValue::from_str(method))
        .map_err(|_| format!("Method {} not found", method))?;

    if func.is_undefined() || func.is_null() {
        return Err(format!("Method {} not found on browser.nevoflux", method));
    }

    let func: js_sys::Function = func.dyn_into()
        .map_err(|_| format!("{} is not a function", method))?;

    let args_array = js_sys::Array::new();
    for arg in args {
        args_array.push(arg);
    }

    let promise = func.apply(&nevoflux, &args_array)
        .map_err(|e| format!("Failed to call {}: {:?}", method, e))?;

    if !promise.is_instance_of::<js_sys::Promise>() {
        return Ok(promise);
    }

    let promise: js_sys::Promise = promise.dyn_into()
        .map_err(|_| "Result is not a Promise".to_string())?;

    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Promise rejected: {:?}", e))
}

/// Call a method on browser.tabs dynamically
async fn call_tabs_method(method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let tabs = get_tabs_api()
        .ok_or_else(|| "browser.tabs API not available".to_string())?;

    let func = js_sys::Reflect::get(&tabs, &JsValue::from_str(method))
        .map_err(|_| format!("Method {} not found", method))?;

    if func.is_undefined() || func.is_null() {
        return Err(format!("Method {} not found on browser.tabs", method));
    }

    let func: js_sys::Function = func.dyn_into()
        .map_err(|_| format!("{} is not a function", method))?;

    let args_array = js_sys::Array::new();
    for arg in args {
        args_array.push(arg);
    }

    let promise = func.apply(&tabs, &args_array)
        .map_err(|e| format!("Failed to call {}: {:?}", method, e))?;

    if !promise.is_instance_of::<js_sys::Promise>() {
        return Ok(promise);
    }

    let promise: js_sys::Promise = promise.dyn_into()
        .map_err(|_| "Result is not a Promise".to_string())?;

    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Promise rejected: {:?}", e))
}

/// Convert JsValue to JSON and parse
fn parse_js_value<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, String> {
    let json_str = js_sys::JSON::stringify(&value)
        .map_err(|_| "Failed to stringify result".to_string())?
        .as_string()
        .ok_or_else(|| "Stringify returned non-string".to_string())?;

    serde_json::from_str(&json_str)
        .map_err(|e| format!("JSON parse error: {}", e))
}

/// Convert Rust value to JsValue
fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, String> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| format!("Serialize error: {}", e))?;

    js_sys::JSON::parse(&json_str)
        .map_err(|_| "Failed to parse JSON to JsValue".to_string())
}

// ==================== Public API ====================

/// Check if browser.nevoflux API is available
pub fn is_available() -> bool {
    get_nevoflux_api().is_some()
}

/// Get tab content as markdown/html/text (auto-restores discarded tabs)
pub async fn get_tab_content(tab_id: u32, options: Option<GetContentOptions>) -> Result<TabContent, String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);
    let options_js = match options {
        Some(opts) => to_js_value(&opts)?,
        None => JsValue::UNDEFINED,
    };

    let result = call_nevoflux_method("getTabContent", &[tab_id_js, options_js]).await?;

    // Check for error response
    if let Ok(obj) = result.clone().dyn_into::<js_sys::Object>() {
        if let Ok(success) = js_sys::Reflect::get(&obj, &JsValue::from_str("success")) {
            if success == JsValue::FALSE {
                if let Ok(error) = js_sys::Reflect::get(&obj, &JsValue::from_str("error")) {
                    if let Ok(error_obj) = error.dyn_into::<js_sys::Object>() {
                        if let Ok(msg) = js_sys::Reflect::get(&error_obj, &JsValue::from_str("message")) {
                            return Err(msg.as_string().unwrap_or_else(|| "Unknown error".to_string()));
                        }
                    }
                }
                return Err("API returned failure".to_string());
            }
        }
    }

    parse_js_value(result)
}

/// Get tab state (discarded, loading, complete)
pub async fn get_tab_state(tab_id: u32) -> Result<TabState, String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);
    let result = call_nevoflux_method("getTabState", &[tab_id_js]).await?;
    parse_js_value(result)
}

/// Get all tabs in the current window
pub async fn get_all_tabs() -> Result<Vec<TabInfo>, String> {
    let query = js_sys::Object::new();
    let result = call_tabs_method("query", &[query.into()]).await?;

    let tabs: Vec<serde_json::Value> = parse_js_value(result)?;

    let tab_infos: Vec<TabInfo> = tabs.into_iter().filter_map(|tab| {
        Some(TabInfo {
            id: tab.get("id")?.as_u64()? as u32,
            url: tab.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: tab.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            active: tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false),
            discarded: tab.get("discarded").and_then(|v| v.as_bool()).unwrap_or(false),
            fav_icon_url: tab.get("favIconUrl").and_then(|v| v.as_str()).map(|s| s.to_string()),
        })
    }).collect();

    Ok(tab_infos)
}

/// Get current active tab
pub async fn get_active_tab() -> Result<TabInfo, String> {
    let query = js_sys::Object::new();
    js_sys::Reflect::set(&query, &JsValue::from_str("active"), &JsValue::TRUE)
        .map_err(|_| "Failed to set active property")?;
    js_sys::Reflect::set(&query, &JsValue::from_str("currentWindow"), &JsValue::TRUE)
        .map_err(|_| "Failed to set currentWindow property")?;

    let result = call_tabs_method("query", &[query.into()]).await?;

    let tabs: Vec<serde_json::Value> = parse_js_value(result)?;

    let tab = tabs.first().ok_or_else(|| "No active tab found".to_string())?;

    Ok(TabInfo {
        id: tab.get("id").and_then(|v| v.as_u64()).ok_or("No tab id")? as u32,
        url: tab.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: tab.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        active: true,
        discarded: tab.get("discarded").and_then(|v| v.as_bool()).unwrap_or(false),
        fav_icon_url: tab.get("favIconUrl").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

/// Start element picker and wait for user selection
pub async fn pick_element(tab_id: u32, hint: Option<&str>) -> Result<PickerResult, String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);

    let options = js_sys::Object::new();
    if let Some(h) = hint {
        js_sys::Reflect::set(&options, &JsValue::from_str("hint"), &JsValue::from_str(h))
            .map_err(|_| "Failed to set hint")?;
    }

    let result = call_nevoflux_method("pickElement", &[tab_id_js, options.into()]).await?;

    // pickElement returns the data directly or wrapped in success/data
    if let Ok(obj) = result.clone().dyn_into::<js_sys::Object>() {
        // Check if it's a wrapper with data field
        if let Ok(data) = js_sys::Reflect::get(&obj, &JsValue::from_str("data")) {
            if !data.is_undefined() {
                return parse_js_value(data);
            }
        }
    }

    parse_js_value(result)
}

/// Cancel active element picker
pub async fn cancel_picker(tab_id: u32) -> Result<(), String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);
    call_nevoflux_method("cancelPicker", &[tab_id_js]).await?;
    Ok(())
}

/// Get current text selection from a tab
pub async fn get_selection(tab_id: u32) -> Result<Option<SelectionData>, String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);
    let result = call_nevoflux_method("getSelection", &[tab_id_js]).await?;

    if result.is_null() || result.is_undefined() {
        return Ok(None);
    }

    // Check for success wrapper
    if let Ok(obj) = result.clone().dyn_into::<js_sys::Object>() {
        if let Ok(data) = js_sys::Reflect::get(&obj, &JsValue::from_str("data")) {
            if data.is_null() || data.is_undefined() {
                return Ok(None);
            }
            if !data.is_undefined() {
                return Ok(Some(parse_js_value(data)?));
            }
        }
    }

    Ok(Some(parse_js_value(result)?))
}

/// Lock page to prevent user interaction
pub async fn lock_page(tab_id: u32, message: Option<&str>) -> Result<(), String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("showOverlay"), &JsValue::TRUE)
        .map_err(|_| "Failed to set showOverlay")?;
    if let Some(m) = message {
        js_sys::Reflect::set(&options, &JsValue::from_str("message"), &JsValue::from_str(m))
            .map_err(|_| "Failed to set message")?;
    }

    call_nevoflux_method("lockPage", &[tab_id_js, options.into()]).await?;
    Ok(())
}

/// Unlock page after agent operations
pub async fn unlock_page(tab_id: u32) -> Result<(), String> {
    let tab_id_js = JsValue::from_f64(tab_id as f64);
    call_nevoflux_method("unlockPage", &[tab_id_js]).await?;
    Ok(())
}
