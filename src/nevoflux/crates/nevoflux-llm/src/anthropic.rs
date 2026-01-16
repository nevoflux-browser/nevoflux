/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Anthropic API Client

use anyhow::{Context, Result};
use nevoflux_common::config::LlmConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Anthropic API client
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    model: String,
    api_base: String,
}

impl AnthropicClient {
    /// Create a new Anthropic client
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .as_ref()
            .context("Anthropic API key not configured")?
            .clone();

        let api_base = config
            .api_base
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.model.clone(),
            api_base,
        })
    }

    /// Send a chat message
    pub async fn chat(&self, message: &str) -> Result<String> {
        let request = ChatRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: vec![Message {
                role: "user".to_string(),
                content: message.to_string(),
            }],
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.api_base))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;

        let chat_response: ChatResponse =
            response.json().await.context("Failed to parse response")?;

        Ok(chat_response
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default())
    }

    /// Send a chat message with streaming response
    pub async fn chat_stream(
        &self,
        _message: &str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Unpin>> {
        // TODO: Implement streaming
        anyhow::bail!("Streaming not yet implemented")
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    text: String,
}
