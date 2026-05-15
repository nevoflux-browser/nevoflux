//! Request and response types for DashScope API.

use serde::{Deserialize, Serialize};

/// Chat completion request for DashScope API.
#[derive(Debug, Clone, Serialize)]
pub struct QwenChatRequest {
    /// Model identifier (e.g., "qwen-turbo", "qwen-plus")
    pub model: String,
    /// Conversation messages
    pub messages: Vec<QwenMessage>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Enable streaming response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QwenMessage {
    /// Role: "system", "user", or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
}

/// Chat completion response from DashScope API.
#[derive(Debug, Clone, Deserialize)]
pub struct QwenChatResponse {
    /// Unique response ID
    pub id: String,
    /// Model that generated the response
    pub model: String,
    /// Response choices
    pub choices: Vec<QwenChoice>,
    /// Token usage statistics
    pub usage: QwenUsage,
}

/// A choice in the completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct QwenChoice {
    /// Choice index
    pub index: u32,
    /// Generated message
    pub message: QwenMessage,
    /// Reason for completion (e.g., "stop", "length")
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QwenUsage {
    /// Tokens used for the prompt
    pub prompt_tokens: u32,
    /// Tokens generated in the completion
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qwen_message_serialization() {
        let msg = QwenMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_qwen_chat_request_serialization() {
        let req = QwenChatRequest {
            model: "qwen-turbo".to_string(),
            messages: vec![QwenMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens: Some(100),
            temperature: None,
            stream: Some(false),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"qwen-turbo\""));
        assert!(json.contains("\"max_tokens\":100"));
        // temperature should not appear when None
        assert!(!json.contains("temperature"));
    }

    #[test]
    fn test_qwen_chat_response_deserialization() {
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
        let resp: QwenChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "Hello!");
        assert_eq!(resp.usage.total_tokens, 15);
    }
}
