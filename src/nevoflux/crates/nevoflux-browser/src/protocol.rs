/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Browser Communication Protocol

use serde::{Deserialize, Serialize};

/// Request to the browser
#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserRequest {
    pub id: u64,
    pub action: String,
    pub params: serde_json::Value,
}

/// Response from the browser
#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserResponse {
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Browser event notification
#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}
