/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message types for chat conversations

use shared_protocol::StreamFormat;

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
    /// Timestamp in milliseconds
    pub timestamp: u64,
    /// Message status
    pub status: MessageStatus,
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

impl Message {
    /// Create a new user message
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: MessageContent::Text(text.into()),
            attachments: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
        }
    }

    /// Create a new user message with image attachments
    pub fn user_with_images(text: impl Into<String>, images: Vec<ImageAttachment>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: MessageContent::Text(text.into()),
            attachments: images,
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
        }
    }

    /// Create a new assistant message with plain text
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Text(text.into()),
            attachments: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
        }
    }

    /// Create a new assistant message with markdown
    pub fn assistant_markdown(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: MessageContent::Markdown(content.into()),
            attachments: Vec::new(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
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
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
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
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
        }
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
}

impl StreamingState {
    /// Create a new streaming state
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            content: String::new(),
            format: StreamFormat::Markdown,
        }
    }

    /// Append content to the stream
    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
    }
}

/// Tab context for display
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabContext {
    /// Tab ID
    pub tab_id: u32,
    /// Tab URL
    pub url: String,
    /// Tab title
    pub title: String,
    /// Favicon URL
    pub favicon_url: Option<String>,
}
