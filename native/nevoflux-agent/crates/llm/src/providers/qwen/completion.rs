//! QwenCompletionModel implementation.
//!
//! Implements the rig-core CompletionModel trait for Qwen models via DashScope API.
//! Also provides streaming support via Server-Sent Events (SSE).

use std::pin::Pin;

use futures::stream::{Stream, StreamExt};
use rig::completion::{
    self, AssistantContent, CompletionError, CompletionRequest, CompletionResponse, Document,
    ToolDefinition, Usage,
};
use rig::message::{Message, ToolResultContent, UserContent};
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{QwenClient, QwenUsage};
use crate::error::LlmError;

/// Completion model for Qwen via DashScope API.
///
/// Implements the rig-core `CompletionModel` trait for seamless integration
/// with the rig framework.
#[derive(Clone)]
pub struct QwenCompletionModel {
    client: QwenClient,
    model: String,
}

impl QwenCompletionModel {
    /// Create a new completion model.
    pub fn new(client: QwenClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }

    /// Get the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Stream a chat completion response.
    ///
    /// Returns a stream of content chunks as the model generates them.
    /// Uses Server-Sent Events (SSE) format from the DashScope API.
    pub async fn stream_chat(
        &self,
        messages: Vec<QwenMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let request = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .client
            .http_client()
            .post(format!("{}/chat/completions", self.client.base_url()))
            .bearer_auth(self.client.api_key())
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message });
        }

        let byte_stream = response.bytes_stream();

        let stream = byte_stream.filter_map(|result| async move {
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut content_parts = Vec::new();
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                continue;
                            }
                            if let Ok(chunk) = serde_json::from_str::<QwenStreamChunk>(data) {
                                if let Some(choice) = chunk.choices.first() {
                                    if let Some(content) = &choice.delta.content {
                                        if !content.is_empty() {
                                            content_parts.push(content.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if content_parts.is_empty() {
                        None
                    } else {
                        Some(Ok(content_parts.join("")))
                    }
                }
                Err(e) => Some(Err(LlmError::Stream(e.to_string()))),
            }
        });

        Ok(Box::pin(stream))
    }
}

// ============================================================================
// Qwen-specific message type for API serialization
// ============================================================================

/// Simple message structure for Qwen API serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QwenMessage {
    pub role: String,
    pub content: String,
    /// Tool call ID for tool result messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl QwenMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Convert rig Message to QwenMessage for API serialization.
fn message_to_qwen(msg: &Message) -> QwenMessage {
    match msg {
        Message::User { content } => {
            let text = extract_text_from_user_content(content);
            QwenMessage::user(text)
        }
        Message::Assistant { content, .. } => {
            let text = extract_text_from_assistant_content(content);
            QwenMessage::assistant(text)
        }
    }
}

/// Extract text content from UserContent.
fn extract_text_from_user_content(content: &OneOrMany<UserContent>) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            UserContent::Text(t) => Some(t.text.clone()),
            UserContent::ToolResult(tr) => {
                let result_text: Vec<String> = tr
                    .content
                    .iter()
                    .filter_map(|rc| match rc {
                        ToolResultContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect();
                Some(format!("[Tool Result {}]: {}", tr.id, result_text.join("")))
            }
            _ => None, // Skip images, audio, documents for now
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract text content from AssistantContent.
fn extract_text_from_assistant_content(content: &OneOrMany<AssistantContent>) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            AssistantContent::Text(t) => Some(t.text.clone()),
            AssistantContent::ToolCall(tc) => Some(format!(
                "[Tool Call {}]: {}({})",
                tc.id, tc.function.name, tc.function.arguments
            )),
            AssistantContent::Reasoning(r) => {
                Some(format!("[Reasoning]: {}", r.reasoning.join(" ")))
            }
            AssistantContent::Image(_) => None, // Skip images for now
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// API response types
// ============================================================================

/// API error response from DashScope.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    message: String,
}

/// API response wrapper for handling success and error responses.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiResponse<T> {
    Ok(T),
    Err(ApiErrorResponse),
}

/// Tool call structure for function calling.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub call_type: String,
    pub function: FunctionCall,
}

/// Function call details.
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Tool definition for Qwen API format.
#[derive(Clone, Debug, Serialize)]
pub struct QwenToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolDefinition,
}

impl From<ToolDefinition> for QwenToolDefinition {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            tool_type: "function".into(),
            function: tool,
        }
    }
}

/// Extended message for completion responses with optional tool calls.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompletionMessage {
    #[allow(dead_code)]
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Extended choice for completion responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompletionChoice {
    #[allow(dead_code)]
    pub index: u32,
    pub message: CompletionMessage,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
}

/// Extended completion response that supports tool calls.
#[derive(Debug, Deserialize, Serialize)]
pub struct QwenCompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: QwenUsage,
}

// ============================================================================
// Streaming types
// ============================================================================

/// Streaming chunk from DashScope API.
#[derive(Debug, Clone, Deserialize)]
pub struct QwenStreamChunk {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

/// Choice in a streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    #[allow(dead_code)]
    pub index: u32,
    pub delta: StreamDelta,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
}

/// Delta content in streaming.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StreamDelta {
    #[serde(default)]
    #[allow(dead_code)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// Streaming response for Qwen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QwenStreamingResponse {
    pub usage: Option<QwenUsage>,
}

impl completion::GetTokenUsage for QwenStreamingResponse {
    fn token_usage(&self) -> Option<Usage> {
        self.usage.as_ref().map(|u| Usage {
            input_tokens: u.prompt_tokens as u64,
            output_tokens: u.completion_tokens as u64,
            total_tokens: u.total_tokens as u64,
        })
    }
}

impl TryFrom<QwenCompletionResponse> for CompletionResponse<QwenCompletionResponse> {
    type Error = CompletionError;

    fn try_from(value: QwenCompletionResponse) -> Result<Self, Self::Error> {
        let usage = Usage {
            input_tokens: value.usage.prompt_tokens as u64,
            output_tokens: value.usage.completion_tokens as u64,
            total_tokens: value.usage.total_tokens as u64,
        };

        match value.choices.as_slice() {
            // Handle tool calls
            [CompletionChoice {
                message:
                    CompletionMessage {
                        tool_calls: Some(calls),
                        ..
                    },
                ..
            }, ..]
                if !calls.is_empty() =>
            {
                let call = calls.first().unwrap();
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                Ok(CompletionResponse {
                    choice: OneOrMany::one(AssistantContent::tool_call(
                        &call.id,
                        &call.function.name,
                        args,
                    )),
                    usage,
                    raw_response: value,
                })
            }
            // Handle text message
            [CompletionChoice {
                message:
                    CompletionMessage {
                        content: Some(content),
                        ..
                    },
                ..
            }, ..] => Ok(CompletionResponse {
                choice: OneOrMany::one(AssistantContent::text(content)),
                usage,
                raw_response: value,
            }),
            _ => Err(CompletionError::ResponseError(
                "Response did not contain a message or tool call".into(),
            )),
        }
    }
}

/// Build prompt with context documents.
fn prompt_with_context(prompt: &Message, documents: &[Document]) -> String {
    let text = match prompt {
        Message::User { content } => extract_text_from_user_content(content),
        Message::Assistant { content, .. } => extract_text_from_assistant_content(content),
    };

    if !documents.is_empty() {
        format!(
            "<attachments>\n{}</attachments>\n\n{}",
            documents
                .iter()
                .map(|doc| doc.to_string())
                .collect::<Vec<_>>()
                .join(""),
            text
        )
    } else {
        text
    }
}

impl completion::CompletionModel for QwenCompletionModel {
    type Response = QwenCompletionResponse;
    type StreamingResponse = QwenStreamingResponse;
    type Client = QwenClient;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        Self::new(client.clone(), model)
    }

    async fn completion(
        &self,
        completion_request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        // Build messages starting with preamble (system message)
        let mut messages: Vec<QwenMessage> = if let Some(preamble) = &completion_request.preamble {
            vec![QwenMessage::system(preamble)]
        } else {
            vec![]
        };

        // The last message in chat_history is the prompt - get all but the last for history
        let history_msgs: Vec<_> = completion_request.chat_history.iter().collect();
        let (history, prompt_msg) = if history_msgs.len() > 1 {
            (&history_msgs[..history_msgs.len() - 1], history_msgs.last())
        } else {
            (&[][..], history_msgs.first())
        };

        // Append chat history (excluding the last message which is the prompt)
        for msg in history {
            messages.push(message_to_qwen(msg));
        }

        // Add the user's prompt with context documents
        if let Some(prompt) = prompt_msg {
            let user_prompt = prompt_with_context(prompt, &completion_request.documents);
            messages.push(QwenMessage::user(user_prompt));
        }

        // Build the request JSON
        // Note: Qwen API rejects null for temperature/max_tokens, so only include when Some.
        let mut request = json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });
        if let Some(temp) = completion_request.temperature {
            request["temperature"] = json!(temp);
        }
        if let Some(max) = completion_request.max_tokens {
            request["max_tokens"] = json!(max);
        }
        if !completion_request.tools.is_empty() {
            request["tools"] = json!(completion_request
                .tools
                .iter()
                .cloned()
                .map(QwenToolDefinition::from)
                .collect::<Vec<_>>());
            request["tool_choice"] = json!("auto");
        }

        // Merge additional params if provided
        let final_request = if let Some(params) = completion_request.additional_params {
            merge_json(request, params)
        } else {
            request
        };

        // Send the request
        let response = self
            .client
            .http_client()
            .post(format!("{}/chat/completions", self.client.base_url()))
            .bearer_auth(self.client.api_key())
            .json(&final_request)
            .send()
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;

        if response.status().is_success() {
            match response
                .json::<ApiResponse<QwenCompletionResponse>>()
                .await
                .map_err(|e| CompletionError::ProviderError(e.to_string()))?
            {
                ApiResponse::Ok(resp) => resp.try_into(),
                ApiResponse::Err(err) => Err(CompletionError::ProviderError(err.message)),
            }
        } else {
            let error_text = response
                .text()
                .await
                .map_err(|e| CompletionError::ProviderError(e.to_string()))?;
            Err(CompletionError::ProviderError(error_text))
        }
    }

    async fn stream(
        &self,
        completion_request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        // Build messages starting with preamble (system message)
        let mut messages: Vec<QwenMessage> = if let Some(preamble) = &completion_request.preamble {
            vec![QwenMessage::system(preamble)]
        } else {
            vec![]
        };

        // The last message in chat_history is the prompt
        let history_msgs: Vec<_> = completion_request.chat_history.iter().collect();
        let (history, prompt_msg) = if history_msgs.len() > 1 {
            (&history_msgs[..history_msgs.len() - 1], history_msgs.last())
        } else {
            (&[][..], history_msgs.first())
        };

        for msg in history {
            messages.push(message_to_qwen(msg));
        }

        if let Some(prompt) = prompt_msg {
            let user_prompt = prompt_with_context(prompt, &completion_request.documents);
            messages.push(QwenMessage::user(user_prompt));
        }

        // Note: Qwen API rejects null for temperature/max_tokens, so only include when Some.
        let mut request = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });
        if let Some(temp) = completion_request.temperature {
            request["temperature"] = json!(temp);
        }
        if let Some(max) = completion_request.max_tokens {
            request["max_tokens"] = json!(max);
        }
        if !completion_request.tools.is_empty() {
            request["tools"] = json!(completion_request
                .tools
                .iter()
                .cloned()
                .map(QwenToolDefinition::from)
                .collect::<Vec<_>>());
            request["tool_choice"] = json!("auto");
        }

        let final_request = if let Some(params) = completion_request.additional_params {
            merge_json(request, params)
        } else {
            request
        };

        let response = self
            .client
            .http_client()
            .post(format!("{}/chat/completions", self.client.base_url()))
            .bearer_auth(self.client.api_key())
            .json(&final_request)
            .send()
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| CompletionError::ProviderError(e.to_string()))?;
            return Err(CompletionError::ProviderError(error_text));
        }

        let byte_stream = response.bytes_stream();

        let stream = byte_stream.filter_map(|result| async move {
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut content_parts = Vec::new();
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                continue;
                            }
                            if let Ok(chunk) = serde_json::from_str::<QwenStreamChunk>(data) {
                                if let Some(choice) = chunk.choices.first() {
                                    if let Some(content) = &choice.delta.content {
                                        if !content.is_empty() {
                                            content_parts.push(content.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if content_parts.is_empty() {
                        None
                    } else {
                        Some(Ok(RawStreamingChoice::Message(content_parts.join(""))))
                    }
                }
                Err(e) => Some(Err(CompletionError::ProviderError(e.to_string()))),
            }
        });

        Ok(StreamingCompletionResponse::stream(Box::pin(stream)))
    }
}

/// Merge two JSON values, with the second taking precedence.
fn merge_json(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                base_map.insert(key, value);
            }
            serde_json::Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::qwen::QwenClient;

    #[test]
    fn test_completion_model_new() {
        let client = QwenClient::new("test-key");
        let model = QwenCompletionModel::new(client, "qwen-turbo");
        assert_eq!(model.model(), "qwen-turbo");
    }

    #[test]
    fn test_completion_model_clone() {
        let client = QwenClient::new("test-key");
        let model = QwenCompletionModel::new(client, "qwen-plus");
        let cloned = model.clone();
        assert_eq!(cloned.model(), "qwen-plus");
    }

    #[test]
    fn test_tool_definition_conversion() {
        let rig_tool = ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get weather information".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
        };

        let qwen_tool: QwenToolDefinition = rig_tool.into();
        assert_eq!(qwen_tool.tool_type, "function");
        assert_eq!(qwen_tool.function.name, "get_weather");
    }

    #[test]
    fn test_completion_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "model": "qwen-turbo",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let resp: QwenCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.model, "qwen-turbo");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("Hello!"));
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn test_completion_response_conversion_message() {
        let json = r#"{
            "id": "chatcmpl-123",
            "model": "qwen-turbo",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello there!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let resp: QwenCompletionResponse = serde_json::from_str(json).unwrap();
        let rig_resp: CompletionResponse<QwenCompletionResponse> = resp.try_into().unwrap();

        // Check that we got text content
        let first = rig_resp.choice.first();
        assert!(matches!(first, AssistantContent::Text(_)));
    }

    #[test]
    fn test_completion_response_conversion_tool_call() {
        let json = r#"{
            "id": "chatcmpl-456",
            "model": "qwen-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"Tokyo\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 10,
                "total_tokens": 25
            }
        }"#;

        let resp: QwenCompletionResponse = serde_json::from_str(json).unwrap();
        let rig_resp: CompletionResponse<QwenCompletionResponse> = resp.try_into().unwrap();

        let first = rig_resp.choice.first();
        match first {
            AssistantContent::ToolCall(tc) => {
                assert_eq!(tc.function.name, "get_weather");
                assert_eq!(tc.function.arguments["location"], "Tokyo");
            }
            _ => panic!("Expected ToolCall choice"),
        }
    }

    #[test]
    fn test_merge_json() {
        let base = serde_json::json!({
            "model": "qwen-turbo",
            "temperature": 0.7
        });
        let overlay = serde_json::json!({
            "temperature": 0.5,
            "top_p": 0.9
        });

        let merged = merge_json(base, overlay);
        assert_eq!(merged["model"], "qwen-turbo");
        assert_eq!(merged["temperature"], 0.5);
        assert_eq!(merged["top_p"], 0.9);
    }

    #[test]
    fn test_qwen_message() {
        let msg = QwenMessage::user("Hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello");

        let msg = QwenMessage::assistant("Hi!");
        assert_eq!(msg.role, "assistant");

        let msg = QwenMessage::system("You are helpful");
        assert_eq!(msg.role, "system");
    }

    #[test]
    fn test_implements_completion_model_trait() {
        fn assert_completion_model<T: completion::CompletionModel>() {}
        assert_completion_model::<QwenCompletionModel>();
    }

    #[test]
    fn test_completion_model_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<QwenCompletionModel>();
    }

    #[test]
    fn test_empty_choices_returns_error() {
        let json = r#"{
            "id": "chatcmpl-123",
            "model": "qwen-turbo",
            "choices": [],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 0,
                "total_tokens": 10
            }
        }"#;

        let resp: QwenCompletionResponse = serde_json::from_str(json).unwrap();
        let result: Result<CompletionResponse<QwenCompletionResponse>, _> = resp.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_stream_chunk_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "model": "qwen-turbo",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        }"#;
        let chunk: QwenStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_stream_delta_default() {
        let delta = StreamDelta::default();
        assert!(delta.role.is_none());
        assert!(delta.content.is_none());
    }
}
