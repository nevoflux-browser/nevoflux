/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! State types for the Content Sidebar

use serde::{Deserialize, Serialize};
use shared_protocol::DisplayContentType;

/// Content Sidebar display state
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ContentSidebarState {
    /// Default state - showing current URL
    #[default]
    Default,
    /// Displaying content from Chat Sidebar
    DisplayingContent,
    /// Loading content
    Loading,
    /// Error state
    Error(String),
}

/// Content to display in the sidebar
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayContent {
    /// Content type determines rendering
    pub content_type: DisplayContentType,
    /// The actual content
    pub content: String,
    /// Optional title
    pub title: Option<String>,
    /// Session ID for tracking
    pub session_id: String,
}

impl DisplayContent {
    /// Create new markdown content
    pub fn markdown(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content_type: DisplayContentType::Markdown,
            content: content.into(),
            title: None,
            session_id: session_id.into(),
        }
    }

    /// Create new HTML content
    pub fn html(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content_type: DisplayContentType::Html,
            content: content.into(),
            title: None,
            session_id: session_id.into(),
        }
    }

    /// Create new text content
    pub fn text(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content_type: DisplayContentType::Text,
            content: content.into(),
            title: None,
            session_id: session_id.into(),
        }
    }

    /// Create new code content
    pub fn code(session_id: impl Into<String>, content: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            content_type: DisplayContentType::Code { language: language.into() },
            content: content.into(),
            title: None,
            session_id: session_id.into(),
        }
    }

    /// Set title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Highlight state for page elements
#[derive(Debug, Clone, Default)]
pub struct HighlightState {
    /// CSS selector of highlighted element
    pub selector: Option<String>,
    /// Highlight visual style
    pub style: HighlightStyle,
    /// Original element styles (for restoration)
    pub original_styles: Option<String>,
}

/// Visual style for element highlighting
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HighlightStyle {
    #[default]
    Outline,
    Overlay,
    Pulse,
}

impl HighlightState {
    /// Create new highlight for selector
    pub fn new(selector: impl Into<String>, style: HighlightStyle) -> Self {
        Self {
            selector: Some(selector.into()),
            style,
            original_styles: None,
        }
    }

    /// Clear highlight
    pub fn clear(&mut self) {
        self.selector = None;
        self.original_styles = None;
    }

    /// Check if highlighting is active
    pub fn is_active(&self) -> bool {
        self.selector.is_some()
    }
}
