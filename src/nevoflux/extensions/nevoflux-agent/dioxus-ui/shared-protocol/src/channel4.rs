/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 4: Page Mode LLM (bidirectional)
//!
//! Messages for page mode LLM calls via browser automation.

use serde::{Deserialize, Serialize};
use crate::common::LlmProvider;

/// OpenAI-compatible message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub stream: bool,
}

/// OpenAI-compatible chunk delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// OpenAI-compatible chunk choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChunkChoice {
    pub delta: OpenAiDelta,
}

/// OpenAI-compatible chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChunk {
    pub choices: Vec<OpenAiChunkChoice>,
}

/// OpenAI-compatible completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletionChoice {
    pub message: OpenAiMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// OpenAI-compatible usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// OpenAI-compatible completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletion {
    pub choices: Vec<OpenAiCompletionChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

/// Page LLM request (Agent → Extension)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLlmRequestPayload {
    pub request_id: String,
    pub provider: LlmProvider,
    pub payload: OpenAiRequest,
}

/// Page LLM chunk (Extension → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLlmChunkPayload {
    pub request_id: String,
    pub payload: OpenAiChunk,
}

/// Page LLM done (Extension → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLlmDonePayload {
    pub request_id: String,
    pub payload: OpenAiCompletion,
}

/// Page LLM error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLlmError {
    pub code: String,
    pub message: String,
}

/// Page LLM error (Extension → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLlmErrorPayload {
    pub request_id: String,
    pub error: PageLlmError,
}

/// All Channel 4 message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum PageLlmMessage {
    PageLlmRequest(PageLlmRequestPayload),
    PageLlmChunk(PageLlmChunkPayload),
    PageLlmDone(PageLlmDonePayload),
    PageLlmError(PageLlmErrorPayload),
}

impl PageLlmMessage {
    /// Get request_id
    pub fn request_id(&self) -> &str {
        match self {
            Self::PageLlmRequest(p) => &p.request_id,
            Self::PageLlmChunk(p) => &p.request_id,
            Self::PageLlmDone(p) => &p.request_id,
            Self::PageLlmError(p) => &p.request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_llm_request_serialization() {
        let msg = PageLlmMessage::PageLlmRequest(PageLlmRequestPayload {
            request_id: "req-1".to_string(),
            provider: LlmProvider::Claude,
            payload: OpenAiRequest {
                model: "claude-3-opus".to_string(),
                messages: vec![OpenAiMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                }],
                stream: true,
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("page_llm_request"));
        assert!(json.contains("claude"));
    }

    #[test]
    fn test_page_llm_chunk_serialization() {
        let msg = PageLlmMessage::PageLlmChunk(PageLlmChunkPayload {
            request_id: "req-1".to_string(),
            payload: OpenAiChunk {
                choices: vec![OpenAiChunkChoice {
                    delta: OpenAiDelta {
                        content: Some("Hello".to_string()),
                    },
                }],
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("page_llm_chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_page_llm_done_serialization() {
        let msg = PageLlmMessage::PageLlmDone(PageLlmDonePayload {
            request_id: "req-done".to_string(),
            payload: OpenAiCompletion {
                choices: vec![OpenAiCompletionChoice {
                    message: OpenAiMessage {
                        role: "assistant".to_string(),
                        content: "Hello, how can I help?".to_string(),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Some(OpenAiUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                }),
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("page_llm_done"));
        assert!(json.contains("stop"));
    }

    #[test]
    fn test_page_llm_error_serialization() {
        let msg = PageLlmMessage::PageLlmError(PageLlmErrorPayload {
            request_id: "req-err".to_string(),
            error: PageLlmError {
                code: "RATE_LIMIT".to_string(),
                message: "Too many requests".to_string(),
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("page_llm_error"));
        assert!(json.contains("RATE_LIMIT"));
    }

    #[test]
    fn test_request_id_page_llm_request() {
        let msg = PageLlmMessage::PageLlmRequest(PageLlmRequestPayload {
            request_id: "test-request-id".to_string(),
            provider: LlmProvider::Claude,
            payload: OpenAiRequest {
                model: "test-model".to_string(),
                messages: vec![],
                stream: false,
            },
        });
        assert_eq!(msg.request_id(), "test-request-id");
    }

    #[test]
    fn test_request_id_page_llm_chunk() {
        let msg = PageLlmMessage::PageLlmChunk(PageLlmChunkPayload {
            request_id: "chunk-request-id".to_string(),
            payload: OpenAiChunk {
                choices: vec![],
            },
        });
        assert_eq!(msg.request_id(), "chunk-request-id");
    }

    #[test]
    fn test_request_id_page_llm_done() {
        let msg = PageLlmMessage::PageLlmDone(PageLlmDonePayload {
            request_id: "done-request-id".to_string(),
            payload: OpenAiCompletion {
                choices: vec![],
                usage: None,
            },
        });
        assert_eq!(msg.request_id(), "done-request-id");
    }

    #[test]
    fn test_request_id_page_llm_error() {
        let msg = PageLlmMessage::PageLlmError(PageLlmErrorPayload {
            request_id: "error-request-id".to_string(),
            error: PageLlmError {
                code: "ERR".to_string(),
                message: "Error".to_string(),
            },
        });
        assert_eq!(msg.request_id(), "error-request-id");
    }

    #[test]
    fn test_page_llm_request_roundtrip() {
        let original = PageLlmMessage::PageLlmRequest(PageLlmRequestPayload {
            request_id: "roundtrip-req".to_string(),
            provider: LlmProvider::Chatgpt,
            payload: OpenAiRequest {
                model: "gpt-4".to_string(),
                messages: vec![
                    OpenAiMessage {
                        role: "system".to_string(),
                        content: "You are helpful.".to_string(),
                    },
                    OpenAiMessage {
                        role: "user".to_string(),
                        content: "Hi".to_string(),
                    },
                ],
                stream: true,
            },
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: PageLlmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id(), "roundtrip-req");
    }

    #[test]
    fn test_openai_delta_empty_content() {
        let delta = OpenAiDelta { content: None };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(!json.contains("content"));
    }

    #[test]
    fn test_openai_completion_without_usage() {
        let completion = OpenAiCompletion {
            choices: vec![OpenAiCompletionChoice {
                message: OpenAiMessage {
                    role: "assistant".to_string(),
                    content: "Response".to_string(),
                },
                finish_reason: None,
            }],
            usage: None,
        };
        let json = serde_json::to_string(&completion).unwrap();
        assert!(!json.contains("usage"));
        assert!(!json.contains("finish_reason"));
    }
}
