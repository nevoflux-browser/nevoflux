/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Application state types for the Chat Sidebar

use serde::{Deserialize, Serialize};
use shared_protocol::TabContext as ProtocolTabContext;
use shared_protocol::TabStatus as ProtocolTabStatus;

/// Global application state
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// Current session ID
    pub session_id: String,
    /// Whether the sidebar is actively being used
    pub is_active: bool,
    /// Current view mode
    pub view_mode: ViewMode,
}

/// View modes for the sidebar
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    Chat,
    Settings,
    History,
}

/// Chat message in the conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: u64,
    #[serde(default)]
    pub status: MessageStatus,
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Message status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    #[default]
    Sent,
    Delivered,
    Error,
}

/// Currently streaming message
#[derive(Debug, Clone, PartialEq)]
pub struct StreamingMessage {
    pub stream_id: String,
    pub content: String,
    pub is_complete: bool,
}

/// Tab context for display
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabContext {
    pub tab_id: u32,
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub is_loading: bool,
}

impl From<ProtocolTabContext> for TabContext {
    fn from(ctx: ProtocolTabContext) -> Self {
        Self {
            tab_id: ctx.tab_id,
            url: ctx.url,
            title: ctx.title,
            favicon_url: ctx.favicon_url,
            is_loading: matches!(ctx.status, ProtocolTabStatus::Loading),
        }
    }
}

/// Connection status with native agent
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl ChatMessage {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.into(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Sent,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Delivered,
        }
    }

    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: content.into(),
            timestamp: js_sys::Date::now() as u64,
            status: MessageStatus::Delivered,
        }
    }
}

impl StreamingMessage {
    /// Create a new streaming message
    pub fn new(stream_id: impl Into<String>) -> Self {
        Self {
            stream_id: stream_id.into(),
            content: String::new(),
            is_complete: false,
        }
    }

    /// Append content to the stream
    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    /// Mark stream as complete
    pub fn complete(&mut self) {
        self.is_complete = true;
    }

    /// Convert to a final chat message
    pub fn into_message(self) -> ChatMessage {
        ChatMessage::assistant(self.content)
    }
}
