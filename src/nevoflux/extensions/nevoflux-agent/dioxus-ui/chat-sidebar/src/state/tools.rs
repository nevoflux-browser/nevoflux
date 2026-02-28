/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Live tool execution state for real-time tool status display

use shared_protocol::ToolAuthRequest;

/// Discriminator for activity items in the feed
#[derive(Clone, Debug, PartialEq)]
pub enum ActivityKind {
    /// A tool execution
    Tool,
    /// A thinking/reasoning block with accumulated content
    Thinking { content: String },
}

impl Default for ActivityKind {
    fn default() -> Self {
        Self::Tool
    }
}

/// Real-time tool execution entry displayed during streaming
#[derive(Clone, Debug, PartialEq)]
pub struct LiveToolEntry {
    /// Unique tool call ID (correlates start/end events)
    pub id: String,
    /// Tool name ("read", "grep", "bash")
    pub name: String,
    /// Display icon (emoji)
    pub icon: String,
    /// Human-readable summary
    pub summary: String,
    /// Current execution status
    pub status: LiveToolStatus,
    /// Execution duration in milliseconds (set on completion)
    pub duration_ms: Option<u64>,
    /// Whether this is a tool call or a thinking block
    pub kind: ActivityKind,
}

/// Live tool execution status
#[derive(Clone, Debug, PartialEq)]
pub enum LiveToolStatus {
    /// Tool is currently executing
    Running,
    /// Tool is waiting for user authorization
    WaitingAuth(ToolAuthRequest),
    /// Tool completed successfully
    Success,
    /// Tool execution failed
    Failed,
}

impl LiveToolStatus {
    /// Get status icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Running => "\u{23F3}",      // ⏳
            Self::WaitingAuth(_) => "\u{26A0}\u{FE0F}", // ⚠️
            Self::Success => "\u{2705}",      // ✅
            Self::Failed => "\u{274C}",       // ❌
        }
    }

    /// Get CSS class suffix
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::WaitingAuth(_) => "waiting-auth",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}
