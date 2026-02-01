/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Rust bindings for browser.nevoflux Experiment API via NevofluxBridge

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

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
    pub attributes: std::collections::HashMap<String, String>,
    pub rect: ElementRect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementRect {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionData {
    pub text: String,
    pub html: String,
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

// ==================== JS Bridge Bindings ====================

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = isAvailable)]
    fn js_is_available() -> bool;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = getTabContent, catch)]
    async fn js_get_tab_content(tab_id: u32, options_json: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = getTabState, catch)]
    async fn js_get_tab_state(tab_id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = getAllTabs, catch)]
    async fn js_get_all_tabs() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = getActiveTab, catch)]
    async fn js_get_active_tab() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = pickElement, catch)]
    async fn js_pick_element(tab_id: u32, options_json: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = cancelPicker, catch)]
    async fn js_cancel_picker(tab_id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = getSelection, catch)]
    async fn js_get_selection(tab_id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = lockPage, catch)]
    async fn js_lock_page(tab_id: u32, options_json: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = NevofluxBridge, js_name = unlockPage, catch)]
    async fn js_unlock_page(tab_id: u32) -> Result<JsValue, JsValue>;
}

// ==================== API Result Handling ====================

#[derive(Debug, Deserialize)]
struct ApiResult<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

fn parse_result<T: for<'de> Deserialize<'de>>(js_value: JsValue) -> Result<T, String> {
    let json_str = js_value
        .as_string()
        .ok_or_else(|| "Response is not a string".to_string())?;

    let result: ApiResult<T> = serde_json::from_str(&json_str)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    if result.success {
        result.data.ok_or_else(|| "No data in response".to_string())
    } else {
        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}

// ==================== Public API ====================

/// Check if NevofluxBridge is available
pub fn is_available() -> bool {
    js_is_available()
}

/// Get tab content as markdown/html/text (auto-restores discarded tabs)
pub async fn get_tab_content(tab_id: u32, options: Option<GetContentOptions>) -> Result<TabContent, String> {
    let options_json = match options {
        Some(opts) => serde_json::to_string(&opts).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    };

    let result = js_get_tab_content(tab_id, &options_json)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Get tab state (discarded, loading, complete)
pub async fn get_tab_state(tab_id: u32) -> Result<TabState, String> {
    let result = js_get_tab_state(tab_id)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Get all tabs in the current window
pub async fn get_all_tabs() -> Result<Vec<TabInfo>, String> {
    let result = js_get_all_tabs()
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Get current active tab
pub async fn get_active_tab() -> Result<TabInfo, String> {
    let result = js_get_active_tab()
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Start element picker and wait for user selection
pub async fn pick_element(tab_id: u32, hint: Option<&str>) -> Result<PickerResult, String> {
    let options = if let Some(h) = hint {
        format!(r#"{{"hint":"{}"}}"#, h)
    } else {
        "{}".to_string()
    };

    let result = js_pick_element(tab_id, &options)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Cancel active element picker
pub async fn cancel_picker(tab_id: u32) -> Result<(), String> {
    js_cancel_picker(tab_id)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    Ok(())
}

/// Get current text selection from a tab
pub async fn get_selection(tab_id: u32) -> Result<Option<SelectionData>, String> {
    let result = js_get_selection(tab_id)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    parse_result(result)
}

/// Lock page to prevent user interaction
pub async fn lock_page(tab_id: u32, message: Option<&str>) -> Result<(), String> {
    let options = if let Some(m) = message {
        format!(r#"{{"showOverlay":true,"message":"{}"}}"#, m)
    } else {
        r#"{"showOverlay":true}"#.to_string()
    };

    js_lock_page(tab_id, &options)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    Ok(())
}

/// Unlock page after agent operations
pub async fn unlock_page(tab_id: u32) -> Result<(), String> {
    js_unlock_page(tab_id)
        .await
        .map_err(|e| format!("JS error: {:?}", e))?;

    Ok(())
}
