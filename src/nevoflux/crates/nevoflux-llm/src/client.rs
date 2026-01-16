/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! LLM Client Interface

use anyhow::Result;
use nevoflux_common::config::{LlmConfig, LlmProvider};

use crate::anthropic::AnthropicClient;
use crate::openai::OpenAiClient;

/// Unified LLM client
pub enum LlmClient {
    Anthropic(AnthropicClient),
    OpenAI(OpenAiClient),
}

impl LlmClient {
    /// Create a new LLM client from configuration
    pub fn new(config: &LlmConfig) -> Result<Self> {
        match config.provider {
            LlmProvider::Anthropic => Ok(Self::Anthropic(AnthropicClient::new(config)?)),
            LlmProvider::OpenAI => Ok(Self::OpenAI(OpenAiClient::new(config)?)),
            LlmProvider::Custom => {
                anyhow::bail!("Custom LLM provider not yet implemented")
            }
        }
    }

    /// Send a chat message and get response
    pub async fn chat(&self, message: &str) -> Result<String> {
        match self {
            Self::Anthropic(client) => client.chat(message).await,
            Self::OpenAI(client) => client.chat(message).await,
        }
    }

    /// Send a chat message with streaming response
    pub async fn chat_stream(
        &self,
        message: &str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Unpin>> {
        match self {
            Self::Anthropic(client) => client.chat_stream(message).await,
            Self::OpenAI(client) => client.chat_stream(message).await,
        }
    }
}
