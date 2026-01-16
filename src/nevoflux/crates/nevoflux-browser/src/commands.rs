/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Browser Control Commands

use anyhow::Result;
use nevoflux_common::types::BrowserAction;
use serde_json::Value;

/// Browser controller for executing actions
pub struct BrowserController {}

impl BrowserController {
    /// Create a new browser controller
    pub fn new() -> Self {
        Self {}
    }

    /// Execute a browser action
    pub async fn execute(&mut self, action: &Value) -> Result<Value> {
        let action: BrowserAction = serde_json::from_value(action.clone())?;

        match action {
            BrowserAction::Navigate { url } => self.navigate(&url).await,
            BrowserAction::Click { selector } => self.click(&selector).await,
            BrowserAction::FillForm { fields } => self.fill_form(&fields).await,
            BrowserAction::ExtractContent => self.extract_content().await,
            BrowserAction::Screenshot => self.screenshot().await,
        }
    }

    /// Navigate to a URL
    async fn navigate(&self, url: &str) -> Result<Value> {
        // Browser actions are actually executed by the extension
        // This just validates and forwards the command
        Ok(serde_json::json!({
            "action": "navigate",
            "url": url
        }))
    }

    /// Click an element
    async fn click(&self, selector: &str) -> Result<Value> {
        Ok(serde_json::json!({
            "action": "click",
            "selector": selector
        }))
    }

    /// Fill form fields
    async fn fill_form(&self, fields: &[nevoflux_common::types::FormField]) -> Result<Value> {
        Ok(serde_json::json!({
            "action": "fill_form",
            "fields": fields
        }))
    }

    /// Extract page content
    async fn extract_content(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "action": "extract_content"
        }))
    }

    /// Take a screenshot
    async fn screenshot(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "action": "screenshot"
        }))
    }
}

impl Default for BrowserController {
    fn default() -> Self {
        Self::new()
    }
}
