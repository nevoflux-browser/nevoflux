/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! OpenAI API Client

use anyhow::{Context, Result};
use nevoflux_common::config::LlmConfig;
use reqwest::Client;

/// OpenAI API client
pub struct OpenAiClient {
    client: Client,
    api_key: String,
    model: String,
    api_base: String,
}

impl OpenAiClient {
    /// Create a new OpenAI client
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .as_ref()
            .context("OpenAI API key not configured")?
            .clone();

        let api_base = config
            .api_base
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.model.clone(),
            api_base,
        })
    }

    /// Send a chat message
    pub async fn chat(&self, _message: &str) -> Result<String> {
        // TODO: Implement OpenAI chat
        anyhow::bail!("OpenAI client not yet implemented")
    }

    /// Send a chat message with streaming response
    pub async fn chat_stream(
        &self,
        _message: &str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Unpin>> {
        // TODO: Implement streaming
        anyhow::bail!("OpenAI streaming not yet implemented")
    }
}
