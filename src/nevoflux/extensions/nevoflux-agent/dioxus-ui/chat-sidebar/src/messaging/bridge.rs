/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! JavaScript bindings for WebExtension messaging API

use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen]
extern "C" {
    // ========================================
    // browser.runtime API
    // ========================================

    /// Send message to background script
    #[wasm_bindgen(js_namespace = ["browser", "runtime"], js_name = sendMessage)]
    pub fn runtime_send_message(message: JsValue) -> js_sys::Promise;

    /// Add message listener
    #[wasm_bindgen(js_namespace = ["browser", "runtime", "onMessage"], js_name = addListener)]
    pub fn runtime_add_listener(callback: &Closure<dyn Fn(JsValue, JsValue, JsValue) -> JsValue>);

}

/// Convert Rust struct to JsValue via JSON
pub fn to_js_value<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(value)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))?;
    js_sys::JSON::parse(&json)
}

/// Convert JsValue to Rust struct via JSON
///
/// Note: This function handles Firefox's Xray wrappers safely by catching
/// stringify errors that can occur with cross-compartment objects.
pub fn from_js_value<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, String> {
    // Handle undefined/null early
    if value.is_undefined() {
        return Err("Value is undefined".to_string());
    }
    if value.is_null() {
        return Err("Value is null".to_string());
    }

    // Try to stringify - this can fail with Xray-wrapped objects in Firefox extensions
    let json_result = js_sys::JSON::stringify(&value);

    let json = match json_result {
        Ok(js_string) => {
            js_string.as_string()
                .ok_or_else(|| "JSON.stringify returned non-string".to_string())?
        }
        Err(_e) => {
            // JSON.stringify failed - likely an Xray wrapper issue
            // Try alternative approaches
            if let Some(s) = value.as_string() {
                // It's a plain string, use it directly
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                return Err("Failed to stringify JsValue (possible Xray wrapper)".to_string());
            }
        }
    };

    serde_json::from_str(&json).map_err(|e| {
        let preview: String = json.chars().take(100).collect();
        format!("Deserialize error: {} (json: {})", e, preview)
    })
}

/// Send message synchronously (fire-and-forget, ignores promise result)
/// Use this when you don't need to wait for the response
pub fn send_message_sync<T: serde::Serialize>(message: &T) -> Result<(), String> {
    let js_value = to_js_value(message)
        .map_err(|e| format!("Serialize error: {:?}", e))?;

    // Just call sendMessage and ignore the returned promise
    // The message will still be sent, we just don't wait for acknowledgment
    let _ = runtime_send_message(js_value);

    Ok(())
}
