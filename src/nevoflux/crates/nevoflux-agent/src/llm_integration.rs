/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! LLM Integration for Computer Use
//!
//! This module provides a high-level interface for interacting with LLM APIs
//! (Claude and OpenAI) with tool support for browser automation (Computer Use).

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use shared_protocol::AutoPageContext;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::tools::{get_browser_tools, ToolDefinition, ToolResultBlock, ToolUse};

// ============================================================================
// LLM Provider Enum
// ============================================================================

/// Supported LLM providers
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LlmProvider {
    #[default]
    Claude,
    OpenAI,
}

impl LlmProvider {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" | "gpt" | "chatgpt" => LlmProvider::OpenAI,
            _ => LlmProvider::Claude,
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for LLM client
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub api_key: String,
    pub model: String,
    pub api_base: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Claude,
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_base: "https://api.anthropic.com".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

impl LlmConfig {
    /// Create OpenAI config with defaults
    pub fn openai_default() -> Self {
        Self {
            provider: LlmProvider::OpenAI,
            api_key: String::new(),
            model: "gpt-4o".to_string(),
            api_base: "https://api.openai.com".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

// ============================================================================
// LLM Client
// ============================================================================

/// LLM client with tool support
pub struct LlmClient {
    config: LlmConfig,
    http_client: Client,
    tools: Vec<ToolDefinition>,
}

impl LlmClient {
    /// Create a new LLM client
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            tools: get_browser_tools(),
        }
    }

    /// Create LLM client from environment variables
    ///
    /// Checks for:
    /// - LLM_PROVIDER: "claude" (default) or "openai"
    /// - ANTHROPIC_API_KEY / OPENAI_API_KEY
    /// - ANTHROPIC_MODEL / OPENAI_MODEL
    /// - ANTHROPIC_API_BASE / OPENAI_API_BASE (for custom endpoints)
    pub fn from_env() -> Result<Self> {
        let provider_str = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "claude".to_string());
        let provider = LlmProvider::from_str(&provider_str);

        let config = match provider {
            LlmProvider::Claude => {
                let api_key = std::env::var("ANTHROPIC_API_KEY")
                    .context("ANTHROPIC_API_KEY environment variable not set")?;

                let model = std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

                let api_base = std::env::var("ANTHROPIC_API_BASE")
                    .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

                LlmConfig {
                    provider: LlmProvider::Claude,
                    api_key,
                    model,
                    api_base,
                    ..Default::default()
                }
            }
            LlmProvider::OpenAI => {
                let api_key = std::env::var("OPENAI_API_KEY")
                    .context("OPENAI_API_KEY environment variable not set")?;

                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".to_string());

                let api_base = std::env::var("OPENAI_API_BASE")
                    .unwrap_or_else(|_| "https://api.openai.com".to_string());

                LlmConfig {
                    provider: LlmProvider::OpenAI,
                    api_key,
                    model,
                    api_base,
                    ..LlmConfig::openai_default()
                }
            }
        };

        info!("LLM client configured for {:?} with model {}", config.provider, config.model);
        Ok(Self::new(config))
    }

    /// Get current provider
    pub fn provider(&self) -> &LlmProvider {
        &self.config.provider
    }

    /// Send a message with tool support (non-streaming)
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
    ) -> Result<ChatResponse> {
        match self.config.provider {
            LlmProvider::Claude => self.claude_chat(messages, page_context).await,
            LlmProvider::OpenAI => self.openai_chat(messages, page_context).await,
        }
    }

    /// Send a streaming message with tool support
    pub async fn chat_stream_with_tools(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
        text_sender: mpsc::UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        match self.config.provider {
            LlmProvider::Claude => self.claude_chat_stream(messages, page_context, text_sender).await,
            LlmProvider::OpenAI => self.openai_chat_stream(messages, page_context, text_sender).await,
        }
    }

    // ========================================================================
    // Claude Implementation
    // ========================================================================

    async fn claude_chat(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
    ) -> Result<ChatResponse> {
        let system = self.build_system_prompt(page_context);

        let request = ClaudeRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: Some(system),
            messages: messages.iter().map(|m| m.to_claude_message()).collect(),
            tools: Some(self.tools.clone()),
            tool_choice: None,
        };

        let response = self.http_client
            .post(format!("{}/v1/messages", self.config.api_base))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Claude API error: {} - {}", status, error_text);
            anyhow::bail!("Claude API returned error: {} - {}", status, error_text);
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse Claude API response")?;

        Ok(claude_response.into())
    }

    async fn claude_chat_stream(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
        text_sender: mpsc::UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        let system = self.build_system_prompt(page_context);

        let request = ClaudeRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: Some(system),
            messages: messages.iter().map(|m| m.to_claude_message()).collect(),
            tools: Some(self.tools.clone()),
            tool_choice: None,
        };

        let mut request_value = serde_json::to_value(&request)?;
        request_value["stream"] = serde_json::Value::Bool(true);

        let response = self.http_client
            .post(format!("{}/v1/messages", self.config.api_base))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_value)
            .send()
            .await
            .context("Failed to send streaming request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Claude API streaming error: {} - {}", status, error_text);
            anyhow::bail!("Claude API returned error: {} - {}", status, error_text);
        }

        // Parse SSE stream
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut current_tool_use: Option<PartialToolUse> = None;
        let mut stop_reason: Option<String> = None;

        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error reading stream chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(event) = serde_json::from_str::<ClaudeStreamEvent>(data) {
                        match event.event_type.as_str() {
                            "content_block_start" => {
                                if let Some(block) = event.content_block {
                                    if block.block_type == "tool_use" {
                                        current_tool_use = Some(PartialToolUse {
                                            id: block.id.unwrap_or_default(),
                                            name: block.name.unwrap_or_default(),
                                            input_json: String::new(),
                                        });
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = event.delta {
                                    if delta.delta_type == "text_delta" {
                                        if let Some(text) = delta.text {
                                            current_text.push_str(&text);
                                            let _ = text_sender.send(text);
                                        }
                                    } else if delta.delta_type == "input_json_delta" {
                                        if let Some(partial_json) = delta.partial_json {
                                            if let Some(ref mut tool) = current_tool_use {
                                                tool.input_json.push_str(&partial_json);
                                            }
                                        }
                                    }
                                }
                            }
                            "content_block_stop" => {
                                if let Some(tool) = current_tool_use.take() {
                                    let input: serde_json::Value = serde_json::from_str(&tool.input_json)
                                        .unwrap_or_else(|_| serde_json::json!({}));

                                    content_blocks.push(ContentBlock::ToolUse {
                                        id: tool.id,
                                        name: tool.name,
                                        input,
                                    });
                                } else if !current_text.is_empty() {
                                    content_blocks.push(ContentBlock::Text {
                                        text: std::mem::take(&mut current_text),
                                    });
                                }
                            }
                            "message_delta" => {
                                if let Some(delta) = event.delta {
                                    stop_reason = delta.stop_reason;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(ChatResponse {
            id: String::new(),
            content: content_blocks,
            stop_reason,
            usage: None,
        })
    }

    // ========================================================================
    // OpenAI Implementation
    // ========================================================================

    async fn openai_chat(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
    ) -> Result<ChatResponse> {
        let system = self.build_system_prompt(page_context);

        // Convert tools to OpenAI format
        let openai_tools: Vec<OpenAITool> = self.tools.iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: Some(t.description.clone()),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Build OpenAI messages (system message is separate)
        let mut openai_messages: Vec<OpenAIMessage> = vec![
            OpenAIMessage {
                role: "system".to_string(),
                content: Some(system),
                tool_calls: None,
                tool_call_id: None,
            }
        ];

        for msg in &messages {
            openai_messages.push(msg.to_openai_message());
        }

        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: openai_messages,
            tools: Some(openai_tools),
            tool_choice: None,
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
        };

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.config.api_base))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI API error: {} - {}", status, error_text);
            anyhow::bail!("OpenAI API returned error: {} - {}", status, error_text);
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .context("Failed to parse OpenAI API response")?;

        Ok(openai_response.into())
    }

    async fn openai_chat_stream(
        &self,
        messages: Vec<Message>,
        page_context: Option<&AutoPageContext>,
        text_sender: mpsc::UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        let system = self.build_system_prompt(page_context);

        // Convert tools to OpenAI format
        let openai_tools: Vec<OpenAITool> = self.tools.iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: Some(t.description.clone()),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Build OpenAI messages
        let mut openai_messages: Vec<OpenAIMessage> = vec![
            OpenAIMessage {
                role: "system".to_string(),
                content: Some(system),
                tool_calls: None,
                tool_call_id: None,
            }
        ];

        for msg in &messages {
            openai_messages.push(msg.to_openai_message());
        }

        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: openai_messages,
            tools: Some(openai_tools),
            tool_choice: None,
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
        };

        let mut request_value = serde_json::to_value(&request)?;
        request_value["stream"] = serde_json::Value::Bool(true);

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.config.api_base))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&request_value)
            .send()
            .await
            .context("Failed to send streaming request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI API streaming error: {} - {}", status, error_text);
            anyhow::bail!("OpenAI API returned error: {} - {}", status, error_text);
        }

        // Parse SSE stream
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut current_tool_calls: Vec<PartialToolCall> = Vec::new();
        let mut finish_reason: Option<String> = None;

        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error reading stream chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(event) = serde_json::from_str::<OpenAIStreamEvent>(data) {
                        if let Some(choice) = event.choices.first() {
                            // Handle finish reason
                            if let Some(ref reason) = choice.finish_reason {
                                finish_reason = Some(reason.clone());
                            }

                            if let Some(ref delta) = choice.delta {
                                // Handle text content
                                if let Some(ref content) = delta.content {
                                    current_text.push_str(content);
                                    let _ = text_sender.send(content.clone());
                                }

                                // Handle tool calls
                                if let Some(ref tool_calls) = delta.tool_calls {
                                    for tc in tool_calls {
                                        let index = tc.index.unwrap_or(0) as usize;

                                        // Ensure we have enough slots
                                        while current_tool_calls.len() <= index {
                                            current_tool_calls.push(PartialToolCall::default());
                                        }

                                        let partial = &mut current_tool_calls[index];

                                        if let Some(ref id) = tc.id {
                                            partial.id = id.clone();
                                        }
                                        if let Some(ref function) = tc.function {
                                            if let Some(ref name) = function.name {
                                                partial.name = name.clone();
                                            }
                                            if let Some(ref args) = function.arguments {
                                                partial.arguments.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Finalize text content
        if !current_text.is_empty() {
            content_blocks.push(ContentBlock::Text { text: current_text });
        }

        // Finalize tool calls
        for tc in current_tool_calls {
            if !tc.id.is_empty() && !tc.name.is_empty() {
                let input: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or_else(|_| serde_json::json!({}));
                content_blocks.push(ContentBlock::ToolUse {
                    id: tc.id,
                    name: tc.name,
                    input,
                });
            }
        }

        // Convert OpenAI finish_reason to Claude-style stop_reason
        let stop_reason = match finish_reason.as_deref() {
            Some("tool_calls") => Some("tool_use".to_string()),
            Some("stop") => Some("end_turn".to_string()),
            Some(other) => Some(other.to_string()),
            None => None,
        };

        Ok(ChatResponse {
            id: String::new(),
            content: content_blocks,
            stop_reason,
            usage: None,
        })
    }

    // ========================================================================
    // Shared Methods
    // ========================================================================

    /// Continue conversation with tool results
    pub fn add_tool_results(messages: &mut Vec<Message>, results: Vec<ToolResultBlock>) {
        messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::ToolResults(results),
        });
    }

    /// Build system prompt with optional page context
    fn build_system_prompt(&self, page_context: Option<&AutoPageContext>) -> String {
        let mut prompt = String::from(
            "You are NevoFlux, an AI assistant integrated into a web browser. You can interact with web pages using browser control tools.\n\n\
            When the user asks you to perform actions on a web page, use the appropriate tools:\n\
            - Use 'click' to click buttons, links, or other elements\n\
            - Use 'type' to enter text into input fields\n\
            - Use 'scroll' to scroll the page up or down\n\
            - Use 'read_text' to read content from specific elements\n\
            - Use 'read_page' to understand the full page content\n\
            - Use 'screenshot' when you need visual confirmation\n\
            - Use 'navigate' to go to a different URL\n\
            - Use 'wait' to wait for elements to appear\n\n\
            Always describe what you're doing before executing tools. If an action fails, explain why and suggest alternatives."
        );

        if let Some(ctx) = page_context {
            prompt.push_str("\n\n## Current Page Context\n\n");
            prompt.push_str(&format!("**URL**: {}\n", ctx.url));
            prompt.push_str(&format!("**Title**: {}\n", ctx.title));
            prompt.push_str(&format!(
                "**Viewport**: {}x{} (scroll: {}, {})\n",
                ctx.viewport.width, ctx.viewport.height,
                ctx.viewport.scroll_x, ctx.viewport.scroll_y
            ));

            if !ctx.interactive_elements.is_empty() {
                prompt.push_str("\n### Interactive Elements on Page\n\n");
                for elem in &ctx.interactive_elements {
                    let text_preview = elem.text.as_ref()
                        .map(|t| format!(" \"{}\"", t.chars().take(50).collect::<String>()))
                        .unwrap_or_default();
                    prompt.push_str(&format!(
                        "- `{}` ({}){}",
                        elem.selector, elem.tag, text_preview
                    ));
                    if let Some(ref placeholder) = elem.placeholder {
                        prompt.push_str(&format!(" [placeholder: {}]", placeholder));
                    }
                    prompt.push('\n');
                }
            }

            if let Some(ref text) = ctx.text_content {
                prompt.push_str("\n### Page Text (excerpt)\n\n");
                prompt.push_str(&text.chars().take(1000).collect::<String>());
                if text.len() > 1000 {
                    prompt.push_str("...");
                }
                prompt.push('\n');
            }
        }

        prompt
    }

    /// Extract tool uses from response
    pub fn extract_tool_uses(response: &ChatResponse) -> Vec<ToolUse> {
        response.content.iter()
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some(ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Extract text content from response
    pub fn extract_text(response: &ChatResponse) -> String {
        response.content.iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Check if response needs tool execution
    pub fn needs_tool_execution(response: &ChatResponse) -> bool {
        response.stop_reason.as_deref() == Some("tool_use")
    }
}

// ============================================================================
// Unified Message Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
    ToolResults(Vec<ToolResultBlock>),
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Text(text.into()),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Text(text.into()),
        }
    }

    pub fn assistant_with_tool_use(content: Vec<ContentBlock>) -> Self {
        let blocks: Vec<serde_json::Value> = content.into_iter()
            .map(|block| match block {
                ContentBlock::Text { text } => serde_json::json!({
                    "type": "text",
                    "text": text
                }),
                ContentBlock::ToolUse { id, name, input } => serde_json::json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }),
            })
            .collect();

        Self {
            role: "assistant".to_string(),
            content: MessageContent::Blocks(blocks),
        }
    }

    /// Convert to Claude message format
    fn to_claude_message(&self) -> ClaudeMessage {
        match &self.content {
            MessageContent::Text(text) => ClaudeMessage {
                role: self.role.clone(),
                content: ClaudeContent::Text(text.clone()),
            },
            MessageContent::Blocks(blocks) => ClaudeMessage {
                role: self.role.clone(),
                content: ClaudeContent::Blocks(blocks.clone()),
            },
            MessageContent::ToolResults(results) => {
                let blocks: Vec<serde_json::Value> = results.iter()
                    .map(|r| serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": r.tool_use_id,
                        "content": r.content,
                        "is_error": r.is_error.unwrap_or(false)
                    }))
                    .collect();
                ClaudeMessage {
                    role: "user".to_string(),
                    content: ClaudeContent::Blocks(blocks),
                }
            }
        }
    }

    /// Convert to OpenAI message format
    fn to_openai_message(&self) -> OpenAIMessage {
        match &self.content {
            MessageContent::Text(text) => OpenAIMessage {
                role: self.role.clone(),
                content: Some(text.clone()),
                tool_calls: None,
                tool_call_id: None,
            },
            MessageContent::Blocks(blocks) => {
                // Check if it's an assistant message with tool_use
                let mut tool_calls = Vec::new();
                let mut text_content = String::new();

                for block in blocks {
                    if let Some(block_type) = block.get("type").and_then(|v| v.as_str()) {
                        match block_type {
                            "text" => {
                                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                    text_content.push_str(text);
                                }
                            }
                            "tool_use" => {
                                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                                tool_calls.push(OpenAIToolCall {
                                    id: Some(id),
                                    index: None,
                                    tool_type: Some("function".to_string()),
                                    function: Some(OpenAIFunctionCall {
                                        name: Some(name),
                                        arguments: Some(serde_json::to_string(&input).unwrap_or_default()),
                                    }),
                                });
                            }
                            _ => {}
                        }
                    }
                }

                OpenAIMessage {
                    role: self.role.clone(),
                    content: if text_content.is_empty() { None } else { Some(text_content) },
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                }
            }
            MessageContent::ToolResults(results) => {
                // OpenAI expects each tool result as a separate message with role "tool"
                // For simplicity, we'll join them (caller should handle multiple messages)
                if let Some(first) = results.first() {
                    OpenAIMessage {
                        role: "tool".to_string(),
                        content: Some(first.content.clone()),
                        tool_calls: None,
                        tool_call_id: Some(first.tool_use_id.clone()),
                    }
                } else {
                    OpenAIMessage {
                        role: "tool".to_string(),
                        content: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }
                }
            }
        }
    }
}

// ============================================================================
// Unified Response Types
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ============================================================================
// Claude-specific Types
// ============================================================================

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: ClaudeContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ClaudeContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    id: String,
    content: Vec<ClaudeContentBlock>,
    stop_reason: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl From<ClaudeResponse> for ChatResponse {
    fn from(resp: ClaudeResponse) -> Self {
        let content = resp.content.into_iter()
            .map(|block| match block {
                ClaudeContentBlock::Text { text } => ContentBlock::Text { text },
                ClaudeContentBlock::ToolUse { id, name, input } => ContentBlock::ToolUse { id, name, input },
            })
            .collect();

        ChatResponse {
            id: resp.id,
            content,
            stop_reason: resp.stop_reason,
            usage: resp.usage.map(|u| Usage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    content_block: Option<ClaudeStreamContentBlock>,
    #[serde(default)]
    delta: Option<ClaudeStreamDelta>,
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
}

struct PartialToolUse {
    id: String,
    name: String,
    input_json: String,
}

// ============================================================================
// OpenAI-specific Types
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<u32>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    tool_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<OpenAIFunctionCall>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

impl From<OpenAIResponse> for ChatResponse {
    fn from(resp: OpenAIResponse) -> Self {
        let mut content = Vec::new();

        if let Some(choice) = resp.choices.into_iter().next() {
            // Add text content
            if let Some(text) = choice.message.content {
                if !text.is_empty() {
                    content.push(ContentBlock::Text { text });
                }
            }

            // Add tool calls
            if let Some(tool_calls) = choice.message.tool_calls {
                for tc in tool_calls {
                    if let (Some(id), Some(func)) = (tc.id, tc.function) {
                        if let (Some(name), Some(args)) = (func.name, func.arguments) {
                            let input: serde_json::Value = serde_json::from_str(&args)
                                .unwrap_or_else(|_| serde_json::json!({}));
                            content.push(ContentBlock::ToolUse { id, name, input });
                        }
                    }
                }
            }

            // Convert finish_reason to stop_reason
            let stop_reason = match choice.finish_reason.as_deref() {
                Some("tool_calls") => Some("tool_use".to_string()),
                Some("stop") => Some("end_turn".to_string()),
                Some(other) => Some(other.to_string()),
                None => None,
            };

            ChatResponse {
                id: resp.id,
                content,
                stop_reason,
                usage: resp.usage.map(|u| Usage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                }),
            }
        } else {
            ChatResponse {
                id: resp.id,
                content: Vec::new(),
                stop_reason: None,
                usage: None,
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamEvent {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: Option<OpenAIStreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert_eq!(LlmProvider::from_str("openai"), LlmProvider::OpenAI);
        assert_eq!(LlmProvider::from_str("gpt"), LlmProvider::OpenAI);
        assert_eq!(LlmProvider::from_str("chatgpt"), LlmProvider::OpenAI);
        assert_eq!(LlmProvider::from_str("claude"), LlmProvider::Claude);
        assert_eq!(LlmProvider::from_str("anthropic"), LlmProvider::Claude);
        assert_eq!(LlmProvider::from_str("unknown"), LlmProvider::Claude);
    }

    #[test]
    fn test_message_creation() {
        let user_msg = Message::user("Hello");
        assert_eq!(user_msg.role, "user");

        let assistant_msg = Message::assistant("Hi there");
        assert_eq!(assistant_msg.role, "assistant");
    }

    #[test]
    fn test_system_prompt_generation() {
        let client = LlmClient::new(LlmConfig::default());
        let prompt = client.build_system_prompt(None);
        assert!(prompt.contains("NevoFlux"));
        assert!(prompt.contains("click"));
    }

    #[test]
    fn test_system_prompt_with_context() {
        let client = LlmClient::new(LlmConfig::default());
        let context = AutoPageContext {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            ..Default::default()
        };
        let prompt = client.build_system_prompt(Some(&context));
        assert!(prompt.contains("https://example.com"));
        assert!(prompt.contains("Example"));
    }

    #[test]
    fn test_openai_config() {
        let config = LlmConfig::openai_default();
        assert_eq!(config.provider, LlmProvider::OpenAI);
        assert_eq!(config.model, "gpt-4o");
        assert!(config.api_base.contains("openai.com"));
    }
}
