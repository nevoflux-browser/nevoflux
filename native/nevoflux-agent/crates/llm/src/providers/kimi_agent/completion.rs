//! KimiAgentCompletionModel implementation.
//!
//! Implements the rig-core CompletionModel trait for the kimi-agent CLI.
//! Each outer turn spawns a new subprocess. Within a turn, the process
//! stays alive for mid-turn tool handling via ToolCallRequest/ToolResult.

use std::sync::Arc;

use rig::completion::{
    self, AssistantContent, CompletionError, CompletionRequest, CompletionResponse, Document,
    ToolDefinition, Usage,
};
use rig::message::{Message, ToolResultContent, UserContent};
use rig::streaming::StreamingCompletionResponse;
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use super::client::KimiAgentClient;
use super::types::{KimiAgentCompletionResponse, KimiUsage};
use super::wire::WireClient;

/// Completion model for the kimi-agent CLI (wire mode).
///
/// Implements the rig-core `CompletionModel` trait by communicating with
/// the `kimi-agent` subprocess over a JSON-RPC 2.0 wire protocol.
///
/// The wire client is kept alive between calls when mid-turn tool handling
/// is needed. A `None` wire client means a new turn should be started.
#[derive(Clone)]
pub struct KimiAgentCompletionModel {
    client: KimiAgentClient,
    model: String,
    /// Active wire client for mid-turn tool handling.
    wire: Arc<Mutex<Option<WireClient>>>,
    /// JSON-RPC request ID from the last ToolCallRequest.
    pending_jsonrpc_id: Arc<Mutex<Option<Value>>>,
}

impl KimiAgentCompletionModel {
    /// Create a new completion model.
    pub fn new(client: KimiAgentClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
            wire: Arc::new(Mutex::new(None)),
            pending_jsonrpc_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Get a reference to the underlying client.
    pub(crate) fn client(&self) -> &KimiAgentClient {
        &self.client
    }
}

/// Streaming response stub for kimi-agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KimiAgentStreamingResponse {
    pub usage: Option<KimiUsage>,
}

impl completion::GetTokenUsage for KimiAgentStreamingResponse {
    fn token_usage(&self) -> Option<Usage> {
        self.usage.as_ref().map(|u| Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
        })
    }
}

/// Build a text prompt from rig messages, system prompt, and documents.
///
/// The kimi-agent wire protocol accepts a single `user_input` string,
/// so multi-turn conversations are serialized into a structured text format.
pub(crate) fn build_prompt_text(
    messages: &[Message],
    system_prompt: Option<&str>,
    documents: &[Document],
) -> String {
    let mut parts = Vec::new();

    if let Some(sys) = system_prompt {
        if !sys.is_empty() {
            parts.push(format!("<system>\n{}\n</system>", sys));
        }
    }

    if !documents.is_empty() {
        let doc_text = documents
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("<attachments>\n{}\n</attachments>", doc_text));
    }

    let has_assistant = messages
        .iter()
        .any(|m| matches!(m, Message::Assistant { .. }));

    if has_assistant {
        parts.push("<conversation_history>".to_string());
        for msg in messages {
            match msg {
                Message::User { content } => {
                    let text = extract_user_text(content);
                    parts.push(format!("[user]: {}", text));
                }
                Message::Assistant { content, .. } => {
                    let text = extract_assistant_text(content);
                    parts.push(format!("[assistant]: {}", text));
                }
            }
        }
        parts.push("</conversation_history>".to_string());
        parts.push("Continue the conversation based on the history above.".to_string());
    } else {
        for msg in messages {
            if let Message::User { content } = msg {
                parts.push(extract_user_text(content));
            }
        }
    }

    parts.join("\n\n")
}

/// Extract text from user content, including tool results.
fn extract_user_text(content: &OneOrMany<UserContent>) -> String {
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
                Some(format!(
                    "<tool_result call_id=\"{}\">\n{}\n</tool_result>",
                    tr.id,
                    result_text.join("")
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract text from assistant content, including tool calls.
fn extract_assistant_text(content: &OneOrMany<AssistantContent>) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            AssistantContent::Text(t) => Some(t.text.clone()),
            AssistantContent::ToolCall(tc) => Some(format!(
                "<tool_call>\n{}\n</tool_call>",
                serde_json::json!({
                    "id": tc.id,
                    "name": tc.function.name,
                    "arguments": tc.function.arguments
                })
            )),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract tool results from the latest user message in the chat history.
///
/// Returns `(tool_call_id, output_text, is_error)` tuples.
fn extract_tool_results_from_messages(messages: &[Message]) -> Vec<(String, String, bool)> {
    let mut results = Vec::new();
    // Only look at the last user message for tool results
    for msg in messages.iter().rev() {
        if let Message::User { content } = msg {
            for c in content.iter() {
                if let UserContent::ToolResult(tr) = c {
                    let text: String = tr
                        .content
                        .iter()
                        .filter_map(|rc| match rc {
                            ToolResultContent::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    results.push((tr.id.clone(), text, false));
                }
            }
            break; // Only the last user message
        }
    }
    results
}

impl completion::CompletionModel for KimiAgentCompletionModel {
    type Response = KimiAgentCompletionResponse;
    type StreamingResponse = KimiAgentStreamingResponse;
    type Client = KimiAgentClient;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        Self::new(client.clone(), model)
    }

    async fn completion(
        &self,
        completion_request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        // Take the wire client and request ID out of the mutex
        let existing_wc = {
            let mut guard = self.wire.lock().await;
            guard.take()
        };
        let existing_id = {
            let mut guard = self.pending_jsonrpc_id.lock().await;
            guard.take()
        };

        // Prepare data for the blocking task
        let config = self.client.clone();
        let model_name = self.model.clone();
        let tools: Vec<ToolDefinition> = completion_request.tools.clone();
        let messages: Vec<Message> = completion_request.chat_history.iter().cloned().collect();
        let prompt = build_prompt_text(
            &messages,
            completion_request.preamble.as_deref(),
            &completion_request.documents,
        );
        let tool_results = extract_tool_results_from_messages(&messages);

        // Run blocking wire operations in a separate thread
        type WireResult = Result<
            (
                Option<WireClient>,
                Option<Value>,
                String,
                Vec<super::types::ExtractedToolCall>,
                KimiUsage,
            ),
            CompletionError,
        >;

        let result = tokio::task::spawn_blocking(move || -> WireResult {
            let mut wc = match existing_wc {
                None => {
                    // New turn: spawn process, initialize, send prompt
                    let mut wc = WireClient::spawn(&config, &model_name)
                        .map_err(CompletionError::ProviderError)?;
                    wc.initialize(&tools)
                        .map_err(CompletionError::ProviderError)?;
                    wc.send_prompt(&prompt)
                        .map_err(CompletionError::ProviderError)?;
                    wc
                }
                Some(mut wc) => {
                    // Resuming: send tool results back to the wire server
                    let request_id = existing_id.unwrap_or(Value::Null);
                    for (call_id, content, is_error) in &tool_results {
                        let err_msg = if *is_error {
                            Some(content.as_str())
                        } else {
                            None
                        };
                        wc.send_tool_result(
                            request_id.clone(),
                            call_id,
                            *is_error,
                            content,
                            err_msg,
                        )
                        .map_err(CompletionError::ProviderError)?;
                    }
                    wc
                }
            };

            let (text, tool_calls, usage) = wc
                .read_until_pause()
                .map_err(CompletionError::ProviderError)?;

            let has_tool_calls = !tool_calls.is_empty();
            let jsonrpc_id = wc.last_request_id().cloned();

            if !has_tool_calls {
                wc.kill();
                Ok((None, None, text, tool_calls, usage))
            } else {
                Ok((Some(wc), jsonrpc_id, text, tool_calls, usage))
            }
        })
        .await
        .map_err(|e| CompletionError::ProviderError(format!("Task join error: {}", e)))??;

        let (wc_back, jsonrpc_id, text, tool_calls, usage) = result;

        // Store wire client and request ID for potential next call
        {
            let mut guard = self.wire.lock().await;
            *guard = wc_back;
        }
        {
            let mut guard = self.pending_jsonrpc_id.lock().await;
            *guard = jsonrpc_id;
        }

        let response = KimiAgentCompletionResponse {
            content: text,
            tool_calls,
            usage,
        };
        response.try_into()
    }

    async fn stream(
        &self,
        _completion_request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        Err(CompletionError::ProviderError(
            "Streaming not yet supported for kimi-agent provider".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_model_new() {
        let client = KimiAgentClient::new("kimi-agent");
        let model = KimiAgentCompletionModel::new(client, "kimi-latest");
        assert_eq!(model.model(), "kimi-latest");
    }

    #[test]
    fn test_completion_model_clone() {
        let client = KimiAgentClient::new("kimi-agent");
        let model = KimiAgentCompletionModel::new(client, "kimi-latest");
        let cloned = model.clone();
        assert_eq!(cloned.model(), "kimi-latest");
    }

    #[test]
    fn test_build_prompt_single_user() {
        let messages = vec![Message::User {
            content: OneOrMany::one(UserContent::text("Hello, read a file for me")),
        }];
        let prompt = build_prompt_text(&messages, None, &[]);
        assert!(prompt.contains("Hello, read a file for me"));
        assert!(!prompt.contains("<conversation_history>"));
    }

    #[test]
    fn test_build_prompt_with_system() {
        let messages = vec![Message::User {
            content: OneOrMany::one(UserContent::text("Hi")),
        }];
        let prompt = build_prompt_text(&messages, Some("You are a helpful assistant"), &[]);
        assert!(prompt.contains("You are a helpful assistant"));
        assert!(prompt.contains("<system>"));
        assert!(prompt.contains("Hi"));
    }

    #[test]
    fn test_build_prompt_multiturn() {
        let messages = vec![
            Message::User {
                content: OneOrMany::one(UserContent::text("Hello")),
            },
            Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::text("Hi there")),
            },
            Message::User {
                content: OneOrMany::one(UserContent::text("How are you?")),
            },
        ];
        let prompt = build_prompt_text(&messages, None, &[]);
        assert!(prompt.contains("<conversation_history>"));
        assert!(prompt.contains("[user]: Hello"));
        assert!(prompt.contains("[assistant]: Hi there"));
        assert!(prompt.contains("[user]: How are you?"));
        assert!(prompt.contains("</conversation_history>"));
    }

    #[test]
    fn test_build_prompt_empty_system_skipped() {
        let messages = vec![Message::User {
            content: OneOrMany::one(UserContent::text("Hi")),
        }];
        let prompt = build_prompt_text(&messages, Some(""), &[]);
        assert!(!prompt.contains("<system>"));
    }

    #[test]
    fn test_build_prompt_with_tool_call_in_history() {
        use rig::message::{ToolCall, ToolFunction};

        let messages = vec![
            Message::User {
                content: OneOrMany::one(UserContent::text("Read /etc/hosts")),
            },
            Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::ToolCall(ToolCall::new(
                    "call_1".to_string(),
                    ToolFunction::new(
                        "read_file".to_string(),
                        serde_json::json!({"path": "/etc/hosts"}),
                    ),
                ))),
            },
            Message::User {
                content: OneOrMany::one(UserContent::text("Thanks")),
            },
        ];
        let prompt = build_prompt_text(&messages, None, &[]);
        assert!(prompt.contains("<tool_call>"));
        assert!(prompt.contains("read_file"));
    }

    #[test]
    fn test_extract_tool_results() {
        use rig::message::{ToolCall, ToolFunction, ToolResult, ToolResultContent};

        let messages = vec![
            Message::User {
                content: OneOrMany::one(UserContent::text("Do something")),
            },
            Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::ToolCall(ToolCall::new(
                    "call_1".to_string(),
                    ToolFunction::new("bash".to_string(), serde_json::json!({"cmd": "ls"})),
                ))),
            },
            Message::User {
                content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                    id: "call_1".to_string(),
                    call_id: None,
                    content: OneOrMany::one(ToolResultContent::text("file1.txt\nfile2.txt")),
                })),
            },
        ];

        let results = extract_tool_results_from_messages(&messages);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "call_1");
        assert_eq!(results[0].1, "file1.txt\nfile2.txt");
        assert!(!results[0].2);
    }

    #[test]
    fn test_implements_completion_model_trait() {
        fn assert_completion_model<T: completion::CompletionModel>() {}
        assert_completion_model::<KimiAgentCompletionModel>();
    }

    #[test]
    fn test_completion_model_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KimiAgentCompletionModel>();
    }

    #[test]
    fn test_streaming_response_token_usage() {
        use rig::completion::GetTokenUsage;

        let resp = KimiAgentStreamingResponse {
            usage: Some(KimiUsage {
                input_tokens: 100,
                output_tokens: 50,
            }),
        };
        let usage = resp.token_usage().unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_streaming_response_no_usage() {
        use rig::completion::GetTokenUsage;

        let resp = KimiAgentStreamingResponse { usage: None };
        assert!(resp.token_usage().is_none());
    }
}
