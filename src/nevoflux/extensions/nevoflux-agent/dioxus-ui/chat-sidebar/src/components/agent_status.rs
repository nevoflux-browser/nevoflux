/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Agent Status Component
//!
//! Displays the current state of the AI agent during Computer Use operations.
//! Shows thinking indicator, tool execution progress, and step counts.

use dioxus::prelude::*;
use shared_protocol::AgentState;

/// Agent execution status for display
#[derive(Debug, Clone, Default)]
pub struct AgentStatus {
    pub state: AgentState,
    pub current_tool: Option<String>,
    pub step_count: u32,
    pub max_steps: u32,
    pub message: Option<String>,
    pub visible: bool,
}

impl AgentStatus {
    pub fn new() -> Self {
        Self {
            state: AgentState::Thinking,
            current_tool: None,
            step_count: 0,
            max_steps: 10,
            message: None,
            visible: false,
        }
    }

    /// Check if agent is actively working
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            AgentState::Thinking | AgentState::ExecutingTool | AgentState::WaitingResult
        )
    }

    /// Get state display label
    pub fn state_label(&self) -> &'static str {
        match self.state {
            AgentState::Thinking => "Thinking...",
            AgentState::ExecutingTool => "Executing",
            AgentState::WaitingResult => "Waiting",
            AgentState::Complete => "Complete",
            AgentState::Error => "Error",
            AgentState::WaitingConfirmation => "Needs Confirmation",
        }
    }

    /// Get progress percentage
    pub fn progress(&self) -> u32 {
        if self.max_steps == 0 {
            0
        } else {
            (self.step_count * 100) / self.max_steps
        }
    }
}

/// Agent Status Display Component
#[component]
pub fn AgentStatusDisplay(status: Signal<AgentStatus>) -> Element {
    let status_val = status.read();

    // Don't render if not visible
    if !status_val.visible {
        return rsx! {};
    }

    let state_class = match status_val.state {
        AgentState::Thinking => "thinking",
        AgentState::ExecutingTool => "executing",
        AgentState::WaitingResult => "waiting",
        AgentState::Complete => "complete",
        AgentState::Error => "error",
        AgentState::WaitingConfirmation => "confirmation",
    };

    let progress = status_val.progress();
    let is_active = status_val.is_active();

    rsx! {
        div {
            class: "agent-status {state_class}",
            class: if is_active { "active" } else { "" },

            // Status icon and label
            div {
                class: "status-header",

                // Animated icon based on state
                div {
                    class: "status-icon",

                    match status_val.state {
                        AgentState::Thinking => rsx! {
                            // Thinking dots animation
                            span { class: "thinking-dots",
                                span { class: "dot", "•" }
                                span { class: "dot", "•" }
                                span { class: "dot", "•" }
                            }
                        },
                        AgentState::ExecutingTool | AgentState::WaitingResult => rsx! {
                            // Spinner
                            span { class: "spinner" }
                        },
                        AgentState::Complete => rsx! {
                            // Checkmark
                            span { class: "checkmark", "✓" }
                        },
                        AgentState::Error => rsx! {
                            // Error icon
                            span { class: "error-icon", "✕" }
                        },
                        AgentState::WaitingConfirmation => rsx! {
                            // Warning icon
                            span { class: "warning-icon", "⚠" }
                        },
                    }
                }

                // Status text
                span {
                    class: "status-label",
                    "{status_val.state_label()}"
                }

                // Current tool if executing
                if let Some(ref tool) = status_val.current_tool {
                    span {
                        class: "current-tool",
                        "{tool}"
                    }
                }
            }

            // Progress bar
            if is_active {
                div {
                    class: "progress-container",

                    div {
                        class: "progress-bar",
                        style: "width: {progress}%",
                    }

                    span {
                        class: "step-count",
                        "Step {status_val.step_count}/{status_val.max_steps}"
                    }
                }
            }

            // Message if any
            if let Some(ref msg) = status_val.message {
                div {
                    class: "status-message",
                    "{msg}"
                }
            }
        }
    }
}

/// Compact inline status indicator
#[component]
pub fn AgentStatusIndicator(status: Signal<AgentStatus>) -> Element {
    let status_val = status.read();

    if !status_val.is_active() {
        return rsx! {};
    }

    rsx! {
        span {
            class: "agent-indicator",
            title: "{status_val.state_label()}",

            span { class: "indicator-dot pulsing" }

            if let Some(ref tool) = status_val.current_tool {
                span { class: "indicator-text", "{tool}" }
            }
        }
    }
}
