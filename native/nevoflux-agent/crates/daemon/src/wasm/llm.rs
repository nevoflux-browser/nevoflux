//! LLM host function implementation.
//!
//! This module provides the infrastructure for calling LLM providers
//! from Wasm guest modules via host functions.

use crate::error::{DaemonError, Result};
use futures::StreamExt;
use nevoflux_llm::providers::acp::context::compress_history;
use nevoflux_llm::providers::acp::tools::{
    extract_tool_calls_from_text, format_tool_definitions_prompt,
};
use nevoflux_llm::providers::acp::{AcpProvider, AcpUpdate, ContentBlock, TextContent};
use nevoflux_llm::providers::kimi_agent::KimiAgentClient;
use nevoflux_llm::providers::qwen::QwenClient;
use nevoflux_llm::ProviderType;
use nevoflux_protocol::json_repair::parse_tool_arguments_json;
use rig::client::CompletionClient;
use rig::client::Nothing;
use rig::completion::{CompletionModel, ToolDefinition};
use rig::message::{
    AssistantContent, DocumentSourceKind, Image, ImageDetail, ImageMediaType, Message, Text,
    ToolCall as RigToolCall, ToolFunction, ToolResult, ToolResultContent, UserContent,
};
use rig::providers::{
    anthropic, cohere, gemini, groq, mistral, ollama, openai, openrouter, perplexity, together, xai,
};
use rig::streaming::{StreamedAssistantContent, ToolCallDeltaContent};
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::{mpsc, Mutex as TokioMutex};

static ACP_PROVIDERS: OnceLock<Arc<TokioMutex<HashMap<String, AcpProvider>>>> = OnceLock::new();

pub fn acp_providers() -> &'static Arc<TokioMutex<HashMap<String, AcpProvider>>> {
    ACP_PROVIDERS.get_or_init(|| Arc::new(TokioMutex::new(HashMap::new())))
}

/// Tool definition for LLM function calling.
///
/// Defines a tool that the LLM can invoke during the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolDefinition {
    /// The name of the tool/function.
    pub name: String,
    /// A description of what the tool does.
    pub description: String,
    /// JSON Schema defining the tool's parameters.
    pub parameters: Value,
}

impl From<LlmToolDefinition> for ToolDefinition {
    fn from(tool: LlmToolDefinition) -> Self {
        ToolDefinition {
            name: tool.name,
            description: tool.description,
            parameters: tool.parameters,
        }
    }
}

/// A tool call made by the LLM.
///
/// Represents a request from the LLM to invoke a specific tool with given arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolCall {
    /// Unique identifier for this tool call.
    pub id: String,
    /// The call ID used to match tool results with tool calls.
    /// For OpenAI Responses API, this is different from `id` and MUST be used
    /// when sending tool results back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// The name of the tool to invoke.
    pub name: String,
    /// The arguments to pass to the tool (JSON object).
    pub arguments: Value,
    /// Optional cryptographic signature for the tool call.
    /// Used by Gemini 3 (thought_signature) to verify the tool call was generated
    /// by the model. MUST be preserved and sent back with tool results for multi-turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Fallback `max_tokens` cap for completion / streaming requests when
/// the caller didn't supply a value. Anthropic's REST API rejects
/// requests without `max_tokens`, so this is required to keep the
/// streaming path from failing on synthesized / test requests.
/// 32768 matches `default_max_tokens()` in config.rs — see the comment
/// there for the reasoning-model rationale (thinking budget + visible
/// output share the same cap).
const DEFAULT_MAX_TOKENS_FALLBACK: u64 = 32_768;

/// Request structure for LLM chat operations.
///
/// This is the JSON structure that Wasm guests send to the `llm_chat` host function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmChatRequest {
    /// The messages to send to the LLM.
    pub messages: Vec<LlmMessage>,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Optional temperature for response generation (0.0 - 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Optional maximum tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Optional list of tools the LLM can use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<LlmToolDefinition>>,
}

/// A single message in an LLM conversation.
///
/// Supports multiple roles:
/// - `user`: A message from the user
/// - `assistant`: A response from the LLM (may include tool_calls)
/// - `system`: A system instruction
/// - `tool`: A tool execution result (requires tool_call_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    /// The role of the message sender.
    pub role: String,
    /// The text content of the message (may be empty for tool-calling assistant messages).
    #[serde(default)]
    pub content: String,
    /// Tool calls made by the assistant (only present when role is "assistant").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    /// The ID of the tool call this message is responding to (only when role is "tool").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Attachments for multimodal messages (images, files).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<LlmAttachment>,
    /// Reasoning / thinking content produced by the model on this turn
    /// (assistant role). Required to be echoed back for DeepSeek thinking
    /// mode when the turn includes tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

impl LlmMessage {
    /// Create a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create a new user message with attachments.
    pub fn user_with_attachments(
        content: impl Into<String>,
        attachments: Vec<LlmAttachment>,
    ) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            attachments,
            reasoning: None,
        }
    }

    /// Create a new assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create an assistant message with tool calls.
    pub fn assistant_with_tool_calls(tool_calls: Vec<LlmToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create a tool result message.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            attachments: Vec::new(),
            reasoning: None,
        }
    }
}

/// An image generated by the LLM (e.g., from image generation models).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmGeneratedImage {
    /// MIME type (e.g., "image/png").
    pub media_type: String,
    /// Base64-encoded image data (without the `data:` URL prefix).
    pub data: String,
}

/// Response structure from LLM chat operations.
///
/// This is the JSON structure returned to Wasm guests from the `llm_chat` host function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmChatResponse {
    /// The generated text content from the LLM (may be empty if tool_calls is present).
    #[serde(default)]
    pub content: String,
    /// The reason the generation stopped (e.g., "stop", "tool_calls", "length").
    #[serde(default)]
    pub finish_reason: String,
    /// Tool calls requested by the LLM (present when finish_reason is "tool_calls").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    /// Optional usage statistics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<LlmUsage>,
    /// Generated images (from image generation models).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<LlmGeneratedImage>,
}

/// Token usage statistics for an LLM request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens used (prompt + completion).
    pub total_tokens: u32,
}

/// Attachment for multimodal messages (images, files, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAttachment {
    /// Attachment name/filename.
    pub name: String,
    /// MIME type (e.g., "image/png", "image/jpeg", "application/pdf").
    pub mime_type: String,
    /// Base64 encoded data.
    pub data: String,
}

/// Result of extracting a screenshot from a tool result JSON.
struct ExtractedScreenshot {
    /// The tool result JSON with the screenshot field removed.
    text_without_screenshot: String,
    /// The cleaned base64-encoded screenshot data.
    base64_data: String,
}

/// Extract screenshot base64 data from a tool result JSON string.
///
/// Looks for a `"screenshot"` key in the top-level JSON object.
/// If found and non-empty, returns the remaining JSON (without the screenshot field)
/// and the cleaned base64 data (data URL prefix stripped, whitespace removed).
fn extract_screenshot_from_tool_result(text: &str) -> Option<ExtractedScreenshot> {
    let mut value: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = value.as_object_mut()?;

    let screenshot = obj.remove("screenshot")?;
    let raw_base64 = screenshot.as_str()?;
    if raw_base64.is_empty() {
        return None;
    }

    // Strip data URL prefix if present (e.g., "data:image/png;base64,")
    let base64_data = if raw_base64.starts_with("data:") {
        raw_base64
            .find(',')
            .map(|i| &raw_base64[i + 1..])
            .unwrap_or(raw_base64)
    } else {
        raw_base64
    };

    // Remove whitespace/newlines from base64
    let clean_base64: String = base64_data.chars().filter(|c| !c.is_whitespace()).collect();
    if clean_base64.is_empty() {
        return None;
    }

    let text_without_screenshot =
        serde_json::to_string(&value).unwrap_or_else(|_| text.to_string());

    Some(ExtractedScreenshot {
        text_without_screenshot,
        base64_data: clean_base64,
    })
}

/// Execute an LLM chat request.
///
/// This function routes the request to the appropriate provider based on the
/// configured provider type.
///
/// # Arguments
///
/// * `provider` - The type of LLM provider to use.
/// * `api_key` - The API key for authentication.
/// * `model` - The model name to use.
/// * `request` - The chat request containing messages.
///
/// # Returns
///
/// Returns a `LlmChatResponse` on success, or a `DaemonError` on failure.
///
/// # Errors
///
/// Returns an error if:
/// - The provider is not implemented
/// - The LLM API call fails
/// - Response parsing fails
pub async fn execute_llm_chat(
    provider: ProviderType,
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    match provider {
        ProviderType::Anthropic => {
            execute_anthropic_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::OpenAi => {
            execute_openai_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::OpenRouter => {
            execute_openrouter_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::DeepSeek => {
            execute_deepseek_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::Qwen => execute_qwen_chat(api_key, model, request, provider, base_url).await,
        ProviderType::Gemini => {
            execute_gemini_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::Groq => execute_groq_chat(api_key, model, request, provider, base_url).await,
        ProviderType::Ollama => {
            execute_ollama_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::Mistral => {
            execute_mistral_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::XAi => execute_xai_chat(api_key, model, request, provider, base_url).await,
        ProviderType::Cohere => {
            execute_cohere_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::Perplexity => {
            execute_perplexity_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::Together => {
            execute_together_chat(api_key, model, request, provider, base_url).await
        }
        ProviderType::ClaudeCode | ProviderType::GeminiCli | ProviderType::OpenClaw => Err(
            DaemonError::InternalError("ACP providers only support streaming mode".to_string()),
        ),
        ProviderType::KimiAgent => {
            execute_kimi_agent_chat(api_key, model, request, provider, base_url).await
        }
    }
}

/// Execute a chat request using the Anthropic provider.
async fn execute_anthropic_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = anthropic::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: anthropic::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Anthropic client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the OpenAI provider.
///
/// When `base_url` is empty, uses rig's standard `openai::Client` which connects
/// to the official OpenAI API (Responses API at `/v1/responses`).
/// When `base_url` is set, uses `openai::CompletionsClient` for OpenAI-compatible
/// endpoints (Chat Completions API at `/chat/completions`).
async fn execute_openai_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    if let Some(url) = base_url {
        // Custom endpoint: use Chat Completions API (/chat/completions)
        let client: openai::CompletionsClient = openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(url)
            .build()
            .map_err(|e| {
                DaemonError::InternalError(format!("Failed to create OpenAI client: {}", e))
            })?;
        let completion_model = client.completion_model(model);
        execute_rig_completion(completion_model, request, provider).await
    } else {
        // Official OpenAI: use rig's standard client (Responses API)
        let client: openai::Client =
            openai::Client::builder()
                .api_key(api_key)
                .build()
                .map_err(|e| {
                    DaemonError::InternalError(format!("Failed to create OpenAI client: {}", e))
                })?;
        let completion_model = client.completion_model(model);
        execute_rig_completion(completion_model, request, provider).await
    }
}

/// Execute a chat request using the OpenRouter provider (native rig provider).
///
/// For image generation models, bypasses rig and uses raw HTTP to capture the
/// `images` field that rig's parser doesn't support.
async fn execute_openrouter_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    if is_image_generation_model(model) {
        return execute_raw_openai_compatible_chat(
            api_key,
            model,
            request,
            base_url.unwrap_or("https://openrouter.ai/api/v1"),
        )
        .await;
    }
    let mut builder = openrouter::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: openrouter::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create OpenRouter client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Check if a model is an image generation model that returns images in the response.
fn is_image_generation_model(model: &str) -> bool {
    model.contains("image-preview")
        || model.contains("image-generation")
        || model.contains("imagen")
}

// ===== Raw HTTP types for parsing OpenRouter/OpenAI responses with images =====

/// Raw response from OpenAI-compatible API (supports `images` extension).
#[derive(Debug, Deserialize)]
struct RawChatResponse {
    choices: Vec<RawChoice>,
}

#[derive(Debug, Deserialize)]
struct RawChoice {
    message: RawMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    content: Option<serde_json::Value>,
    #[serde(default)]
    images: Vec<RawImageEntry>,
    #[serde(default)]
    tool_calls: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawImageEntry {
    image_url: RawImageUrl,
}

#[derive(Debug, Deserialize)]
struct RawImageUrl {
    url: String,
}

/// Raw streaming chunk from OpenAI-compatible API (supports `images` extension).
#[derive(Debug, Deserialize)]
struct RawStreamChunk {
    choices: Vec<RawStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct RawStreamChoice {
    delta: RawStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawStreamDelta {
    content: Option<String>,
    #[serde(default)]
    images: Vec<RawImageEntry>,
}

/// Parse a data URL like "data:image/png;base64,iVBOR..." into (media_type, base64_data).
fn parse_data_url(url: &str) -> Option<(String, String)> {
    let url = url.strip_prefix("data:")?;
    let (header, data) = url.split_once(',')?;
    let media_type = header.strip_suffix(";base64")?.to_string();
    Some((media_type, data.to_string()))
}

/// Convert LlmChatRequest to OpenAI-compatible JSON request body.
fn build_openai_request_body(model: &str, request: &LlmChatRequest) -> serde_json::Value {
    let mut messages = Vec::new();

    // Add system message if present
    if let Some(ref system) = request.system {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
    }

    // Convert messages
    for msg in &request.messages {
        match msg.role.as_str() {
            "system" => {
                messages.push(serde_json::json!({
                    "role": "system",
                    "content": msg.content
                }));
            }
            "user" => {
                if msg.attachments.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content
                    }));
                } else {
                    // Multimodal message with attachments
                    let mut content_parts = vec![serde_json::json!({
                        "type": "text",
                        "text": msg.content
                    })];
                    for att in &msg.attachments {
                        if att.mime_type.starts_with("image/") {
                            let data_url = if att.data.starts_with("data:") {
                                att.data.clone()
                            } else {
                                format!("data:{};base64,{}", att.mime_type, att.data)
                            };
                            content_parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": data_url }
                            }));
                        }
                    }
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": content_parts
                    }));
                }
            }
            "assistant" => {
                let mut msg_json = serde_json::json!({
                    "role": "assistant",
                    "content": msg.content
                });
                if let Some(ref tool_calls) = msg.tool_calls {
                    let tc_json: Vec<serde_json::Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string()
                                }
                            })
                        })
                        .collect();
                    msg_json["tool_calls"] = serde_json::Value::Array(tc_json);
                }
                messages.push(msg_json);
            }
            "tool" => {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": msg.tool_call_id,
                    "content": msg.content
                }));
            }
            _ => {
                messages.push(serde_json::json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages
    });

    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(ref tools) = request.tools {
        if !tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools_json);
        }
    }

    body
}

/// Build an OpenAI-compatible request body for DeepSeek, threading
/// `reasoning_content` onto the same assistant message that carries
/// `tool_calls` when present.
///
/// We cannot reuse `build_openai_request_body` because it has no concept
/// of `reasoning_content`. Bypassing rig 0.29's deepseek converter
/// (which splits Reasoning + ToolCall into two wire messages and drops
/// the reasoning_content from the tool-call message) is the whole point
/// of having this function.
fn build_deepseek_request_body(
    model: &str,
    request: &LlmChatRequest,
    stream: bool,
) -> serde_json::Value {
    let mut messages = Vec::new();

    if let Some(ref system) = request.system {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system,
        }));
    }

    for msg in &request.messages {
        match msg.role.as_str() {
            "system" => {
                messages.push(serde_json::json!({
                    "role": "system",
                    "content": msg.content,
                }));
            }
            "user" => {
                if msg.attachments.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content,
                    }));
                } else {
                    let mut parts = vec![serde_json::json!({
                        "type": "text",
                        "text": msg.content,
                    })];
                    for att in &msg.attachments {
                        if att.mime_type.starts_with("image/") {
                            let data_url = if att.data.starts_with("data:") {
                                att.data.clone()
                            } else {
                                format!("data:{};base64,{}", att.mime_type, att.data)
                            };
                            parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": data_url },
                            }));
                        }
                    }
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": parts,
                    }));
                }
            }
            "assistant" => {
                let mut m = serde_json::json!({
                    "role": "assistant",
                    "content": msg.content,
                });
                if let Some(ref tcs) = msg.tool_calls {
                    let arr: Vec<serde_json::Value> = tcs
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                },
                            })
                        })
                        .collect();
                    m["tool_calls"] = serde_json::Value::Array(arr);
                }
                if let Some(ref r) = msg.reasoning {
                    if !r.is_empty() {
                        m["reasoning_content"] = serde_json::Value::String(r.clone());
                    }
                }
                messages.push(m);
            }
            "tool" => {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": msg.tool_call_id,
                    "content": msg.content,
                }));
            }
            _ => {
                messages.push(serde_json::json!({
                    "role": msg.role,
                    "content": msg.content,
                }));
            }
        }
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });

    if let Some(t) = request.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if let Some(m) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(m);
    }
    if let Some(ref tools) = request.tools {
        if !tools.is_empty() {
            let arr: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        },
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(arr);
            body["tool_choice"] = serde_json::json!("auto");
        }
    }

    body
}

/// Execute a chat request using raw HTTP (bypassing rig) to capture `images` field.
///
/// This is used for image generation models where rig's parser drops the `images` field.
async fn execute_raw_openai_compatible_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    base_url: &str,
) -> Result<LlmChatResponse> {
    let body = build_openai_request_body(model, &request);

    tracing::debug!(
        "Raw HTTP chat request to {}/chat/completions, model={}",
        base_url,
        model
    );

    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Raw HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(DaemonError::InternalError(format!(
            "Raw HTTP request failed with status {}: {}",
            status,
            &text[..text.len().min(500)]
        )));
    }

    let raw: RawChatResponse = response
        .json()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Failed to parse response: {}", e)))?;

    let choice = raw
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| DaemonError::InternalError("No choices in response".to_string()))?;

    // Extract text content
    let content = match choice.message.content {
        Some(serde_json::Value::String(s)) => s,
        Some(serde_json::Value::Array(arr)) => {
            // Content array: extract text parts
            arr.iter()
                .filter_map(|v| v.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        }
        _ => String::new(),
    };

    // Extract images
    let images: Vec<LlmGeneratedImage> = choice
        .message
        .images
        .iter()
        .filter_map(|img| {
            parse_data_url(&img.image_url.url)
                .map(|(media_type, data)| LlmGeneratedImage { media_type, data })
        })
        .collect();

    if !images.is_empty() {
        tracing::info!(
            "Raw HTTP response contains {} generated image(s)",
            images.len()
        );
    }

    Ok(LlmChatResponse {
        content,
        finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".to_string()),
        tool_calls: None,
        usage: None,
        images,
    })
}

/// Execute a non-streaming DeepSeek request via raw HTTP.
///
/// Sister of `stream_deepseek_raw` — see that function's doc-comment for
/// the rig 0.29 split-message bug rationale. This path also captures
/// `reasoning_content` from the response (currently surfaced only via
/// the model's outgoing turn; the host doesn't yet plumb it back into
/// `LlmChatResponse` — see follow-up tracked in plan).
async fn execute_deepseek_chat_raw(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let body = build_deepseek_request_body(model, &request, false);
    let base = base_url.unwrap_or("https://api.deepseek.com/v1");
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| DaemonError::InternalError(format!("DeepSeek request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(DaemonError::InternalError(format!(
            "DeepSeek HTTP {}: {}",
            status,
            &text[..text.len().min(500)]
        )));
    }

    let raw: serde_json::Value = response
        .json()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Failed to parse response: {}", e)))?;

    let choice = raw["choices"]
        .get(0)
        .ok_or_else(|| DaemonError::InternalError("No choices in DeepSeek response".to_string()))?;

    let message = &choice["message"];
    let content = message["content"].as_str().unwrap_or("").to_string();
    let finish_reason = choice["finish_reason"]
        .as_str()
        .unwrap_or("stop")
        .to_string();

    let tool_calls = message["tool_calls"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let function = tc.get("function")?;
                let name = function["name"].as_str()?.to_string();
                let args_raw = function["arguments"].as_str().unwrap_or("{}");
                let arguments: serde_json::Value = serde_json::from_str(args_raw)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                Some(LlmToolCall {
                    id: id.clone(),
                    call_id: Some(id),
                    name,
                    arguments,
                    signature: None,
                })
            })
            .collect::<Vec<_>>()
    });

    Ok(LlmChatResponse {
        content,
        finish_reason,
        tool_calls,
        usage: None,
        images: vec![],
    })
}

/// Execute a chat request using the DeepSeek provider.
///
/// Delegates to `execute_deepseek_chat_raw` — see that function's doc for
/// the rig 0.29 split-message bug rationale.
async fn execute_deepseek_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    _provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    execute_deepseek_chat_raw(api_key, model, request, base_url).await
}

/// Execute a chat request using the Qwen provider (custom implementation).
async fn execute_qwen_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut client = QwenClient::new(api_key);
    if let Some(url) = base_url {
        client = client.with_base_url(url);
    }
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Kimi Agent CLI provider.
async fn execute_kimi_agent_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    _base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let client = if api_key == "kimi-agent-cli" {
        KimiAgentClient::new("kimi-agent")
    } else {
        KimiAgentClient::new("kimi-agent").with_api_key(api_key)
    };

    let workspace_dir = resolve_workspace_dir();
    let client = client.with_working_dir(workspace_dir);

    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Resolve the default workspace directory for the Claude Code CLI subprocess.
///
/// Uses `$NEVOFLUX_DATA_DIR/workspace` or `~/.local/share/nevoflux/workspace`.
fn resolve_workspace_dir() -> String {
    let data_dir = std::env::var("NEVOFLUX_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            directories::ProjectDirs::from("com", "nevoflux", "nevoflux")
                .map(|dirs| dirs.data_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });
    let workspace = data_dir.join("workspace");
    std::fs::create_dir_all(&workspace).ok();
    workspace.to_string_lossy().to_string()
}

/// Resolve skills directories to pass as `--add-dir` to CLI providers.
///
/// Execute a chat request using the Google Gemini provider.
async fn execute_gemini_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = gemini::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: gemini::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Gemini client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Groq provider.
async fn execute_groq_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = groq::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: groq::Client = builder
        .build()
        .map_err(|e| DaemonError::InternalError(format!("Failed to create Groq client: {}", e)))?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Ollama provider (local models).
async fn execute_ollama_chat(
    _api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    // Ollama doesn't need an API key for local usage, use Nothing
    let mut builder = ollama::Client::builder().api_key(Nothing);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: ollama::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Ollama client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Mistral provider.
async fn execute_mistral_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = mistral::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: mistral::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Mistral client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the xAI (Grok) provider.
async fn execute_xai_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = xai::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: xai::Client = builder
        .build()
        .map_err(|e| DaemonError::InternalError(format!("Failed to create xAI client: {}", e)))?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Cohere provider.
async fn execute_cohere_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = cohere::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: cohere::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Cohere client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Perplexity provider.
async fn execute_perplexity_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = perplexity::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: perplexity::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Perplexity client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Execute a chat request using the Together AI provider.
async fn execute_together_chat(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<LlmChatResponse> {
    let mut builder = together::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: together::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Together client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    execute_rig_completion(completion_model, request, provider).await
}

/// Generic completion execution for any rig-compatible model.
async fn execute_rig_completion<M>(
    completion_model: M,
    request: LlmChatRequest,
    provider: ProviderType,
) -> Result<LlmChatResponse>
where
    M: CompletionModel,
{
    // Convert request messages to rig messages, separating system from chat history
    let mut chat_history: Vec<Message> = Vec::new();
    let mut system_prompt: Option<String> = request.system.clone();

    for (idx, msg) in request.messages.iter().enumerate() {
        tracing::info!(
            "Processing message[{}]: role={}, content_len={}, tool_calls_count={:?}, tool_call_id={:?}",
            idx,
            msg.role,
            msg.content.len(),
            msg.tool_calls.as_ref().map(|tc| tc.len()),
            msg.tool_call_id
        );
        match msg.role.as_str() {
            "system" => {
                // Use the last system message if multiple are provided
                system_prompt = Some(msg.content.clone());
            }
            "tool" => {
                let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
                tracing::info!(
                    "Converting tool result: tool_call_id={}, content_len={}, attachments={}, starts_with_call={}, starts_with_fc={}",
                    tool_call_id,
                    msg.content.len(),
                    msg.attachments.len(),
                    tool_call_id.starts_with("call_"),
                    tool_call_id.starts_with("fc_")
                );

                // Check for image attachments first (from agent-level screenshot handling)
                let image_attachment = msg
                    .attachments
                    .iter()
                    .find(|a| a.mime_type.starts_with("image/"));

                match image_attachment {
                    Some(attachment) if provider == ProviderType::Anthropic => {
                        // Anthropic: image inside tool result (native multimodal tool results)
                        let clean_b64 = clean_base64_data(&attachment.data);
                        let detected_media_type =
                            detect_image_media_type_from_base64(&clean_b64, &attachment.mime_type);
                        let tool_result = ToolResult {
                            id: tool_call_id.clone(),
                            call_id: Some(tool_call_id),
                            content: OneOrMany::many(vec![
                                ToolResultContent::text(&msg.content),
                                ToolResultContent::image_base64(
                                    &clean_b64,
                                    Some(detected_media_type),
                                    None,
                                ),
                            ])
                            .unwrap_or_else(|_| {
                                OneOrMany::one(ToolResultContent::text(&msg.content))
                            }),
                        };
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                        });
                    }
                    Some(attachment) => {
                        // OpenAI/others: text tool result + separate user image message
                        let clean_b64 = clean_base64_data(&attachment.data);
                        let detected_media_type =
                            detect_image_media_type_from_base64(&clean_b64, &attachment.mime_type);
                        let tool_result = ToolResult {
                            id: tool_call_id.clone(),
                            call_id: Some(tool_call_id),
                            content: OneOrMany::one(ToolResultContent::text(&msg.content)),
                        };
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                        });
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::Image(Image {
                                data: DocumentSourceKind::Base64(clean_b64),
                                media_type: Some(detected_media_type),
                                detail: Some(ImageDetail::Auto),
                                additional_params: None,
                            })),
                        });
                    }
                    None => {
                        // No attachment — fallback to extract_screenshot_from_tool_result
                        match extract_screenshot_from_tool_result(&msg.content) {
                            Some(screenshot) if provider == ProviderType::Anthropic => {
                                let detected_media_type = detect_image_media_type_from_base64(
                                    &screenshot.base64_data,
                                    "image/png",
                                );
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::many(vec![
                                        ToolResultContent::text(
                                            &screenshot.text_without_screenshot,
                                        ),
                                        ToolResultContent::image_base64(
                                            &screenshot.base64_data,
                                            Some(detected_media_type),
                                            None,
                                        ),
                                    ])
                                    .unwrap_or_else(|_| {
                                        OneOrMany::one(ToolResultContent::text(&msg.content))
                                    }),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                            }
                            Some(screenshot) => {
                                let detected_media_type = detect_image_media_type_from_base64(
                                    &screenshot.base64_data,
                                    "image/png",
                                );
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::one(ToolResultContent::text(
                                        &screenshot.text_without_screenshot,
                                    )),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::Image(Image {
                                        data: DocumentSourceKind::Base64(screenshot.base64_data),
                                        media_type: Some(detected_media_type),
                                        detail: Some(ImageDetail::Auto),
                                        additional_params: None,
                                    })),
                                });
                            }
                            None => {
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::one(ToolResultContent::Text(Text {
                                        text: msg.content.clone(),
                                    })),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                            }
                        }
                    }
                }
            }
            "assistant" => {
                tracing::info!(
                    "Converting assistant message: content_len={}, tool_calls_present={}, tool_calls_count={:?}, has_reasoning={}",
                    msg.content.len(),
                    msg.tool_calls.is_some(),
                    msg.tool_calls.as_ref().map(|tc| tc.len()),
                    msg.reasoning.is_some()
                );
                if let Some(ref tool_calls) = msg.tool_calls {
                    tracing::info!(
                        "Converting assistant tool_calls: count={}, ids={:?}, call_ids={:?}",
                        tool_calls.len(),
                        tool_calls.iter().map(|tc| &tc.id).collect::<Vec<_>>(),
                        tool_calls.iter().map(|tc| &tc.call_id).collect::<Vec<_>>()
                    );
                }
                chat_history.push(build_rig_assistant_message(msg, provider));
            }
            _ => {
                // Treat as user message, with optional attachments
                let mut user_content: Vec<UserContent> = Vec::new();

                // Add text content if present
                if !msg.content.is_empty() {
                    user_content.push(UserContent::text(&msg.content));
                }

                // Add image attachments
                for attachment in &msg.attachments {
                    tracing::info!(
                        "Processing attachment: name={}, mime_type={}, data_len={}",
                        attachment.name,
                        attachment.mime_type,
                        attachment.data.len()
                    );

                    // Warn about large images that may cause intermittent failures with some models
                    // Known issue: gpt-4o-mini can intermittently fail to process images larger than ~200KB base64
                    // Recommendation: Use gpt-4o for large image processing, or resize images before sending
                    const LARGE_IMAGE_THRESHOLD: usize = 200_000; // ~200KB base64
                    if attachment.data.len() > LARGE_IMAGE_THRESHOLD {
                        tracing::warn!(
                            "Large image attachment detected: {} ({} bytes). Some models (e.g., gpt-4o-mini) may have \
                            intermittent issues with large images. Consider using gpt-4o for better reliability.",
                            attachment.name,
                            attachment.data.len()
                        );
                    }

                    if mime_to_image_media_type(&attachment.mime_type).is_some() {
                        // Strip data URL prefix if present (e.g., "data:image/png;base64,")
                        let base64_data = if attachment.data.starts_with("data:") {
                            attachment
                                .data
                                .find(",")
                                .map(|i| &attachment.data[i + 1..])
                                .unwrap_or(&attachment.data)
                        } else {
                            &attachment.data
                        };
                        // Remove any whitespace/newlines from base64 (some encoders add line breaks)
                        let clean_base64: String =
                            base64_data.chars().filter(|c| !c.is_whitespace()).collect();
                        tracing::info!(
                            "Using cleaned base64 data: original_len={}, clean_len={}",
                            base64_data.len(),
                            clean_base64.len()
                        );
                        let detected_media_type = detect_image_media_type_from_base64(
                            &clean_base64,
                            &attachment.mime_type,
                        );
                        user_content.push(UserContent::Image(Image {
                            data: DocumentSourceKind::Base64(clean_base64),
                            media_type: Some(detected_media_type),
                            detail: Some(ImageDetail::Auto),
                            additional_params: None,
                        }));
                    }
                }

                if !user_content.is_empty() {
                    chat_history.push(Message::User {
                        content: OneOrMany::many(user_content)
                            .unwrap_or_else(|_| OneOrMany::one(UserContent::text(" "))),
                    });
                }
            }
        }
    }

    // Merge consecutive same-role messages to ensure strict user/assistant alternation
    let chat_history = merge_consecutive_same_role_messages(chat_history);

    // Check if we have any messages
    if chat_history.is_empty() {
        return Err(DaemonError::InternalError(
            "LLM chat requires at least one user message".into(),
        ));
    }

    // Build the completion request
    // IMPORTANT: rig's builder appends prompt to the END of chat_history.
    // We extract the last user message and pass it as the prompt so rig places it at the end.
    // For multi-turn with multimodal content, we keep all messages in chat_history and use
    // the text from the last user message as prompt. Some API providers return empty responses
    // when a multimodal message is passed directly as the prompt in multi-turn conversations.
    let (prompt_message, chat_history) = {
        let last_user_idx = chat_history
            .iter()
            .rposition(|m| matches!(m, Message::User { .. }));
        if let Some(idx) = last_user_idx {
            let has_multimodal = match &chat_history[idx] {
                Message::User { content } => content
                    .iter()
                    .any(|c| matches!(c, UserContent::Image(_) | UserContent::Document(_))),
                _ => false,
            };
            if has_multimodal && idx > 0 {
                // Multi-turn multimodal: keep all messages in chat_history,
                // use text from last user message as prompt
                let text = match &chat_history[idx] {
                    Message::User { content } => content
                        .iter()
                        .filter_map(|c| match c {
                            UserContent::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                (Message::from(text), chat_history)
            } else {
                // Single-turn or text-only: pop the message as prompt
                let mut history = chat_history;
                let msg = history.remove(idx);
                (msg, history)
            }
        } else {
            (Message::from(""), chat_history)
        }
    };

    // Build the completion request using the builder pattern
    let mut builder = completion_model.completion_request(prompt_message);

    // Add system prompt (preamble) if present
    if let Some(preamble) = system_prompt {
        builder = builder.preamble(preamble);
    }

    // Add chat history
    builder = builder.messages(chat_history);

    // Add tools if provided
    if let Some(tools) = request.tools {
        let rig_tools: Vec<ToolDefinition> = tools.into_iter().map(|t| t.into()).collect();
        builder = builder.tools(rig_tools);
    }

    // Anthropic mandates `max_tokens`. See the matching block in
    // `stream_rig_completion` for the long version of why this exists.
    let resolved_max_tokens: u64 = request
        .max_tokens
        .map(|n| n as u64)
        .unwrap_or(DEFAULT_MAX_TOKENS_FALLBACK);
    builder = builder.max_tokens(resolved_max_tokens);

    // Execute the request
    let completion_response = builder
        .send()
        .await
        .map_err(|e| DaemonError::InternalError(format!("LLM chat failed: {}", e)))?;

    // Extract the response content and handle tool calls
    process_completion_response(completion_response.choice)
}

/// Build a rig `Message::Assistant` from a host-side `LlmMessage`.
/// Provider-agnostic: does not emit reasoning content.
fn build_rig_assistant_message(msg: &LlmMessage, _provider: ProviderType) -> Message {
    let mut assistant_contents: Vec<AssistantContent> = Vec::new();

    if !msg.content.is_empty() {
        assistant_contents.push(AssistantContent::text(&msg.content));
    }

    if let Some(ref tool_calls) = msg.tool_calls {
        for tc in tool_calls {
            let mut rig_tool_call = RigToolCall::new(
                tc.id.clone(),
                ToolFunction::new(tc.name.clone(), tc.arguments.clone()),
            );
            if let Some(ref call_id) = tc.call_id {
                rig_tool_call = rig_tool_call.with_call_id(call_id.clone());
            }
            assistant_contents.push(AssistantContent::ToolCall(rig_tool_call));
        }
    }

    if assistant_contents.is_empty() {
        assistant_contents.push(AssistantContent::text(" "));
    }

    let content = if assistant_contents.len() == 1 {
        OneOrMany::one(assistant_contents.remove(0))
    } else {
        OneOrMany::many(assistant_contents)
            .unwrap_or_else(|_| OneOrMany::one(AssistantContent::text(" ")))
    };

    Message::Assistant { id: None, content }
}

/// Merge consecutive messages with the same role into single messages.
///
/// LLM APIs typically require strict user/assistant alternation. This function
/// consolidates consecutive same-role messages by combining their content items
/// into a single message.
fn merge_consecutive_same_role_messages(messages: Vec<Message>) -> Vec<Message> {
    let original_len = messages.len();
    if original_len <= 1 {
        return messages;
    }

    let mut merged: Vec<Message> = Vec::with_capacity(original_len);

    for msg in messages {
        let should_merge = match (&merged.last(), &msg) {
            (Some(Message::User { content: existing }), Message::User { content: new }) => {
                // Don't merge if either message contains ToolResult — rig's OpenAI provider
                // drops non-ToolResult content (e.g. Image) when ToolResult is present.
                // Tool result images are intentionally separated into their own User message.
                let existing_has_tool_result = existing
                    .iter()
                    .any(|c| matches!(c, UserContent::ToolResult(_)));
                let new_has_tool_result =
                    new.iter().any(|c| matches!(c, UserContent::ToolResult(_)));
                !existing_has_tool_result && !new_has_tool_result
            }
            (Some(Message::Assistant { .. }), Message::Assistant { .. }) => true,
            _ => false,
        };

        if should_merge {
            let last = merged.last_mut().unwrap();
            match (last, msg) {
                (
                    Message::User {
                        content: existing, ..
                    },
                    Message::User { content: new, .. },
                ) => {
                    for item in new.into_iter() {
                        existing.push(item);
                    }
                }
                (
                    Message::Assistant {
                        content: existing, ..
                    },
                    Message::Assistant { content: new, .. },
                ) => {
                    for item in new.into_iter() {
                        existing.push(item);
                    }
                }
                _ => unreachable!(),
            }
        } else {
            merged.push(msg);
        }
    }

    if merged.len() < original_len {
        tracing::info!(
            "Merged consecutive same-role messages: {} -> {}",
            original_len,
            merged.len()
        );
    }

    merged
}

/// Convert MIME type string to rig ImageMediaType.
fn mime_to_image_media_type(mime: &str) -> Option<ImageMediaType> {
    match mime.to_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some(ImageMediaType::JPEG),
        "image/png" => Some(ImageMediaType::PNG),
        "image/gif" => Some(ImageMediaType::GIF),
        "image/webp" => Some(ImageMediaType::WEBP),
        _ => None,
    }
}

/// Detect image format from base64-encoded data by checking magic bytes.
/// Returns the detected ImageMediaType, falling back to the declared MIME type.
fn detect_image_media_type_from_base64(base64_data: &str, declared_mime: &str) -> ImageMediaType {
    // Base64-encoded magic byte prefixes:
    // JPEG: /9j/       (0xFF 0xD8 0xFF)
    // PNG:  iVBORw0KGo (0x89 0x50 0x4E 0x47 = \x89PNG)
    // GIF:  R0lGOD      (GIF87a/GIF89a)
    // WEBP: UklGR       (RIFF....WEBP)
    let detected = if base64_data.starts_with("/9j/") {
        Some(ImageMediaType::JPEG)
    } else if base64_data.starts_with("iVBORw0KGo") {
        Some(ImageMediaType::PNG)
    } else if base64_data.starts_with("R0lGOD") {
        Some(ImageMediaType::GIF)
    } else if base64_data.starts_with("UklGR") {
        Some(ImageMediaType::WEBP)
    } else {
        None
    };

    match detected {
        Some(media_type) => {
            if let Some(declared) = mime_to_image_media_type(declared_mime) {
                if std::mem::discriminant(&media_type) != std::mem::discriminant(&declared) {
                    tracing::warn!(
                        "Image format mismatch: declared={}, detected={:?}. Using detected format.",
                        declared_mime,
                        media_type
                    );
                }
            }
            media_type
        }
        None => mime_to_image_media_type(declared_mime).unwrap_or(ImageMediaType::PNG),
    }
}

/// Clean base64 data by stripping data URL prefix and removing whitespace.
///
/// Some encoders produce `data:image/png;base64,iVBOR...` or add line breaks.
/// The Anthropic API requires raw base64 without prefix or whitespace.
fn clean_base64_data(data: &str) -> String {
    let stripped = if data.starts_with("data:") {
        data.find(",").map(|i| &data[i + 1..]).unwrap_or(data)
    } else {
        data
    };
    stripped.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Extract text content from a rig Message.

/// Process the completion response and convert to LlmChatResponse.
fn process_completion_response(choice: OneOrMany<AssistantContent>) -> Result<LlmChatResponse> {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<LlmToolCall> = Vec::new();

    for content in choice.iter() {
        match content {
            AssistantContent::Text(t) => {
                text_parts.push(t.text.clone());
            }
            AssistantContent::ToolCall(tc) => {
                // Parse stringified JSON arguments into Object.
                // Some providers return arguments as a JSON string even in complete tool calls.
                let arguments = match &tc.function.arguments {
                    serde_json::Value::String(s) => parse_tool_arguments_json(s),
                    other => other.clone(),
                };
                tool_calls.push(LlmToolCall {
                    id: tc.id.clone(),
                    call_id: tc.call_id.clone(),
                    name: tc.function.name.clone(),
                    arguments,
                    signature: tc.signature.clone(),
                });
            }
            AssistantContent::Reasoning(r) => {
                // Include reasoning as text with a prefix
                let reasoning_text = r.reasoning.join(" ");
                if !reasoning_text.is_empty() {
                    text_parts.push(format!("[Reasoning]: {}", reasoning_text));
                }
            }
            AssistantContent::Image(_) => {
                // Skip images for now
            }
        }
    }

    if !tool_calls.is_empty() {
        Ok(LlmChatResponse {
            content: text_parts.join("\n"),
            finish_reason: "tool_calls".into(),
            tool_calls: Some(tool_calls),
            usage: None,
            images: vec![],
        })
    } else {
        Ok(LlmChatResponse {
            content: text_parts.join("\n"),
            finish_reason: "stop".into(),
            tool_calls: None,
            usage: None,
            images: vec![],
        })
    }
}

// ============================================================================
// Streaming Support
// ============================================================================

/// A streaming chunk from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmStreamChunk {
    /// Text delta (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Tool calls in this chunk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<LlmToolCall>,
    /// Whether this is the final chunk.
    #[serde(default)]
    pub done: bool,
    /// Reasoning/thinking content from LLM (provider-agnostic).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Generated images in this chunk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<LlmGeneratedImage>,
}

/// Stream entry for tracking active LLM streams.
pub struct LlmStreamEntry {
    /// Receiver for chunks from the background task.
    pub receiver: mpsc::Receiver<LlmStreamChunk>,
    /// Whether the stream is done.
    pub done: bool,
}

/// Registry for managing active LLM streams.
pub struct LlmStreamRegistry {
    /// Next available stream ID.
    next_id: AtomicU64,
    /// Map of stream ID to entry.
    entries: RwLock<HashMap<u64, LlmStreamEntry>>,
}

impl LlmStreamRegistry {
    /// Create a new stream registry.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Allocate a new stream ID.
    pub fn allocate_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Register a stream receiver.
    pub fn register(&self, id: u64, receiver: mpsc::Receiver<LlmStreamChunk>) {
        let mut entries = self.entries.write().unwrap();
        entries.insert(
            id,
            LlmStreamEntry {
                receiver,
                done: false,
            },
        );
    }

    /// Get the next chunk from a stream.
    pub fn next_chunk(&self, id: u64) -> Result<Option<LlmStreamChunk>> {
        let mut entries = self.entries.write().unwrap();
        let entry = entries
            .get_mut(&id)
            .ok_or_else(|| DaemonError::InternalError(format!("Stream {} not found", id)))?;

        if entry.done {
            return Ok(None);
        }

        // Try to receive the next chunk (non-blocking would require async)
        // For now, we block briefly
        match entry.receiver.try_recv() {
            Ok(chunk) => {
                if chunk.done {
                    entry.done = true;
                }
                Ok(Some(chunk))
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                // No chunk available yet
                Ok(None)
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                entry.done = true;
                Ok(None)
            }
        }
    }

    /// Close and remove a stream.
    pub fn close(&self, id: u64) {
        let mut entries = self.entries.write().unwrap();
        entries.remove(&id);
    }
}

impl Default for LlmStreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Start a streaming LLM chat request.
///
/// Returns a stream ID and spawns a background task to process the stream.
pub async fn start_llm_stream(
    provider: ProviderType,
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    registry: Arc<LlmStreamRegistry>,
    base_url: Option<&str>,
    host_services: Option<crate::wasm::services::HostServices>,
) -> Result<u64> {
    let stream_id = registry.allocate_id();
    let (tx, rx) = mpsc::channel(32);

    // Register the receiver
    registry.register(stream_id, rx);

    // Clone values for the spawned task
    let api_key = api_key.to_string();
    let model = model.to_string();
    let base_url_owned = base_url.map(String::from);

    // Spawn background task to process the stream
    tokio::spawn(async move {
        let result = execute_llm_stream_inner(
            provider,
            &api_key,
            &model,
            request,
            tx.clone(),
            base_url_owned.as_deref(),
            host_services,
        )
        .await;
        if let Err(e) = result {
            tracing::error!("Stream error: {}", e);
            // Send error as final chunk
            let _ = tx
                .send(LlmStreamChunk {
                    text: Some(format!("[Error: {}]", e)),
                    tool_calls: vec![],
                    done: true,
                    reasoning: None,
                    images: vec![],
                })
                .await;
        }
    });

    Ok(stream_id)
}

/// Internal function to execute a streaming LLM request.
async fn execute_llm_stream_inner(
    provider: ProviderType,
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    base_url: Option<&str>,
    _host_services: Option<crate::wasm::services::HostServices>,
) -> Result<()> {
    match provider {
        ProviderType::Anthropic => {
            stream_anthropic(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::OpenAi => {
            stream_openai(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::OpenRouter => {
            stream_openrouter(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::DeepSeek => {
            stream_deepseek(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::Gemini => {
            stream_gemini(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::Groq => stream_groq(api_key, model, request, tx, provider, base_url).await,
        ProviderType::Mistral => {
            stream_mistral(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::XAi => stream_xai(api_key, model, request, tx, provider, base_url).await,
        ProviderType::Cohere => {
            stream_cohere(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::Perplexity => {
            stream_perplexity(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::Together => {
            stream_together(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::ClaudeCode | ProviderType::GeminiCli | ProviderType::OpenClaw => {
            stream_acp_completion(
                api_key,
                model,
                request,
                tx,
                provider,
                base_url,
                _host_services,
            )
            .await
        }
        ProviderType::KimiAgent => {
            stream_kimi_agent(api_key, model, request, tx, provider, base_url).await
        }
        ProviderType::Qwen => stream_qwen(api_key, model, request, tx, provider, base_url).await,
        // Ollama doesn't support streaming in rig yet
        ProviderType::Ollama => Err(DaemonError::InternalError(format!(
            "Streaming not supported for provider {:?}",
            provider
        ))),
    }
}

/// Stream from Anthropic provider.
async fn stream_anthropic(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = anthropic::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: anthropic::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Anthropic client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from OpenAI provider.
///
/// When `base_url` is empty, uses rig's standard `openai::Client` (Responses API).
/// When `base_url` is set, uses `openai::CompletionsClient` (Chat Completions API).
async fn stream_openai(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    if let Some(url) = base_url {
        // Custom endpoint: use Chat Completions API (/chat/completions)
        let client: openai::CompletionsClient = openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(url)
            .build()
            .map_err(|e| {
                DaemonError::InternalError(format!("Failed to create OpenAI client: {}", e))
            })?;
        let completion_model = client.completion_model(model);
        stream_rig_completion(completion_model, request, tx, provider).await
    } else {
        // Official OpenAI: use rig's standard client (Responses API)
        let client: openai::Client =
            openai::Client::builder()
                .api_key(api_key)
                .build()
                .map_err(|e| {
                    DaemonError::InternalError(format!("Failed to create OpenAI client: {}", e))
                })?;
        let completion_model = client.completion_model(model);
        stream_rig_completion(completion_model, request, tx, provider).await
    }
}

/// Stream from OpenRouter provider.
///
/// For image generation models, uses non-streaming raw HTTP and emulates streaming,
/// since image generation responses come as a single response with large binary data.
async fn stream_openrouter(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    if is_image_generation_model(model) {
        // For image generation: use non-streaming, then emit as stream chunks
        let response = execute_raw_openai_compatible_chat(
            api_key,
            model,
            request,
            base_url.unwrap_or("https://openrouter.ai/api/v1"),
        )
        .await?;

        // Emit text content if any
        if !response.content.is_empty() {
            let _ = tx
                .send(LlmStreamChunk {
                    text: Some(response.content),
                    tool_calls: vec![],
                    done: false,
                    reasoning: None,
                    images: vec![],
                })
                .await;
        }

        // Emit images
        if !response.images.is_empty() {
            let _ = tx
                .send(LlmStreamChunk {
                    text: None,
                    tool_calls: vec![],
                    done: false,
                    reasoning: None,
                    images: response.images,
                })
                .await;
        }

        // Done
        let _ = tx
            .send(LlmStreamChunk {
                text: None,
                tool_calls: vec![],
                done: true,
                reasoning: None,
                images: vec![],
            })
            .await;

        return Ok(());
    }

    let mut builder = openrouter::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: openrouter::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create OpenRouter client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from DeepSeek provider.
///
/// Delegates to `stream_deepseek_raw` — see that function's doc for
/// the rig 0.29 split-message bug rationale.
async fn stream_deepseek(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    _provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    stream_deepseek_raw(api_key, model, request, tx, base_url).await
}

/// Stream from Qwen provider using raw HTTP + SSE parsing.
///
/// We bypass rig's streaming trait because Qwen's rig `stream()` implementation
/// doesn't handle tool_calls in deltas. This raw implementation sends tools
/// in the request and parses both content and tool_calls from the SSE stream.
async fn stream_qwen(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    _provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    use futures::StreamExt;
    use nevoflux_llm::providers::qwen::QwenMessage;

    let mut client = QwenClient::new(api_key);
    if let Some(url) = base_url {
        client = client.with_base_url(url);
    }

    // Convert messages to QwenMessage format
    let mut qwen_messages = Vec::new();
    if let Some(system) = &request.system {
        qwen_messages.push(QwenMessage::system(system.clone()));
    }
    for msg in &request.messages {
        match msg.role.as_str() {
            "user" => qwen_messages.push(QwenMessage::user(&msg.content)),
            "assistant" => qwen_messages.push(QwenMessage::assistant(&msg.content)),
            "system" => qwen_messages.push(QwenMessage::system(&msg.content)),
            "tool" => {
                let tool_msg =
                    QwenMessage::tool(msg.tool_call_id.as_deref().unwrap_or(""), &msg.content);
                qwen_messages.push(tool_msg);
            }
            _ => qwen_messages.push(QwenMessage::user(&msg.content)),
        }
    }

    // Build request JSON with optional tools
    let mut body = serde_json::json!({
        "model": model,
        "messages": qwen_messages,
        "stream": true,
    });
    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            let qwen_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(qwen_tools);
            body["tool_choice"] = serde_json::json!("auto");
        }
    }
    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(max);
    }

    let base = base_url.unwrap_or("https://dashscope.aliyuncs.com/compatible-mode/v1");
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(format!("{}/chat/completions", base))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| DaemonError::InternalError(format!("Qwen stream request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(DaemonError::InternalError(format!(
            "Qwen stream HTTP {}: {}",
            status, text
        )));
    }

    let mut byte_stream = response.bytes_stream();
    // Accumulate streaming tool call deltas (arguments come in fragments)
    struct ToolCallAccum {
        id: String,
        name: String,
        arguments: String,
    }
    let mut accumulated_tool_calls: HashMap<i64, ToolCallAccum> = HashMap::new();

    while let Some(result) = byte_stream.next().await {
        match result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let data = match line.strip_prefix("data: ") {
                        Some(d) if d != "[DONE]" => d,
                        _ => continue,
                    };

                    let chunk: serde_json::Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let choice = match chunk["choices"].get(0) {
                        Some(c) => c,
                        None => continue,
                    };
                    let delta = &choice["delta"];

                    // Handle text content
                    if let Some(content) = delta["content"].as_str() {
                        if !content.is_empty() {
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: Some(content.to_string()),
                                    tool_calls: vec![],
                                    done: false,
                                    reasoning: None,
                                    images: vec![],
                                })
                                .await;
                        }
                    }

                    // Handle tool calls in delta
                    if let Some(tool_calls) = delta["tool_calls"].as_array() {
                        for tc in tool_calls {
                            let index = tc["index"].as_i64().unwrap_or(0);
                            let entry = accumulated_tool_calls.entry(index).or_insert_with(|| {
                                ToolCallAccum {
                                    id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                }
                            });

                            if let Some(id) = tc["id"].as_str() {
                                entry.id = id.to_string();
                            }
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func["name"].as_str() {
                                    entry.name = name.to_string();
                                }
                                if let Some(args) = func["arguments"].as_str() {
                                    entry.arguments.push_str(args);
                                }
                            }
                        }
                    }

                    // Handle reasoning/thinking content
                    if let Some(reasoning) = delta["reasoning_content"].as_str() {
                        if !reasoning.is_empty() {
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: None,
                                    tool_calls: vec![],
                                    done: false,
                                    reasoning: Some(reasoning.to_string()),
                                    images: vec![],
                                })
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Qwen stream chunk error: {}", e);
                break;
            }
        }
    }

    // Send accumulated tool calls if any
    if !accumulated_tool_calls.is_empty() {
        let mut tool_calls: Vec<LlmToolCall> = accumulated_tool_calls
            .into_values()
            .map(|tc| LlmToolCall {
                id: tc.id.clone(),
                call_id: Some(tc.id),
                name: tc.name,
                arguments: serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
                signature: None,
            })
            .collect();
        tool_calls.sort_by_key(|tc| tc.id.clone());
        let _ = tx
            .send(LlmStreamChunk {
                text: None,
                tool_calls,
                done: false,
                reasoning: None,
                images: vec![],
            })
            .await;
    }

    // Send final done chunk
    let _ = tx
        .send(LlmStreamChunk {
            text: None,
            tool_calls: vec![],
            done: true,
            reasoning: None,
            images: vec![],
        })
        .await;

    Ok(())
}

/// Stream from DeepSeek using raw HTTP + SSE parsing.
///
/// Bypasses rig 0.29's `rig::providers::deepseek` because its
/// `TryFrom<message::Message>` for `Message::Assistant` splits a rig
/// assistant message containing both `Reasoning` and `ToolCall` content
/// variants into TWO separate wire messages: the first carries
/// `reasoning_content` with empty `tool_calls`, the second carries
/// `tool_calls` with `reasoning_content: None`. DeepSeek validates the
/// second message, sees the missing reasoning_content, and returns
/// 400 invalid_request_error on every tool-using thinking-mode turn.
///
/// We sidestep that by emitting the wire JSON ourselves so reasoning
/// and tool_calls ride on the same assistant message — which matches
/// DeepSeek's API contract per https://api-docs.deepseek.com/zh-cn/guides/thinking_mode.
async fn stream_deepseek_raw(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    base_url: Option<&str>,
) -> Result<()> {
    use futures::StreamExt;

    let body = build_deepseek_request_body(model, &request, true);
    let base = base_url.unwrap_or("https://api.deepseek.com/v1");
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));

    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            DaemonError::InternalError(format!("DeepSeek stream request failed: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(DaemonError::InternalError(format!(
            "DeepSeek stream HTTP {}: {}",
            status,
            &text[..text.len().min(500)]
        )));
    }

    // Accumulate streaming tool call deltas (arguments come in fragments).
    struct ToolCallAccum {
        id: String,
        name: String,
        arguments: String,
    }
    let mut accumulated_tool_calls: HashMap<i64, ToolCallAccum> = HashMap::new();

    // SSE lines may be split across byte chunks — buffer until we see a newline.
    let mut byte_stream = response.bytes_stream();
    let mut line_buf = String::new();

    while let Some(result) = byte_stream.next().await {
        match result {
            Ok(bytes) => {
                line_buf.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(nl_pos) = line_buf.find('\n') {
                    let line = line_buf[..nl_pos].trim_end_matches('\r').to_string();
                    line_buf.drain(..=nl_pos);

                    let data = match line.strip_prefix("data: ") {
                        Some(d) if d != "[DONE]" => d.to_string(),
                        _ => continue,
                    };

                    let chunk: serde_json::Value = match serde_json::from_str(&data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let choice = match chunk["choices"].get(0) {
                        Some(c) => c,
                        None => continue,
                    };
                    let delta = &choice["delta"];

                    // Text content delta.
                    if let Some(content) = delta["content"].as_str() {
                        if !content.is_empty() {
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: Some(content.to_string()),
                                    tool_calls: vec![],
                                    done: false,
                                    reasoning: None,
                                    images: vec![],
                                })
                                .await;
                        }
                    }

                    // Reasoning_content delta — surfaced as `reasoning` on the chunk.
                    if let Some(r) = delta["reasoning_content"].as_str() {
                        if !r.is_empty() {
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: None,
                                    tool_calls: vec![],
                                    done: false,
                                    reasoning: Some(r.to_string()),
                                    images: vec![],
                                })
                                .await;
                        }
                    }

                    // Tool-call deltas — accumulate by index, finalize at end.
                    if let Some(tcs) = delta["tool_calls"].as_array() {
                        for tc in tcs {
                            let index = tc["index"].as_i64().unwrap_or(0);
                            let entry = accumulated_tool_calls.entry(index).or_insert_with(|| {
                                ToolCallAccum {
                                    id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                }
                            });
                            if let Some(id) = tc["id"].as_str() {
                                entry.id = id.to_string();
                            }
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func["name"].as_str() {
                                    entry.name = name.to_string();
                                }
                                if let Some(args) = func["arguments"].as_str() {
                                    entry.arguments.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("DeepSeek stream chunk error: {}", e);
                break;
            }
        }
    }

    // Emit accumulated tool calls in a single chunk before `done`.
    if !accumulated_tool_calls.is_empty() {
        let mut tool_calls: Vec<LlmToolCall> = accumulated_tool_calls
            .into_values()
            .map(|tc| LlmToolCall {
                id: tc.id.clone(),
                call_id: Some(tc.id),
                name: tc.name,
                arguments: serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
                signature: None,
            })
            .collect();
        tool_calls.sort_by_key(|tc| tc.id.clone());
        let _ = tx
            .send(LlmStreamChunk {
                text: None,
                tool_calls,
                done: false,
                reasoning: None,
                images: vec![],
            })
            .await;
    }

    let _ = tx
        .send(LlmStreamChunk {
            text: None,
            tool_calls: vec![],
            done: true,
            reasoning: None,
            images: vec![],
        })
        .await;

    Ok(())
}

/// Stream from Gemini provider.
async fn stream_gemini(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = gemini::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: gemini::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Gemini client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Groq provider.
async fn stream_groq(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = groq::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: groq::Client = builder
        .build()
        .map_err(|e| DaemonError::InternalError(format!("Failed to create Groq client: {}", e)))?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Mistral provider.
async fn stream_mistral(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = mistral::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: mistral::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Mistral client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from xAI provider.
async fn stream_xai(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = xai::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: xai::Client = builder
        .build()
        .map_err(|e| DaemonError::InternalError(format!("Failed to create xAI client: {}", e)))?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Cohere provider.
async fn stream_cohere(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = cohere::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: cohere::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Cohere client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Perplexity provider.
async fn stream_perplexity(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = perplexity::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: perplexity::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Perplexity client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Together provider.
async fn stream_together(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    base_url: Option<&str>,
) -> Result<()> {
    let mut builder = together::Client::builder().api_key(api_key);
    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }
    let client: together::Client = builder.build().map_err(|e| {
        DaemonError::InternalError(format!("Failed to create Together client: {}", e))
    })?;
    let completion_model = client.completion_model(model);
    stream_rig_completion(completion_model, request, tx, provider).await
}

/// Stream from Kimi Agent CLI provider.
///
/// Uses the kimi-agent wire protocol's native streaming: spawns the subprocess,
/// reads JSON-RPC events one at a time, and emits each ContentPart as a stream chunk.
async fn stream_kimi_agent(
    api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    _provider: ProviderType,
    _base_url: Option<&str>,
) -> Result<()> {
    use nevoflux_llm::providers::kimi_agent::wire::{WireClient, WireEvent};

    let client = if api_key == "kimi-agent-cli" {
        KimiAgentClient::new("kimi-agent")
    } else {
        KimiAgentClient::new("kimi-agent").with_api_key(api_key)
    };
    let workspace_dir = resolve_workspace_dir();
    let config = client.with_working_dir(workspace_dir);
    let model_str = model.to_string();

    // Build prompt from request messages (may be string or ContentPart[])
    let prompt = build_kimi_prompt(&request);
    let has_media = matches!(&prompt, serde_json::Value::Array(_));

    // Convert tool definitions
    let tools: Vec<ToolDefinition> = request
        .tools
        .map(|t| t.into_iter().map(Into::into).collect())
        .unwrap_or_default();

    // Channel to bridge blocking wire reads to async
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<WireEvent>(64);

    tracing::info!(
        "stream_kimi_agent: spawning wire client, model={}, prompt_len={}, has_media={}, tools={}",
        model_str,
        prompt.to_string().len(),
        has_media,
        tools.len()
    );

    // Spawn blocking thread for wire protocol I/O
    tokio::task::spawn_blocking(move || {
        tracing::info!("stream_kimi_agent: spawn_blocking thread started");
        let mut wc = match WireClient::spawn(&config, &model_str) {
            Ok(wc) => {
                tracing::info!("stream_kimi_agent: kimi-agent process spawned");
                wc
            }
            Err(e) => {
                tracing::error!("stream_kimi_agent: failed to spawn kimi-agent: {}", e);
                let _ = event_tx.blocking_send(WireEvent::Unknown(format!(
                    "error: failed to spawn kimi-agent: {}",
                    e
                )));
                return;
            }
        };
        if let Err(e) = wc.initialize(&tools) {
            tracing::error!("stream_kimi_agent: failed to initialize: {}", e);
            let _ = event_tx.blocking_send(WireEvent::Unknown(format!(
                "error: failed to initialize kimi-agent: {}",
                e
            )));
            return;
        }
        tracing::info!("stream_kimi_agent: initialized successfully");

        let send_result = match &prompt {
            serde_json::Value::String(text) => wc.send_prompt(text),
            _ => wc.send_prompt_multimodal(prompt),
        };
        if let Err(e) = send_result {
            tracing::error!("stream_kimi_agent: failed to send prompt: {}", e);
            let _ = event_tx.blocking_send(WireEvent::Unknown(format!(
                "error: failed to send prompt: {}",
                e
            )));
            return;
        }
        tracing::info!("stream_kimi_agent: prompt sent, reading events...");

        let mut event_count: u64 = 0;
        loop {
            match wc.read_next_event() {
                Some(event) => {
                    event_count += 1;
                    let is_terminal = matches!(
                        event,
                        WireEvent::TurnEnd | WireEvent::ToolCallRequest { .. }
                    );
                    if event_count <= 10 || is_terminal {
                        tracing::info!("stream_kimi_agent: event #{}: {:?}", event_count, event);
                    }
                    if event_tx.blocking_send(event).is_err() {
                        tracing::warn!(
                            "stream_kimi_agent: receiver dropped after {} events",
                            event_count
                        );
                        break;
                    }
                    if is_terminal {
                        tracing::info!(
                            "stream_kimi_agent: terminal event after {} events",
                            event_count
                        );
                        break;
                    }
                }
                None => {
                    tracing::info!(
                        "stream_kimi_agent: wire stream ended after {} events",
                        event_count
                    );
                    break;
                }
            }
        }
    });

    // Receive wire events and emit stream chunks
    let mut tool_calls = Vec::new();
    while let Some(event) = event_rx.recv().await {
        match event {
            WireEvent::ContentPart { text } => {
                let _ = tx
                    .send(LlmStreamChunk {
                        text: Some(text),
                        tool_calls: vec![],
                        done: false,
                        reasoning: None,
                        images: vec![],
                    })
                    .await;
            }
            WireEvent::ToolCallRequest {
                id,
                name,
                arguments,
            } => {
                tool_calls.push(LlmToolCall {
                    id,
                    call_id: None,
                    name,
                    arguments,
                    signature: None,
                });
            }
            WireEvent::TurnEnd => break,
            WireEvent::ThinkingPart { text } => {
                let _ = tx
                    .send(LlmStreamChunk {
                        text: None,
                        tool_calls: vec![],
                        done: false,
                        reasoning: Some(text),
                        images: vec![],
                    })
                    .await;
            }
            WireEvent::Unknown(ref msg) if msg.starts_with("error") => {
                tracing::warn!("stream_kimi_agent: error event: {}", msg);
                let _ = tx
                    .send(LlmStreamChunk {
                        text: Some(format!("\n\n[kimi-agent error] {}", msg)),
                        tool_calls: vec![],
                        done: false,
                        reasoning: None,
                        images: vec![],
                    })
                    .await;
                break;
            }
            // Skip informational events: TurnBegin, StepBegin,
            // StatusUpdate, ToolCall (built-in), ToolCallPart, ToolResult,
            // CompactionBegin/End, SubagentEvent, Unknown
            _ => {}
        }
    }

    // Send final done chunk with any tool calls
    let _ = tx
        .send(LlmStreamChunk {
            text: None,
            tool_calls,
            done: true,
            reasoning: None,
            images: vec![],
        })
        .await;

    Ok(())
}

/// Stream completion via ACP protocol.
///
/// Manages a persistent ACP subprocess per provider type. Creates a fresh
/// session for each request to avoid context duplication (the WASM agent
/// sends full conversation history). Supports automatic retry with
/// progressively more aggressive context compression on ContextLengthExceeded.
async fn stream_acp_completion(
    _api_key: &str,
    model: &str,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
    _base_url: Option<&str>,
    host_services: Option<crate::wasm::services::HostServices>,
) -> Result<()> {
    let context_limit = nevoflux_llm::default_context_window_for(provider) as usize;

    // For OpenClaw: ensure MCP stdio server is registered in gateway config
    // BEFORE spawning the ACP process. Config changes trigger a gateway restart,
    // so we must do this before connecting to avoid "gateway closed (1012)" errors.
    if matches!(provider, ProviderType::OpenClaw) {
        match crate::openclaw_setup::ensure_openclaw_configured() {
            Ok(true) => {
                tracing::info!("OpenClaw first-time setup completed, waiting for gateway restart");
                // Give the gateway time to restart after config change
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Ok(false) => {} // Already configured, no restart needed
            Err(e) => tracing::warn!("OpenClaw setup failed: {}", e),
        }
    }

    // Get or create ACP provider (lazy init, auto-reconnect on crash).
    let provider_key = format!("{:?}", provider);

    {
        let mut providers = acp_providers().lock().await;

        if !providers.contains_key(&provider_key) || !providers[&provider_key].is_alive() {
            let work_dir = resolve_workspace_dir();
            let config = match provider {
                ProviderType::ClaudeCode => nevoflux_llm::providers::acp::claude::build_config(
                    std::path::PathBuf::from(&work_dir),
                ),
                ProviderType::GeminiCli => nevoflux_llm::providers::acp::gemini::build_config(
                    model,
                    std::path::PathBuf::from(&work_dir),
                ),
                ProviderType::OpenClaw => nevoflux_llm::providers::acp::openclaw::build_config(
                    std::path::PathBuf::from(&work_dir),
                ),
                _ => {
                    return Err(DaemonError::InternalError(format!(
                        "ACP not supported for {:?}",
                        provider
                    )));
                }
            };

            let mut acp = AcpProvider::new(config);
            acp.connect()
                .await
                .map_err(|e| DaemonError::InternalError(format!("Failed to connect ACP: {}", e)))?;
            providers.insert(provider_key.clone(), acp);
        }
    }

    // Detect MCP bridge mode via tool_bridge presence
    let use_mcp_bridge = {
        let providers = acp_providers().lock().await;
        providers
            .get(&provider_key)
            .and_then(|p| p.tool_bridge())
            .is_some()
    };

    // MCP bridge setup
    let _tool_executor_guard = if use_mcp_bridge {
        let providers = acp_providers().lock().await;
        let acp = providers.get(&provider_key).unwrap();
        let tool_bridge = acp.tool_bridge().unwrap().clone();
        drop(providers);

        // Start HTTP MCP server (once, persists across requests)
        if tool_bridge.mcp_server_url().is_none() {
            match crate::wasm::mcp_http_server::start_mcp_http_server(tool_bridge.clone()).await {
                Ok((port, handle)) => {
                    let url = format!("http://127.0.0.1:{port}/mcp");
                    tracing::info!("MCP HTTP server started at {}", url);
                    tool_bridge.set_mcp_server_url(url.clone());
                    tool_bridge.set_server_handle(handle);
                    // Write URL to state file so OpenClaw plugin can discover it
                    if let Err(e) = crate::openclaw_setup::write_mcp_url(&url) {
                        tracing::warn!("Failed to write MCP URL state file: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to start MCP HTTP server: {}", e);
                }
            }
        }

        // Update tool definitions from request
        if let Some(tools) = &request.tools {
            let mcp_tools: Vec<nevoflux_llm::providers::acp::mcp_bridge::McpToolDef> = tools
                .iter()
                .map(|t| nevoflux_llm::providers::acp::mcp_bridge::McpToolDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: t.parameters.clone(),
                })
                .collect();
            tool_bridge.update_tools(mcp_tools);
        }

        // Spawn tool executor task (only if host_services available)
        if let Some(services) = host_services.clone() {
            let (tool_tx, tool_rx) = tokio::sync::mpsc::channel(32);
            tool_bridge.set_executor(tool_tx);
            tokio::spawn(crate::wasm::mcp_tool_executor::run_tool_executor(
                tool_rx,
                services.clone(),
                tool_bridge.clone(),
            ));

            // Spawn permission handler — forwards permission requests to sidebar via browser_ask_user
            let (perm_tx, perm_rx) = tokio::sync::mpsc::channel::<
                nevoflux_llm::providers::acp::mcp_bridge::PermissionRequest,
            >(4);
            tool_bridge.set_permission_handler(perm_tx);
            let is_iteration = services.is_iteration;
            if let Some(browser_ctx) = services.browser_context() {
                tokio::spawn(crate::wasm::mcp_tool_executor::run_permission_handler(
                    perm_rx,
                    browser_ctx,
                    is_iteration,
                ));
            } else if is_iteration {
                // Iteration without browser_ctx: still spawn the handler so
                // it can auto-approve. Without a handler, the bridge would
                // hang on every permission request. The dummy sender is
                // never read because is_iteration=true short-circuits before
                // any browser_ask_user call.
                if let Some(browser_sender) = services.browser_sender.clone() {
                    let dummy_ctx = crate::wasm::services::BrowserContext {
                        sender: browser_sender,
                        client_identity: Vec::new(),
                        proxy_id: String::new(),
                        asset_server: services.asset_server.clone(),
                    };
                    tokio::spawn(crate::wasm::mcp_tool_executor::run_permission_handler(
                        perm_rx,
                        dummy_ctx,
                        true,
                    ));
                }
                // If browser_sender is None too, the bridge won't work in
                // any mode — that's a daemon-startup misconfig, not our
                // problem to handle here.
            }
        } else {
            tracing::warn!(
                "MCP bridge mode but no host_services — MCP tool calls will return 'no active tool executor'"
            );
        }

        Some(tool_bridge.executor_guard())
    } else {
        None
    };

    // OpenClaw manages tools natively via its own MCP system — don't inject
    // tool XML into the prompt (it would bloat context by ~30KB and may exceed
    // the model's context window).
    let skip_tool_xml = use_mcp_bridge || matches!(provider, ProviderType::OpenClaw);

    // Retry with progressive compression on context length errors.
    for level in 0..=2u8 {
        let content = if skip_tool_xml {
            // MCP/OpenClaw mode: system prompt WITHOUT tool XML
            build_acp_content_mcp(
                &request,
                context_limit,
                match level {
                    0 => 0.30,
                    1 => 0.15,
                    _ => 0.0,
                },
            )
        } else {
            match level {
                0 => build_acp_content(&request, context_limit, 0.30),
                1 => build_acp_content(&request, context_limit, 0.15),
                _ => build_acp_content_minimal(&request),
            }
        };

        let providers = acp_providers().lock().await;
        let acp = providers
            .get(&provider_key)
            .ok_or_else(|| DaemonError::InternalError("ACP provider disappeared".to_string()))?;

        let session_id = acp.new_session().await.map_err(|e| {
            DaemonError::InternalError(format!("Failed to create ACP session: {}", e))
        })?;

        let mut response_rx = acp
            .prompt(session_id, content)
            .await
            .map_err(|e| DaemonError::InternalError(format!("Failed to send ACP prompt: {}", e)))?;

        // Drop the lock before doing I/O on the receiver.
        drop(providers);

        // Accumulate all text to extract <tool_call> XML at the end.
        let mut accumulated_text = String::new();
        let mut had_context_error = false;

        while let Some(update) = response_rx.recv().await {
            match update {
                AcpUpdate::Text(text) => {
                    accumulated_text.push_str(&text);
                    // Stream text chunks immediately for real-time display.
                    // Tool call extraction happens at Complete.
                    let _ = tx
                        .send(LlmStreamChunk {
                            text: Some(text),
                            tool_calls: vec![],
                            done: false,
                            reasoning: None,
                            images: vec![],
                        })
                        .await;
                }
                AcpUpdate::Thought(thought) => {
                    let _ = tx
                        .send(LlmStreamChunk {
                            text: None,
                            tool_calls: vec![],
                            done: false,
                            reasoning: Some(thought),
                            images: vec![],
                        })
                        .await;
                }
                AcpUpdate::Complete(_) => {
                    if use_mcp_bridge {
                        // MCP mode: no tool call extraction — tools called natively via MCP.
                        // Send done:true so the forwarder completes and agent returns.
                        // Pending artifacts (from create_artifact) are drained and sent
                        // by server.rs after the agent returns — sidebar handles artifact
                        // messages independently of the streaming done signal.
                        let _ = tx
                            .send(LlmStreamChunk {
                                text: None,
                                tool_calls: vec![],
                                done: true,
                                reasoning: None,
                                images: vec![],
                            })
                            .await;
                    } else {
                        // Direct mode: extract tool calls from the accumulated text.
                        let (cleaned_text, extracted) =
                            extract_tool_calls_from_text(&accumulated_text);

                        let tool_calls: Vec<LlmToolCall> = extracted
                            .into_iter()
                            .map(|tc| LlmToolCall {
                                id: tc.id.clone(),
                                call_id: Some(tc.id),
                                name: tc.name,
                                arguments: tc.arguments,
                                signature: None,
                            })
                            .collect();

                        if !tool_calls.is_empty() {
                            tracing::info!(
                                "ACP: extracted {} tool calls from text",
                                tool_calls.len()
                            );
                            // Send cleaned text (without <tool_call> XML) + tool calls
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: if cleaned_text.is_empty() {
                                        None
                                    } else {
                                        Some(cleaned_text)
                                    },
                                    tool_calls,
                                    done: true,
                                    reasoning: None,
                                    images: vec![],
                                })
                                .await;
                        } else {
                            let _ = tx
                                .send(LlmStreamChunk {
                                    text: None,
                                    tool_calls: vec![],
                                    done: true,
                                    reasoning: None,
                                    images: vec![],
                                })
                                .await;
                        }
                    }
                    return Ok(());
                }
                AcpUpdate::Error(e) => {
                    if e.to_lowercase().contains("context")
                        && e.to_lowercase().contains("length")
                        && level < 2
                    {
                        tracing::warn!(
                            "Context length exceeded at level {}, retrying with level {}",
                            level,
                            level + 1
                        );
                        had_context_error = true;
                        break;
                    }
                    return Err(DaemonError::InternalError(format!("ACP error: {}", e)));
                }
            }
        }

        if !had_context_error {
            return Ok(());
        }
    }

    Ok(())
}

/// Build the system prompt with tool definitions appended.
fn build_acp_system_prompt(request: &LlmChatRequest) -> String {
    let mut prompt = String::new();

    if let Some(system) = &request.system {
        prompt.push_str("IMPORTANT: You MUST follow these system instructions. They override any previous instructions or default behavior.\n\n");
        prompt.push_str(system);
    }

    // Inject tool definitions so the model outputs <tool_call> XML
    if let Some(tools) = &request.tools {
        let tool_defs: Vec<ToolDefinition> = tools.iter().cloned().map(Into::into).collect();
        let tool_prompt = format_tool_definitions_prompt(&tool_defs);
        if !tool_prompt.is_empty() {
            prompt.push_str(&tool_prompt);
        }
    }

    if !prompt.is_empty() {
        prompt.push_str("\n\nRemember: Follow the system instructions above. Do NOT identify yourself as Gemini CLI, Claude Code, or any other agent. You are the assistant described in the instructions above.");
    }

    prompt
}

/// Build ACP content blocks with dynamic token budget compression.
fn build_acp_content(
    request: &LlmChatRequest,
    context_limit: usize,
    budget_ratio: f32,
) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    let system_prompt = build_acp_system_prompt(request);
    if !system_prompt.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(system_prompt)));
    }

    let messages: Vec<(String, String)> = request
        .messages
        .iter()
        .map(|m| {
            let mut content = m.content.clone();
            // Preserve tool call markers so ACP agent can see what tools were used
            if m.role == "assistant" {
                if let Some(ref calls) = m.tool_calls {
                    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
                    if !names.is_empty() {
                        content.push_str(&format!(" [called: {}]", names.join(", ")));
                    }
                }
            }
            (m.role.clone(), content)
        })
        .collect();

    let budget = (context_limit as f32 * budget_ratio) as usize;
    let history = compress_history(&messages, budget, 3);
    if !history.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(history)));
    }

    blocks
}

/// Build minimal ACP content: system prompt + last message only.
fn build_acp_content_minimal(request: &LlmChatRequest) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    let system_prompt = build_acp_system_prompt(request);
    if !system_prompt.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(system_prompt)));
    }

    if let Some(last) = request.messages.last() {
        blocks.push(ContentBlock::Text(TextContent::new(format!(
            "[{}]\n{}",
            last.role, last.content
        ))));
    }

    blocks
}

/// Build content for MCP bridge mode — system prompt WITHOUT tool XML.
fn build_acp_content_mcp(
    request: &LlmChatRequest,
    context_limit: usize,
    budget_ratio: f32,
) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    // System prompt without tool definitions (tools discovered via MCP)
    if let Some(system) = &request.system {
        blocks.push(ContentBlock::Text(TextContent::new(system.clone())));
    }

    // Compressed history (same as direct mode)
    if !request.messages.is_empty() && budget_ratio > 0.0 {
        let budget = (context_limit as f32 * budget_ratio) as usize;
        let messages: Vec<(String, String)> = request
            .messages
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let compressed = compress_history(&messages, budget, 3);
        if !compressed.is_empty() {
            blocks.push(ContentBlock::Text(TextContent::new(compressed)));
        }
    }

    // For minimal mode (budget_ratio == 0.0), add only last message
    if budget_ratio == 0.0 && !request.messages.is_empty() {
        if let Some(last) = request.messages.last() {
            blocks.push(ContentBlock::Text(TextContent::new(format!(
                "[{}] {}",
                last.role, last.content
            ))));
        }
    }

    blocks
}

/// Build a kimi-agent prompt string from an LLM chat request.
/// Format a message's text content together with any image attachments as inline base64.
///
/// Kimi Agent uses a text-based wire protocol and cannot handle multimodal `Image` content
/// natively.  We embed each image attachment as a data-URI so the model can still "see" it.
/// Check if any message has media attachments (image, audio, video).
fn has_media_attachments(request: &LlmChatRequest) -> bool {
    request
        .messages
        .iter()
        .any(|m| m.attachments.iter().any(|a| is_media_attachment(a)))
}

/// Check if an attachment is a supported media type (image, audio, video).
fn is_media_attachment(att: &LlmAttachment) -> bool {
    att.mime_type.starts_with("image/")
        || att.mime_type.starts_with("audio/")
        || att.mime_type.starts_with("video/")
}

/// Build a data URI for a media attachment.
fn attachment_to_data_uri(att: &LlmAttachment) -> String {
    if att.data.starts_with("data:") {
        att.data.clone()
    } else {
        format!("data:{};base64,{}", att.mime_type, att.data)
    }
}

/// Convert a media attachment to a wire protocol ContentPart.
///
/// Maps to `ImageURLPart`, `AudioURLPart`, or `VideoURLPart` per wire protocol v1.4.
fn attachment_to_content_part(att: &LlmAttachment) -> Option<serde_json::Value> {
    let url = attachment_to_data_uri(att);
    if att.mime_type.starts_with("image/") {
        Some(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": url }
        }))
    } else if att.mime_type.starts_with("audio/") {
        Some(serde_json::json!({
            "type": "audio_url",
            "audio_url": { "url": url }
        }))
    } else if att.mime_type.starts_with("video/") {
        Some(serde_json::json!({
            "type": "video_url",
            "video_url": { "url": url }
        }))
    } else {
        None
    }
}

/// Build the kimi-agent prompt as a `serde_json::Value`.
///
/// Returns either a plain string (when no media) or a `ContentPart[]` array
/// (when image/audio/video attachments are present), matching wire protocol v1.4.
fn build_kimi_prompt(request: &LlmChatRequest) -> serde_json::Value {
    let mut text_parts = Vec::new();

    if let Some(ref sys) = request.system {
        if !sys.is_empty() {
            text_parts.push(format!("<system>\n{}\n</system>", sys));
        }
    }

    let has_assistant = request.messages.iter().any(|m| m.role == "assistant");

    if has_assistant {
        text_parts.push("<conversation_history>".to_string());
        for msg in &request.messages {
            if msg.role == "system" {
                continue;
            }
            if msg.role == "tool" {
                // Wrap tool results in XML tags so the model treats them as internal data
                // rather than conversational text to echo back to the user.
                text_parts.push(format!("<tool_result>\n{}\n</tool_result>", msg.content));
            } else {
                text_parts.push(format!("[{}]: {}", msg.role, msg.content));
            }
        }
        text_parts.push("</conversation_history>".to_string());
        text_parts.push("Continue the conversation based on the history above. Do not repeat or quote raw tool results or JSON in your response to the user.".to_string());
    } else {
        for msg in &request.messages {
            if msg.role != "system" && !msg.content.is_empty() {
                text_parts.push(msg.content.clone());
            }
        }
    }

    let prompt_text = text_parts.join("\n\n");

    // If there are media attachments, use ContentPart[] format
    if has_media_attachments(request) {
        let mut content_parts: Vec<serde_json::Value> = Vec::new();

        // Add text part
        if !prompt_text.is_empty() {
            content_parts.push(serde_json::json!({
                "type": "text",
                "text": prompt_text
            }));
        }

        // Add media parts (image/audio/video) from all messages
        for msg in &request.messages {
            for att in &msg.attachments {
                if let Some(part) = attachment_to_content_part(att) {
                    content_parts.push(part);
                }
            }
        }

        serde_json::Value::Array(content_parts)
    } else {
        serde_json::Value::String(prompt_text)
    }
}

/// Generic streaming completion for any rig-compatible model.
async fn stream_rig_completion<M>(
    completion_model: M,
    request: LlmChatRequest,
    tx: mpsc::Sender<LlmStreamChunk>,
    provider: ProviderType,
) -> Result<()>
where
    M: CompletionModel,
    M::StreamingResponse: Clone + Unpin + rig::completion::GetTokenUsage,
{
    tracing::info!(
        "=== stream_rig_completion ENTRY === message_count={}",
        request.messages.len()
    );

    // Convert request messages to rig messages (same as non-streaming)
    let mut chat_history: Vec<Message> = Vec::new();
    let mut system_prompt: Option<String> = request.system.clone();

    for (idx, msg) in request.messages.iter().enumerate() {
        tracing::info!(
            "Processing message[{}]: role={}, content_len={}, attachments_count={}, tool_calls_count={:?}, tool_call_id={:?}",
            idx,
            msg.role,
            msg.content.len(),
            msg.attachments.len(),
            msg.tool_calls.as_ref().map(|tc| tc.len()),
            msg.tool_call_id
        );
        match msg.role.as_str() {
            "system" => {
                system_prompt = Some(msg.content.clone());
            }
            "tool" => {
                let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
                tracing::info!(
                    "Converting tool result (stream): tool_call_id={}, content_len={}, attachments={}, starts_with_call={}, starts_with_fc={}",
                    tool_call_id,
                    msg.content.len(),
                    msg.attachments.len(),
                    tool_call_id.starts_with("call_"),
                    tool_call_id.starts_with("fc_")
                );

                // Check for image attachments first (from agent-level screenshot handling)
                let image_attachment = msg
                    .attachments
                    .iter()
                    .find(|a| a.mime_type.starts_with("image/"));

                match image_attachment {
                    Some(attachment) if provider == ProviderType::Anthropic => {
                        // Anthropic: image inside tool result
                        let clean_b64 = clean_base64_data(&attachment.data);
                        let detected_media_type =
                            detect_image_media_type_from_base64(&clean_b64, &attachment.mime_type);
                        let tool_result = ToolResult {
                            id: tool_call_id.clone(),
                            call_id: Some(tool_call_id),
                            content: OneOrMany::many(vec![
                                ToolResultContent::text(&msg.content),
                                ToolResultContent::image_base64(
                                    &clean_b64,
                                    Some(detected_media_type),
                                    None,
                                ),
                            ])
                            .unwrap_or_else(|_| {
                                OneOrMany::one(ToolResultContent::text(&msg.content))
                            }),
                        };
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                        });
                    }
                    Some(attachment) => {
                        // OpenAI/others: text tool result + separate user image message
                        let clean_b64 = clean_base64_data(&attachment.data);
                        let detected_media_type =
                            detect_image_media_type_from_base64(&clean_b64, &attachment.mime_type);
                        let tool_result = ToolResult {
                            id: tool_call_id.clone(),
                            call_id: Some(tool_call_id),
                            content: OneOrMany::one(ToolResultContent::text(&msg.content)),
                        };
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                        });
                        chat_history.push(Message::User {
                            content: OneOrMany::one(UserContent::Image(Image {
                                data: DocumentSourceKind::Base64(clean_b64),
                                media_type: Some(detected_media_type),
                                detail: Some(ImageDetail::Auto),
                                additional_params: None,
                            })),
                        });
                    }
                    None => {
                        // No attachment — fallback to extract_screenshot_from_tool_result
                        match extract_screenshot_from_tool_result(&msg.content) {
                            Some(screenshot) if provider == ProviderType::Anthropic => {
                                let detected_media_type = detect_image_media_type_from_base64(
                                    &screenshot.base64_data,
                                    "image/png",
                                );
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::many(vec![
                                        ToolResultContent::text(
                                            &screenshot.text_without_screenshot,
                                        ),
                                        ToolResultContent::image_base64(
                                            &screenshot.base64_data,
                                            Some(detected_media_type),
                                            None,
                                        ),
                                    ])
                                    .unwrap_or_else(|_| {
                                        OneOrMany::one(ToolResultContent::text(&msg.content))
                                    }),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                            }
                            Some(screenshot) => {
                                let detected_media_type = detect_image_media_type_from_base64(
                                    &screenshot.base64_data,
                                    "image/png",
                                );
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::one(ToolResultContent::text(
                                        &screenshot.text_without_screenshot,
                                    )),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::Image(Image {
                                        data: DocumentSourceKind::Base64(screenshot.base64_data),
                                        media_type: Some(detected_media_type),
                                        detail: Some(ImageDetail::Auto),
                                        additional_params: None,
                                    })),
                                });
                            }
                            None => {
                                let tool_result = ToolResult {
                                    id: tool_call_id.clone(),
                                    call_id: Some(tool_call_id),
                                    content: OneOrMany::one(ToolResultContent::Text(Text {
                                        text: msg.content.clone(),
                                    })),
                                };
                                chat_history.push(Message::User {
                                    content: OneOrMany::one(UserContent::ToolResult(tool_result)),
                                });
                            }
                        }
                    }
                }
            }
            "assistant" => {
                tracing::info!(
                    "Converting assistant message: content_len={}, tool_calls_present={}, tool_calls_count={:?}, has_reasoning={}",
                    msg.content.len(),
                    msg.tool_calls.is_some(),
                    msg.tool_calls.as_ref().map(|tc| tc.len()),
                    msg.reasoning.is_some()
                );
                if let Some(ref tool_calls) = msg.tool_calls {
                    tracing::info!(
                        "Converting assistant tool_calls: count={}, ids={:?}, call_ids={:?}",
                        tool_calls.len(),
                        tool_calls.iter().map(|tc| &tc.id).collect::<Vec<_>>(),
                        tool_calls.iter().map(|tc| &tc.call_id).collect::<Vec<_>>()
                    );
                }
                chat_history.push(build_rig_assistant_message(msg, provider));
            }
            _ => {
                let mut user_content: Vec<UserContent> = Vec::new();
                if !msg.content.is_empty() {
                    user_content.push(UserContent::text(&msg.content));
                }
                for attachment in &msg.attachments {
                    tracing::info!(
                        "Processing attachment (stream): name={}, mime_type={}, data_len={}",
                        attachment.name,
                        attachment.mime_type,
                        attachment.data.len()
                    );

                    // Warn about large images that may cause intermittent failures with some models
                    const LARGE_IMAGE_THRESHOLD: usize = 200_000; // ~200KB base64
                    if attachment.data.len() > LARGE_IMAGE_THRESHOLD {
                        tracing::warn!(
                            "Large image attachment detected (stream): {} ({} bytes). Some models (e.g., gpt-4o-mini) \
                            may have intermittent issues with large images. Consider using gpt-4o for better reliability.",
                            attachment.name,
                            attachment.data.len()
                        );
                    }

                    if mime_to_image_media_type(&attachment.mime_type).is_some() {
                        // Strip data URL prefix if present (e.g., "data:image/png;base64,")
                        let base64_data = if attachment.data.starts_with("data:") {
                            attachment
                                .data
                                .find(",")
                                .map(|i| &attachment.data[i + 1..])
                                .unwrap_or(&attachment.data)
                        } else {
                            &attachment.data
                        };
                        // Remove any whitespace/newlines from base64 (some encoders add line breaks)
                        let clean_base64: String =
                            base64_data.chars().filter(|c| !c.is_whitespace()).collect();
                        tracing::info!(
                            "Using cleaned base64 data (stream): original_len={}, clean_len={}",
                            base64_data.len(),
                            clean_base64.len()
                        );
                        let detected_media_type = detect_image_media_type_from_base64(
                            &clean_base64,
                            &attachment.mime_type,
                        );
                        user_content.push(UserContent::Image(Image {
                            data: DocumentSourceKind::Base64(clean_base64),
                            media_type: Some(detected_media_type),
                            detail: Some(ImageDetail::Auto),
                            additional_params: None,
                        }));
                    } else {
                        tracing::warn!(
                            "Unsupported attachment mime_type: {}",
                            attachment.mime_type
                        );
                    }
                }
                if !user_content.is_empty() {
                    chat_history.push(Message::User {
                        content: OneOrMany::many(user_content)
                            .unwrap_or_else(|_| OneOrMany::one(UserContent::text(" "))),
                    });
                }
            }
        }
    }

    // Merge consecutive same-role messages to ensure strict user/assistant alternation
    let chat_history = merge_consecutive_same_role_messages(chat_history);

    // Check if we have any messages
    if chat_history.is_empty() {
        return Err(DaemonError::InternalError(
            "LLM stream requires at least one user message".into(),
        ));
    }

    // Debug: Log message structure before processing
    tracing::info!("=== LLM STREAM MESSAGE STRUCTURE (before prompt extraction) ===");
    for (i, msg) in chat_history.iter().enumerate() {
        match msg {
            Message::User { content } => {
                let content_types: Vec<&str> = content
                    .iter()
                    .map(|c| match c {
                        UserContent::Text(_) => "Text",
                        UserContent::Image(_) => "Image",
                        UserContent::Document(_) => "Document",
                        UserContent::Audio(_) => "Audio",
                        UserContent::Video(_) => "Video",
                        UserContent::ToolResult(tr) => {
                            tracing::info!(
                                "  Message[{}] User/ToolResult: id={}, call_id={:?}",
                                i,
                                tr.id,
                                tr.call_id
                            );
                            "ToolResult"
                        }
                    })
                    .collect();
                tracing::info!("Message[{}]: User({:?})", i, content_types);
            }
            Message::Assistant { content, .. } => {
                let content_types: Vec<String> = content
                    .iter()
                    .map(|c| match c {
                        AssistantContent::Text(_) => "Text".to_string(),
                        AssistantContent::ToolCall(tc) => {
                            tracing::info!(
                                "  Message[{}] Assistant/ToolCall: id={}, call_id={:?}, name={}",
                                i,
                                tc.id,
                                tc.call_id,
                                tc.function.name
                            );
                            format!("ToolCall(id={},call_id={:?})", tc.id, tc.call_id)
                        }
                        AssistantContent::Reasoning(_) => "Reasoning".to_string(),
                        AssistantContent::Image(_) => "Image".to_string(),
                    })
                    .collect();
                tracing::info!("Message[{}]: Assistant({:?})", i, content_types);
            }
        }
    }
    tracing::info!("=== END MESSAGE STRUCTURE ===");

    // Check if this is a continuation after tool calls (has tool result messages)
    let has_tool_results = chat_history
        .iter()
        .any(|m| matches!(m, Message::User { content } if content.iter().any(|c| matches!(c, UserContent::ToolResult(_)))));

    // Build the completion request
    // IMPORTANT: rig's builder appends prompt to the END of chat_history.
    // We must extract the last user message and pass it directly as the prompt Message,
    // so rig places it at the end. Using an empty string prompt would create an extra
    // empty User message, causing the LLM to lose focus on multimodal content (images).
    tracing::info!(
        "has_tool_results={}, message_count={}",
        has_tool_results,
        chat_history.len()
    );

    let (prompt_message, chat_history) = {
        let last_user_idx = chat_history
            .iter()
            .rposition(|m| matches!(m, Message::User { .. }));
        if let Some(idx) = last_user_idx {
            let has_multimodal = match &chat_history[idx] {
                Message::User { content } => content
                    .iter()
                    .any(|c| matches!(c, UserContent::Image(_) | UserContent::Document(_))),
                _ => false,
            };
            if has_multimodal && idx > 0 {
                // Multi-turn multimodal: keep all messages in chat_history,
                // use text from last user message as prompt. Some API providers
                // return empty responses when a multimodal message is passed
                // directly as the prompt in multi-turn conversations.
                let text = match &chat_history[idx] {
                    Message::User { content } => content
                        .iter()
                        .filter_map(|c| match c {
                            UserContent::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                tracing::info!(
                    "Multi-turn multimodal: keeping message[{}] in history, prompt text_len={}",
                    idx,
                    text.len()
                );
                (Message::from(text), chat_history)
            } else {
                tracing::info!("Extracted prompt message from message[{}]", idx);
                let mut history = chat_history;
                let msg = history.remove(idx);
                (msg, history)
            }
        } else {
            tracing::info!("No user message found, using empty prompt");
            (Message::from(""), chat_history)
        }
    };

    tracing::info!("Final: chat_history_len={}", chat_history.len());

    let mut builder = completion_model.completion_request(prompt_message);

    if let Some(preamble) = system_prompt {
        builder = builder.preamble(preamble);
    }

    builder = builder.messages(chat_history);

    if let Some(tools) = request.tools {
        let rig_tools: Vec<ToolDefinition> = tools.into_iter().map(|t| t.into()).collect();
        builder = builder.tools(rig_tools);
    }

    // Anthropic's REST API mandates `max_tokens` on every request — without
    // it rig-core fails with `RequestError: max_tokens must be set for
    // Anthropic` before any HTTP traffic happens. Other providers tolerate
    // an unset value (they fall back to their own model-default cap), but
    // we always pass through whatever the caller put on the request. When
    // the caller didn't set anything, fall back to a generic 4096 cap that
    // covers chat-style turns without truncating. The agent path
    // (agent_host.rs) already populates this from config; the fallback
    // here is for synthetic / test paths that build LlmChatRequest by
    // hand.
    let resolved_max_tokens: u64 = request
        .max_tokens
        .map(|n| n as u64)
        .unwrap_or(DEFAULT_MAX_TOKENS_FALLBACK);
    builder = builder.max_tokens(resolved_max_tokens);

    // Execute streaming request
    let mut stream_response = builder
        .stream()
        .await
        .map_err(|e| DaemonError::InternalError(format!("LLM stream failed: {}", e)))?;

    // Process stream chunks
    // Track tool calls that were already sent (complete tool calls)
    let mut sent_tool_call_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    // Track tool calls being built from deltas (not yet sent)
    let mut accumulated_tool_calls: HashMap<String, LlmToolCall> = HashMap::new();
    // Diagnostics: track bytes and chunks sent
    let mut total_text_bytes: usize = 0;
    let mut total_text_chunks: usize = 0;
    let mut receiver_dropped = false;

    // First chunk timeout: 5 minutes (provider queue delays, cold starts).
    // Inter-chunk timeout: 2 minutes (healthy streams shouldn't gap longer).
    let first_chunk_timeout = std::time::Duration::from_secs(300);
    let inter_chunk_timeout = std::time::Duration::from_secs(120);
    let mut got_first_chunk = false;

    loop {
        let timeout_dur = if got_first_chunk {
            inter_chunk_timeout
        } else {
            first_chunk_timeout
        };
        let chunk_result = match tokio::time::timeout(timeout_dur, stream_response.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break, // stream ended
            Err(_) => {
                tracing::warn!(
                    "LLM stream timeout after {:?} (got_first_chunk={})",
                    timeout_dur,
                    got_first_chunk
                );
                let _ = tx
                    .send(LlmStreamChunk {
                        text: Some(format!(
                            "\n\n[error] LLM provider timed out after {} seconds with no {}.",
                            timeout_dur.as_secs(),
                            if got_first_chunk {
                                "new data"
                            } else {
                                "response"
                            }
                        )),
                        tool_calls: vec![],
                        done: false,
                        reasoning: None,
                        images: vec![],
                    })
                    .await;
                break;
            }
        };
        match chunk_result {
            Ok(choice) => {
                let chunk = match choice {
                    StreamedAssistantContent::Text(text) => {
                        let len = text.text.len();
                        total_text_bytes += len;
                        total_text_chunks += 1;
                        if total_text_chunks <= 3 || total_text_chunks % 50 == 0 {
                            tracing::info!(
                                "Stream chunk #{}: Text({} bytes), total={} bytes",
                                total_text_chunks,
                                len,
                                total_text_bytes
                            );
                        }
                        LlmStreamChunk {
                            text: Some(text.text),
                            tool_calls: vec![],
                            done: false,
                            reasoning: None,
                            images: vec![],
                        }
                    }
                    StreamedAssistantContent::ToolCall(tc) => {
                        tracing::info!(
                            "Stream chunk: COMPLETE ToolCall received - id={}, call_id={:?}, name={}, complete_args_type={:?}",
                            tc.id,
                            tc.call_id,
                            tc.function.name,
                            match &tc.function.arguments {
                                serde_json::Value::String(s) => format!("String(len={})", s.len()),
                                serde_json::Value::Object(o) => format!("Object(keys={})", o.len()),
                                other => format!("{:?}", other),
                            }
                        );
                        // Defer tool call to final chunk processing. Update accumulated entry
                        // with metadata and COMPLETE arguments (which may be more complete
                        // than delta-accumulated content for some providers).
                        let entry =
                            accumulated_tool_calls
                                .entry(tc.id.clone())
                                .or_insert_with(|| LlmToolCall {
                                    id: tc.id.clone(),
                                    call_id: None,
                                    name: String::new(),
                                    arguments: serde_json::Value::String(String::new()),
                                    signature: None,
                                });
                        if entry.name.is_empty() {
                            entry.name = tc.function.name.clone();
                        }
                        entry.call_id = tc.call_id.clone().or(entry.call_id.take());
                        entry.signature = tc.signature.clone().or(entry.signature.take());
                        // Keep the more complete arguments: compare COMPLETE's args with accumulated delta.
                        // Some providers send full args in COMPLETE, others only in deltas.
                        let complete_args_len = match &tc.function.arguments {
                            serde_json::Value::String(s) => s.len(),
                            serde_json::Value::Object(o) if !o.is_empty() => {
                                tc.function.arguments.to_string().len()
                            }
                            _ => 0,
                        };
                        let accumulated_args_len = match &entry.arguments {
                            serde_json::Value::String(s) => s.len(),
                            _ => 0,
                        };
                        if complete_args_len > 0 && complete_args_len >= accumulated_args_len {
                            tracing::info!(
                                "Using COMPLETE args (len={}) over accumulated (len={}) for {}",
                                complete_args_len,
                                accumulated_args_len,
                                tc.id
                            );
                            entry.arguments = tc.function.arguments.clone();
                        }
                        continue; // Don't emit a chunk — tool call will be sent at stream end
                    }
                    StreamedAssistantContent::ToolCallDelta { id, content } => {
                        tracing::debug!("Stream chunk: ToolCallDelta(id={})", id);
                        // Handle tool call deltas (accumulate for later sending)
                        let tc = accumulated_tool_calls.entry(id.clone()).or_insert_with(|| {
                            LlmToolCall {
                                id: id.clone(),
                                call_id: None,
                                name: String::new(),
                                arguments: serde_json::Value::String(String::new()),
                                signature: None,
                            }
                        });
                        match content {
                            ToolCallDeltaContent::Name(name) => {
                                tc.name = name;
                            }
                            ToolCallDeltaContent::Delta(delta) => {
                                // Append to arguments string representation
                                if let Some(s) = tc.arguments.as_str() {
                                    tc.arguments =
                                        serde_json::Value::String(format!("{}{}", s, delta));
                                }
                            }
                        }
                        continue; // Don't send delta chunks
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        tracing::debug!("Stream chunk: Reasoning");
                        LlmStreamChunk {
                            text: None,
                            tool_calls: vec![],
                            done: false,
                            reasoning: Some(reasoning.reasoning.join(" ")),
                            images: vec![],
                        }
                    }
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                        tracing::debug!("Stream chunk: ReasoningDelta({})", reasoning);
                        LlmStreamChunk {
                            text: None,
                            tool_calls: vec![],
                            done: false,
                            reasoning: Some(reasoning),
                            images: vec![],
                        }
                    }
                    StreamedAssistantContent::Final(_) => {
                        tracing::info!(
                            "Stream chunk: Final (text_so_far={} bytes, {} chunks)",
                            total_text_bytes,
                            total_text_chunks
                        );
                        continue; // Final is a summary; content was already streamed as Text/ToolCall chunks
                    }
                };

                if tx.send(chunk).await.is_err() {
                    tracing::warn!(
                        "Stream receiver dropped after {} text chunks ({} bytes). Stopping.",
                        total_text_chunks,
                        total_text_bytes
                    );
                    receiver_dropped = true;
                    break;
                }
                got_first_chunk = true;
            }
            Err(e) => {
                tracing::error!("Stream chunk error: {}", e);
                // Send the error as text so the WASM agent can display it
                let error_text = format!("\n[Error: {}]", e);
                let _ = tx
                    .send(LlmStreamChunk {
                        text: Some(error_text),
                        tool_calls: vec![],
                        done: false,
                        reasoning: None,
                        images: vec![],
                    })
                    .await;
                break;
            }
        }
    }

    // Send final chunk with only tool calls that were built from deltas (not already sent).
    // Delta-accumulated arguments are Value::String (raw JSON text); parse them into
    // Value::Object so that downstream tool dispatch can index fields directly.
    let final_tool_calls: Vec<LlmToolCall> = accumulated_tool_calls
        .into_values()
        .filter(|tc| !sent_tool_call_ids.contains(&tc.id))
        .map(|mut tc| {
            if let serde_json::Value::String(ref s) = tc.arguments {
                tc.arguments = parse_tool_arguments_json(s);
            }
            tc
        })
        .collect();

    if !final_tool_calls.is_empty() {
        tracing::info!(
            "Sending {} accumulated tool calls in final chunk: {:?}",
            final_tool_calls.len(),
            final_tool_calls
                .iter()
                .map(|tc| (&tc.id, &tc.call_id, &tc.name))
                .collect::<Vec<_>>()
        );
    }
    tracing::info!(
        "Stream complete. text_chunks={}, text_bytes={}, sent_tool_call_ids={:?}, final_tool_calls_count={}, receiver_dropped={}",
        total_text_chunks,
        total_text_bytes,
        sent_tool_call_ids,
        final_tool_calls.len(),
        receiver_dropped
    );

    let _ = tx
        .send(LlmStreamChunk {
            text: None,
            tool_calls: final_tool_calls,
            done: true,
            reasoning: None,
            images: vec![],
        })
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_message_serialization() {
        let message = LlmMessage::user("Hello, world!");

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello, world!"));

        let parsed: LlmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "user");
        assert_eq!(parsed.content, "Hello, world!");
    }

    #[test]
    fn test_llm_request_serialization() {
        let request = LlmChatRequest {
            messages: vec![LlmMessage::user("Hello")],
            system: Some("You are helpful.".into()),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("You are helpful."));
        assert!(json.contains("0.7"));
        assert!(json.contains("1000"));

        let parsed: LlmChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.system, Some("You are helpful.".into()));
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.max_tokens, Some(1000));
    }

    #[test]
    fn test_llm_request_minimal() {
        let request = LlmChatRequest {
            messages: vec![LlmMessage::user("Hi")],
            ..Default::default()
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: LlmChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert!(parsed.system.is_none());
        assert!(parsed.temperature.is_none());
        assert!(parsed.max_tokens.is_none());
    }

    #[test]
    fn test_llm_response_serialization() {
        let response = LlmChatResponse {
            content: "Hi there!".into(),
            finish_reason: "stop".into(),
            tool_calls: None,
            usage: Some(LlmUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
            images: vec![],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Hi there!"));
        assert!(json.contains("stop"));
        assert!(json.contains("15"));

        let parsed: LlmChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "Hi there!");
        assert_eq!(parsed.finish_reason, "stop");
        assert!(parsed.usage.is_some());
        let usage = parsed.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_llm_response_without_usage() {
        let response = LlmChatResponse {
            content: "Hello!".into(),
            finish_reason: "stop".into(),
            ..Default::default()
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: LlmChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "Hello!");
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn test_llm_usage_serialization() {
        let usage = LlmUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let json = serde_json::to_string(&usage).unwrap();
        let parsed: LlmUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.prompt_tokens, 100);
        assert_eq!(parsed.completion_tokens, 50);
        assert_eq!(parsed.total_tokens, 150);
    }

    #[test]
    fn test_llm_message_clone() {
        let message = LlmMessage::assistant("Test");
        let cloned = message.clone();
        assert_eq!(cloned.role, message.role);
        assert_eq!(cloned.content, message.content);
    }

    #[test]
    fn test_llm_request_clone() {
        let request = LlmChatRequest {
            messages: vec![LlmMessage::user("Test")],
            system: Some("System".into()),
            temperature: Some(0.5),
            max_tokens: Some(500),
            tools: None,
        };
        let cloned = request.clone();
        assert_eq!(cloned.messages.len(), 1);
        assert_eq!(cloned.system, Some("System".into()));
    }

    #[test]
    fn test_llm_response_clone() {
        let response = LlmChatResponse {
            content: "Response".into(),
            finish_reason: "stop".into(),
            ..Default::default()
        };
        let cloned = response.clone();
        assert_eq!(cloned.content, "Response");
    }

    #[test]
    fn test_llm_usage_clone() {
        let usage = LlmUsage {
            prompt_tokens: 1,
            completion_tokens: 2,
            total_tokens: 3,
        };
        let cloned = usage.clone();
        assert_eq!(cloned.total_tokens, 3);
    }

    #[test]
    fn test_llm_message_debug() {
        let message = LlmMessage::user("Debug test");
        let debug = format!("{:?}", message);
        assert!(debug.contains("LlmMessage"));
        assert!(debug.contains("user"));
    }

    #[test]
    fn test_llm_request_debug() {
        let request = LlmChatRequest::default();
        let debug = format!("{:?}", request);
        assert!(debug.contains("LlmChatRequest"));
    }

    #[test]
    fn test_llm_response_debug() {
        let response = LlmChatResponse {
            content: "Test".into(),
            finish_reason: "stop".into(),
            ..Default::default()
        };
        let debug = format!("{:?}", response);
        assert!(debug.contains("LlmChatResponse"));
    }

    #[test]
    fn test_llm_usage_debug() {
        let usage = LlmUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        };
        let debug = format!("{:?}", usage);
        assert!(debug.contains("LlmUsage"));
    }

    #[test]
    fn test_llm_request_multiple_messages() {
        let request = LlmChatRequest {
            messages: vec![
                LlmMessage::user("Hello"),
                LlmMessage::assistant("Hi!"),
                LlmMessage::user("How are you?"),
            ],
            ..Default::default()
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: LlmChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages.len(), 3);
        assert_eq!(parsed.messages[0].role, "user");
        assert_eq!(parsed.messages[1].role, "assistant");
        assert_eq!(parsed.messages[2].role, "user");
    }

    #[test]
    fn test_llm_request_from_json_string() {
        let json = r#"{
            "messages": [
                {"role": "user", "content": "What is 2+2?"}
            ],
            "system": "You are a math tutor.",
            "temperature": 0.0,
            "max_tokens": 100
        }"#;

        let request: LlmChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].content, "What is 2+2?");
        assert_eq!(request.system, Some("You are a math tutor.".into()));
        assert_eq!(request.temperature, Some(0.0));
        assert_eq!(request.max_tokens, Some(100));
    }

    #[test]
    fn test_llm_response_to_json_string() {
        let response = LlmChatResponse {
            content: "The answer is 4.".into(),
            finish_reason: "stop".into(),
            tool_calls: None,
            usage: Some(LlmUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
                total_tokens: 30,
            }),
            images: vec![],
        };

        let json = serde_json::to_string_pretty(&response).unwrap();
        assert!(json.contains("The answer is 4."));
        assert!(json.contains("\"total_tokens\": 30"));
    }

    #[test]
    fn test_llm_tool_definition() {
        let tool = LlmToolDefinition {
            name: "get_weather".into(),
            description: "Get the current weather".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                },
                "required": ["location"]
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("get_weather"));
        assert!(json.contains("location"));

        // Test conversion to rig::ToolDefinition
        let rig_tool: ToolDefinition = tool.into();
        assert_eq!(rig_tool.name, "get_weather");
    }

    #[test]
    fn test_llm_tool_call() {
        let tool_call = LlmToolCall {
            id: "call_123".into(),
            call_id: Some("call_123".into()),
            name: "get_weather".into(),
            arguments: serde_json::json!({"location": "Tokyo"}),
            signature: None,
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("call_123"));
        assert!(json.contains("get_weather"));
        assert!(json.contains("Tokyo"));
    }

    #[test]
    fn test_llm_message_with_tool_calls() {
        let tool_call = LlmToolCall {
            id: "call_abc".into(),
            call_id: Some("call_abc".into()),
            name: "search".into(),
            arguments: serde_json::json!({"query": "rust"}),
            signature: None,
        };
        let message = LlmMessage::assistant_with_tool_calls(vec![tool_call]);

        assert_eq!(message.role, "assistant");
        assert!(message.content.is_empty());
        assert!(message.tool_calls.is_some());
        assert_eq!(message.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_llm_message_tool_result() {
        let message =
            LlmMessage::tool_result("call_abc", "Search result: Rust is a programming language");

        assert_eq!(message.role, "tool");
        assert!(message.content.contains("Rust"));
        assert_eq!(message.tool_call_id, Some("call_abc".into()));
    }

    #[test]
    fn test_llm_response_with_tool_calls() {
        let response = LlmChatResponse {
            content: String::new(),
            finish_reason: "tool_calls".into(),
            tool_calls: Some(vec![LlmToolCall {
                id: "call_xyz".into(),
                call_id: Some("call_xyz".into()),
                name: "calculator".into(),
                arguments: serde_json::json!({"expression": "2+2"}),
                signature: None,
            }]),
            usage: None,
            images: vec![],
        };

        assert_eq!(response.finish_reason, "tool_calls");
        assert!(response.tool_calls.is_some());
        let calls = response.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "calculator");
    }

    #[test]
    fn test_llm_request_with_tools() {
        let request = LlmChatRequest {
            messages: vec![LlmMessage::user("What's the weather?")],
            tools: Some(vec![LlmToolDefinition {
                name: "get_weather".into(),
                description: "Get weather info".into(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            ..Default::default()
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("get_weather"));
        assert!(json.contains("Get weather info"));
    }

    // =========================================================================
    // Streaming Tests
    // =========================================================================

    #[test]
    fn test_stream_chunk_serialization() {
        let chunk = LlmStreamChunk {
            text: Some("Hello".into()),
            tool_calls: vec![],
            done: false,
            reasoning: None,
            images: vec![],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("false"));

        let parsed: LlmStreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, Some("Hello".into()));
        assert!(!parsed.done);
    }

    #[test]
    fn test_stream_chunk_with_tool_calls() {
        let chunk = LlmStreamChunk {
            text: None,
            tool_calls: vec![LlmToolCall {
                id: "call_123".into(),
                call_id: Some("call_123".into()),
                name: "search".into(),
                arguments: serde_json::json!({"query": "rust"}),
                signature: None,
            }],
            done: false,
            reasoning: None,
            images: vec![],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("call_123"));
        assert!(json.contains("search"));
    }

    #[test]
    fn test_stream_chunk_done() {
        let chunk = LlmStreamChunk {
            text: None,
            tool_calls: vec![],
            done: true,
            reasoning: None,
            images: vec![],
        };

        assert!(chunk.done);
        assert!(chunk.text.is_none());
    }

    #[test]
    fn test_stream_registry_allocate_id() {
        let registry = LlmStreamRegistry::new();
        let id1 = registry.allocate_id();
        let id2 = registry.allocate_id();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_stream_registry_close_nonexistent() {
        let registry = LlmStreamRegistry::new();
        // Should not panic
        registry.close(999);
    }

    #[tokio::test]
    async fn test_stream_registry_register_and_close() {
        let registry = LlmStreamRegistry::new();
        let (tx, rx) = mpsc::channel(16);
        let id = registry.allocate_id();

        registry.register(id, rx);

        // Send a chunk
        tx.send(LlmStreamChunk {
            text: Some("Hello".into()),
            tool_calls: vec![],
            done: false,
            reasoning: None,
            images: vec![],
        })
        .await
        .unwrap();

        // Get chunk
        let chunk = registry.next_chunk(id).unwrap();
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().text, Some("Hello".into()));

        // Close
        registry.close(id);

        // Should fail to get next chunk
        let result = registry.next_chunk(id);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stream_registry_done_flag() {
        let registry = LlmStreamRegistry::new();
        let (tx, rx) = mpsc::channel(16);
        let id = registry.allocate_id();

        registry.register(id, rx);

        // Send done chunk
        tx.send(LlmStreamChunk {
            text: None,
            tool_calls: vec![],
            done: true,
            reasoning: None,
            images: vec![],
        })
        .await
        .unwrap();

        // Get chunk - should mark stream as done
        let chunk = registry.next_chunk(id).unwrap();
        assert!(chunk.is_some());
        assert!(chunk.unwrap().done);

        // Next call should return None (stream is done)
        let chunk = registry.next_chunk(id).unwrap();
        assert!(chunk.is_none());
    }

    // =========================================================================
    // Screenshot Extraction Tests
    // =========================================================================

    #[test]
    fn test_extract_screenshot_with_screenshot() {
        let json = r#"{"success":true,"data":"some data","screenshot":"aW1hZ2VfZGF0YQ=="}"#;
        let result = extract_screenshot_from_tool_result(json);
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert_eq!(extracted.base64_data, "aW1hZ2VfZGF0YQ==");
        // The remaining JSON should not contain "screenshot"
        assert!(!extracted.text_without_screenshot.contains("screenshot"));
        assert!(extracted.text_without_screenshot.contains("success"));
        assert!(extracted.text_without_screenshot.contains("some data"));
    }

    #[test]
    fn test_extract_screenshot_without_screenshot() {
        let json = r#"{"success":true,"data":"some data"}"#;
        let result = extract_screenshot_from_tool_result(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_screenshot_empty_screenshot() {
        let json = r#"{"success":true,"screenshot":""}"#;
        let result = extract_screenshot_from_tool_result(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_screenshot_non_json() {
        let text = "This is just plain text, not JSON";
        let result = extract_screenshot_from_tool_result(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_screenshot_data_url_prefix() {
        let json = r#"{"success":true,"screenshot":"data:image/png;base64,aW1hZ2VfZGF0YQ=="}"#;
        let result = extract_screenshot_from_tool_result(json);
        assert!(result.is_some());
        let extracted = result.unwrap();
        // data URL prefix should be stripped
        assert_eq!(extracted.base64_data, "aW1hZ2VfZGF0YQ==");
        assert!(!extracted.base64_data.starts_with("data:"));
    }

    #[test]
    fn test_extract_screenshot_preserves_other_fields() {
        let json = r#"{"success":true,"data":{"url":"https://example.com","title":"Test"},"error":null,"screenshot":"c2NyZWVuc2hvdA=="}"#;
        let result = extract_screenshot_from_tool_result(json);
        assert!(result.is_some());
        let extracted = result.unwrap();
        // Verify other fields are preserved
        let remaining: serde_json::Value =
            serde_json::from_str(&extracted.text_without_screenshot).unwrap();
        assert_eq!(remaining["success"], true);
        assert_eq!(remaining["data"]["url"], "https://example.com");
        assert_eq!(remaining["data"]["title"], "Test");
        assert!(remaining["error"].is_null());
        // screenshot should be gone
        assert!(remaining.get("screenshot").is_none());
    }

    #[test]
    fn test_tool_message_with_image_attachment_serialization() {
        // Verify that LlmMessage with tool role and attachments serializes correctly
        let msg = LlmMessage {
            role: "tool".into(),
            content: r#"{"success":true,"screenshot_available":true}"#.into(),
            tool_calls: None,
            tool_call_id: Some("call_123".into()),
            attachments: vec![LlmAttachment {
                name: "screenshot.png".into(),
                mime_type: "image/png".into(),
                data: "iVBORw0KGgo=".into(),
            }],
            reasoning: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("screenshot_available"));
        assert!(json.contains("image/png"));
        assert!(json.contains("iVBORw0KGgo="));

        let parsed: LlmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "tool");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].mime_type, "image/png");
    }

    #[test]
    fn test_tool_message_without_attachment_no_attachment_field() {
        // When no attachments, the field should be omitted in serialization
        let msg = LlmMessage {
            role: "tool".into(),
            content: r#"{"success":true}"#.into(),
            tool_calls: None,
            tool_call_id: Some("call_123".into()),
            attachments: vec![],
            reasoning: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("attachments"));
    }

    #[test]
    fn test_extract_screenshot_not_triggered_for_compact_result() {
        // The new compact screenshot result should NOT trigger extract_screenshot_from_tool_result
        let compact = r#"{"success":true,"screenshot_available":true}"#;
        let result = extract_screenshot_from_tool_result(compact);
        assert!(
            result.is_none(),
            "Compact screenshot result should not have extractable screenshot"
        );
    }

    #[test]
    fn test_parse_tool_arguments_json_valid() {
        let result = parse_tool_arguments_json(r#"{"code": "print(1)"}"#);
        assert_eq!(result["code"].as_str().unwrap(), "print(1)");
    }

    #[test]
    fn test_parse_tool_arguments_json_invalid_escapes() {
        // \d is not a valid JSON escape but appears in regex
        let result = parse_tool_arguments_json(r#"{"code": "re.match(\\d+, s)"}"#);
        assert!(result["code"].as_str().is_some());
    }

    #[test]
    fn test_parse_tool_arguments_json_unescaped_quotes() {
        // LLM produces unescaped quotes: print("=" * 80)
        // In the raw JSON string: {"code": "print(\"=" * 80)"}
        let malformed = r#"{"code": "print(\"=" * 80)"}"#;
        let result = parse_tool_arguments_json(malformed);
        assert_eq!(result["code"].as_str().unwrap(), r#"print("=" * 80)"#);
    }

    #[test]
    fn llm_message_preserves_reasoning_across_serde() {
        let json = r#"{
            "role": "assistant",
            "content": "",
            "tool_calls": [{"id": "call_1", "name": "x", "arguments": {}}],
            "reasoning": "deciding which tool to call"
        }"#;
        let msg: LlmMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(
            msg.reasoning.as_deref(),
            Some("deciding which tool to call")
        );

        let reserialized = serde_json::to_string(&msg).unwrap();
        assert!(reserialized.contains("\"reasoning\":\"deciding which tool to call\""));
    }

    #[test]
    fn build_rig_assistant_message_never_emits_reasoning() {
        use rig::completion::message::AssistantContent;

        for provider in [
            ProviderType::DeepSeek,
            ProviderType::OpenAi,
            ProviderType::Anthropic,
        ] {
            let msg = LlmMessage {
                role: "assistant".into(),
                content: "hi".into(),
                tool_calls: None,
                tool_call_id: None,
                attachments: vec![],
                reasoning: Some("internal thoughts".into()),
            };
            let rig_msg = build_rig_assistant_message(&msg, provider);
            let rig::completion::message::Message::Assistant { content, .. } = rig_msg else {
                panic!("expected Assistant");
            };
            let has_reasoning = content
                .iter()
                .any(|c| matches!(c, AssistantContent::Reasoning(_)));
            assert!(
                !has_reasoning,
                "build_rig_assistant_message must never emit reasoning content; provider {:?} leaked it",
                provider
            );
        }
    }

    #[test]
    fn deepseek_request_body_packs_reasoning_with_tool_calls() {
        let request = LlmChatRequest {
            messages: vec![
                LlmMessage::user("hi"),
                LlmMessage {
                    role: "assistant".into(),
                    content: String::new(),
                    tool_calls: Some(vec![LlmToolCall {
                        id: "call_1".into(),
                        call_id: None,
                        name: "browser_get_markdown".into(),
                        arguments: serde_json::json!({"tab_id": 4}),
                        signature: None,
                    }]),
                    tool_call_id: None,
                    attachments: vec![],
                    reasoning: Some("step 1: get the page".into()),
                },
                LlmMessage {
                    role: "tool".into(),
                    content: "<page contents>".into(),
                    tool_calls: None,
                    tool_call_id: Some("call_1".into()),
                    attachments: vec![],
                    reasoning: None,
                },
            ],
            system: Some("system prompt".into()),
            temperature: None,
            max_tokens: Some(1024),
            tools: None,
        };

        let body = build_deepseek_request_body("deepseek-v4-flash", &request, true);

        let messages = body["messages"].as_array().expect("messages array");
        let assistant = messages
            .iter()
            .find(|m| m["role"] == "assistant")
            .expect("assistant message present");

        // CRITICAL: reasoning_content and tool_calls must be on the SAME message.
        assert_eq!(
            assistant["reasoning_content"], "step 1: get the page",
            "reasoning_content must ride with the assistant turn that has tool_calls"
        );
        let tool_calls = assistant["tool_calls"]
            .as_array()
            .expect("tool_calls present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "browser_get_markdown");

        assert_eq!(body["stream"], true);
        assert_eq!(body["model"], "deepseek-v4-flash");
        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn deepseek_request_body_omits_reasoning_when_absent() {
        let request = LlmChatRequest {
            messages: vec![LlmMessage::user("hi"), LlmMessage::assistant("hello")],
            system: None,
            temperature: None,
            max_tokens: None,
            tools: None,
        };

        let body = build_deepseek_request_body("deepseek-chat", &request, false);
        let messages = body["messages"].as_array().expect("messages");
        let assistant = messages.iter().find(|m| m["role"] == "assistant").unwrap();

        assert!(
            assistant.get("reasoning_content").is_none(),
            "reasoning_content key must be absent when no reasoning was carried"
        );
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn deepseek_sse_line_buffer_splits_correctly() {
        // This is a smoke test for the line-buffering invariant we encode in
        // stream_deepseek_raw. We extract the buffering logic into a small
        // testable helper to keep the test isolated from the full async path.
        // (See impl in stream_deepseek_raw — replicated logic here so a future
        // refactor that changes the buffering flow is forced to update this test.)

        let mut line_buf = String::new();
        let mut emitted_lines: Vec<String> = Vec::new();
        let chunks = [
            "data: {\"choices\":[{\"delta\":{\"co",
            "ntent\":\"hi\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}\nda",
            "ta: [DONE]\n",
        ];
        for chunk in &chunks {
            line_buf.push_str(chunk);
            while let Some(nl_pos) = line_buf.find('\n') {
                let line = line_buf[..nl_pos].trim_end_matches('\r').to_string();
                line_buf.drain(..=nl_pos);
                emitted_lines.push(line);
            }
        }

        assert_eq!(
            emitted_lines.len(),
            3,
            "three complete SSE lines must be emitted, even though the input is split into 4 byte-chunks at arbitrary boundaries"
        );
        assert!(emitted_lines[0].contains("\"content\":\"hi\""));
        assert!(emitted_lines[1].contains("\"reasoning_content\":\"think\""));
        assert_eq!(emitted_lines[2], "data: [DONE]");
    }
}
