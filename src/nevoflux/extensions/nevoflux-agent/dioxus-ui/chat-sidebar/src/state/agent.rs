/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Agent status state

use shared_protocol::AgentState;

/// Agent execution status for UI display
#[derive(Debug, Clone, Default)]
pub struct AgentStatusState {
    /// Current agent state
    pub state: AgentState,
    /// Currently executing tool
    pub current_tool: Option<ToolDisplayInfo>,
    /// Step progress information
    pub step: Option<StepDisplayInfo>,
    /// Error message if in error state
    pub error_message: Option<String>,
    /// Whether the status bar should be visible
    pub visible: bool,
}

/// Tool information for display
#[derive(Debug, Clone)]
pub struct ToolDisplayInfo {
    /// Tool name
    pub name: String,
    /// Display icon (emoji)
    pub icon: &'static str,
    /// Optional description or target
    pub description: Option<String>,
}

/// Step progress information
#[derive(Debug, Clone)]
pub struct StepDisplayInfo {
    /// Current step number
    pub current: u32,
    /// Total steps
    pub total: u32,
}

impl AgentStatusState {
    /// Check if agent is actively working
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            AgentState::Thinking
                | AgentState::Executing
                | AgentState::ExecutingTool
                | AgentState::Waiting
                | AgentState::WaitingResult
                | AgentState::WaitingConfirmation
        )
    }

    /// Get state display label
    pub fn state_label(&self) -> &'static str {
        match self.state {
            AgentState::Idle => "Ready",
            AgentState::Thinking => "Thinking...",
            AgentState::Executing | AgentState::ExecutingTool => "Executing",
            AgentState::Waiting | AgentState::WaitingResult => "Waiting",
            AgentState::WaitingConfirmation => "Waiting for confirmation",
            AgentState::Complete => "Complete",
            AgentState::Error => "Error",
        }
    }

    /// Set to thinking state
    pub fn set_thinking(&mut self) {
        self.state = AgentState::Thinking;
        self.current_tool = None;
        self.step = None;
        self.error_message = None;
        self.visible = true;
    }

    /// Set to executing state
    pub fn set_executing(&mut self) {
        self.state = AgentState::Executing;
        self.visible = true;
    }

    /// Set to waiting state
    pub fn set_waiting(&mut self) {
        self.state = AgentState::Waiting;
        self.visible = true;
    }

    /// Set to complete state
    pub fn set_complete(&mut self) {
        self.state = AgentState::Complete;
        self.visible = true;
    }

    /// Set to error state
    pub fn set_error(&mut self, message: &str) {
        self.state = AgentState::Error;
        self.error_message = Some(message.to_string());
        self.visible = true;
    }

    /// Hide the status bar
    pub fn hide(&mut self) {
        self.visible = false;
    }
}

/// Get display icon for a tool name
pub fn get_tool_icon(name: &str) -> &'static str {
    match name {
        "click_element" | "click" => "🖱",
        "type_text" | "type" | "input" => "⌨️",
        "screenshot" | "capture" => "📷",
        "scroll" | "scroll_page" => "↕️",
        "navigate" | "goto" | "open_url" => "🌐",
        "extract_content" | "read" | "get_text" => "📄",
        "wait" | "sleep" => "⏱",
        _ => "⚙️",
    }
}
