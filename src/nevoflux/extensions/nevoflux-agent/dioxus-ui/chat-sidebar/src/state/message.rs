/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message types for chat conversations

use shared_protocol::{StreamFormat, ToolCallInfo};
use crate::state::tools::ActivityKind;

/// Tool call data for display in activity feed
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallData {
    /// Tool call ID from the agent
    pub id: String,
    /// Tool name (e.g., "Read", "Bash", "click")
    pub name: String,
    /// Display icon (emoji or string from daemon)
    pub icon: String,
    /// Extracted human-readable target (e.g., file path, command)
    pub display_target: Option<String>,
    /// Raw JSON arguments for expanded view
    pub arguments: String,
    /// Execution duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Completion status
    pub status: Option<ToolCallStatus>,
    /// Whether this is a tool call or a thinking block
    pub kind: ActivityKind,
}

/// Tool call completion status for activity feed display
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Success,
    Failed,
}

/// Image attachment for messages
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAttachment {
    /// Unique ID
    pub id: String,
    /// Display name
    pub name: String,
    /// MIME type (e.g., "image/png")
    pub mime_type: String,
    /// Base64 encoded image data
    pub data: String,
}

/// A chat message in the conversation
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Unique message ID
    pub id: String,
    /// Message role (user/assistant/system)
    pub role: MessageRole,
    /// Message content
    pub content: MessageContent,
    /// Image attachments (displayed above text)
    pub attachments: Vec<ImageAttachment>,
    /// Tool calls from agent (displayed as activity feed)
    pub tool_calls: Vec<ToolCallData>,
    /// Timestamp in milliseconds
    pub timestamp: u64,
    /// Message status
    pub status: MessageStatus,
    /// Whether this message was received live (during current session).
    /// Used to show "Done" indicator for no-tool-call responses.
    /// Historical messages loaded from session.resolve keep the default `false`.
    pub is_live: bool,
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Message content variants
#[derive(Debug, Clone, PartialEq)]
pub enum MessageContent {
    /// Plain text
    Text(String),
    /// Markdown formatted text
    Markdown(String),
    /// Code block with language
    Code { language: String, code: String },
    /// Error message
    Error {
        code: String,
        message: String,
        recoverable: bool,
    },
    /// Plan proposal from agent
    Plan(PlanData),
    /// Artifact card (canvas preview)
    Artifact(ArtifactData),
    /// Q&A pair (user's response to agent's question)
    QA { question: String, answer: String },
}

/// Artifact streaming state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactState {
    Streaming,
    Complete,
}

/// Artifact data for display in ArtifactCard
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactData {
    /// Artifact ID (used in nevoflux://canvas/{id})
    pub id: String,
    /// Display title
    pub title: String,
    /// Content type (text/html, text/markdown, image/svg+xml, etc.)
    pub content_type: String,
    /// Current state
    pub state: ArtifactState,
}

/// Message delivery status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MessageStatus {
    /// Message is being sent
    Sending,
    /// Message was sent successfully
    #[default]
    Sent,
    /// Message failed to send
    Error,
}

/// A single step in a plan
#[derive(Debug, Clone, PartialEq)]
pub struct PlanStepData {
    pub description: String,
    pub model: Option<String>,
}

/// Plan data for display
#[derive(Debug, Clone, PartialEq)]
pub struct PlanData {
    pub summary: String,
    pub steps: Vec<PlanStepData>,
    /// Whether this plan is the active (latest) one awaiting user response
    pub is_active: bool,
}

impl Message {
    /// Create a new user message
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: MessageContent::Text(text.into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a new user message with image attachments
    pub fn user_with_images(text: impl Into<String>, images: Vec<ImageAttachment>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: MessageContent::Text(text.into()),
            attachments: images,
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a new assistant message with plain text
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Text(text.into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a new assistant message with markdown
    pub fn assistant_markdown(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Markdown(content.into()),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a new error message
    pub fn error(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: MessageContent::Error {
                code: code.into(),
                message: message.into(),
                recoverable,
            },
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a code message
    pub fn code(language: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Code {
                language: language.into(),
                code: code.into(),
            },
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a plan proposal message
    pub fn plan(summary: impl Into<String>, steps: Vec<PlanStepData>, is_active: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Plan(PlanData {
                summary: summary.into(),
                steps,
                is_active,
            }),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create a Q&A message (user's response to agent's question)
    pub fn qa(question: impl Into<String>, answer: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: MessageContent::QA {
                question: question.into(),
                answer: answer.into(),
            },
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create an artifact message
    pub fn artifact(id: impl Into<String>, title: impl Into<String>, content_type: impl Into<String>, state: ArtifactState) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Artifact(ArtifactData {
                id: id.into(),
                title: title.into(),
                content_type: content_type.into(),
                state,
            }),
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Create an assistant message with activity feed (tool calls + filtered content)
    pub fn assistant_with_activity(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallData>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Markdown(content.into()),
            attachments: Vec::new(),
            tool_calls,
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
            is_live: false,
        }
    }

    /// Mark this message as received live (during current session).
    /// Live messages show a "Done" indicator when they have no tool calls.
    pub fn set_live(mut self) -> Self {
        self.is_live = true;
        self
    }
}

/// State for a message being streamed
#[derive(Debug, Clone, PartialEq)]
pub struct StreamingState {
    /// Stream ID
    pub id: String,
    /// Accumulated content
    pub content: String,
    /// Content format
    pub format: StreamFormat,
    /// Tool calls accumulated from non-done stream chunks
    pub tool_calls: Vec<ToolCallInfo>,
}

impl StreamingState {
    /// Create a new streaming state
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            content: String::new(),
            format: StreamFormat::Markdown,
            tool_calls: Vec::new(),
        }
    }

    /// Append content to the stream
    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    /// Accumulate tool calls from a non-done stream chunk
    pub fn accumulate_tool_calls(&mut self, calls: &[ToolCallInfo]) {
        for tc in calls {
            // Deduplicate by ID
            if !self.tool_calls.iter().any(|existing| existing.id == tc.id) {
                self.tool_calls.push(tc.clone());
            }
        }
    }
}

/// Tab context for display
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabContext {
    /// Tab ID (Firefox numeric ID, used for browser tools)
    pub tab_id: u32,
    /// Zen Sync ID (persistent across restarts, used as session_id)
    pub zen_sync_id: Option<String>,
    /// Tab URL
    pub url: String,
    /// Tab title
    pub title: String,
    /// Favicon URL
    pub favicon_url: Option<String>,
}
