/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message list component

use dioxus::prelude::*;
use shared_protocol::AgentState;
use crate::context::use_app_context;
use crate::state::{MessageContent, StreamingState, ActivityKind};
use crate::components::{PlanCard, ArtifactCard};
use super::MessageBubble;

/// Message list component showing all chat messages
#[component]
pub fn MessageList() -> Element {
    let ctx = use_app_context();
    let messages_signal = ctx.messages;
    let streaming_signal = ctx.streaming;
    let agent_status_signal = ctx.agent_status;

    let messages = messages_signal.read();
    let streaming = streaming_signal.read();
    let agent_status = agent_status_signal.read();

    // Debug: log every render of MessageList
    {
        let msg_summary: Vec<String> = messages.iter().map(|m| {
            format!("{}(tc={})", m.id.get(..8).unwrap_or(&m.id), m.tool_calls.len())
        }).collect();
        web_sys::console::log_1(&format!(
            "[WASM] MessageList RENDER: {} msgs [{}], streaming={}, thinking={}",
            messages.len(),
            msg_summary.join(", "),
            streaming.is_some(),
            agent_status.visible && agent_status.is_active(),
        ).into());
    }

    // Find the index of the last user message
    let last_user_index = messages.iter().enumerate()
        .filter(|(_, m)| m.role == crate::state::MessageRole::User)
        .map(|(i, _)| i)
        .last();

    // Show thinking indicator when agent is actively working (before AND during streaming)
    let show_thinking = agent_status.visible && agent_status.is_active();

    // Auto-scroll to bottom when messages change or streaming updates
    use_effect(move || {
        // Read signals inside effect to track changes
        let msg_count = messages_signal.read().len();
        let stream_len = streaming_signal.read().as_ref().map(|s| s.content.len()).unwrap_or(0);
        let _agent_visible = agent_status_signal.read().visible;

        // Log for debugging
        tracing::debug!("Scroll trigger: messages={}, stream_len={}", msg_count, stream_len);

        // Use spawn to scroll after a small delay for DOM to update
        spawn(async move {
            // Small delay to ensure DOM is rendered
            gloo::timers::future::TimeoutFuture::new(50).await;
            scroll_to_bottom();
        });
    });

    rsx! {
        div { class: "message-list",
            // Historical messages
            for (index, msg) in messages.iter().enumerate() {
                if matches!(msg.content, MessageContent::Plan(_)) {
                    PlanCard {
                        key: "{msg.id}",
                        message: msg.clone(),
                    }
                } else if matches!(msg.content, MessageContent::Artifact(_)) {
                    ArtifactCard {
                        key: "{msg.id}",
                        message: msg.clone(),
                    }
                } else {
                    MessageBubble {
                        key: "{msg.id}",
                        message: msg.clone(),
                        is_last_user: last_user_index == Some(index),
                        message_index: index,
                    }
                }
            }

            // Streaming message (action area + content only, no dots inside)
            if streaming.is_some() || show_thinking {
                StreamingBubble {
                    stream: streaming.as_ref().cloned(),
                }
            }

            // Thinking indicator — standalone below all messages/streaming.
            // Stays in the same position regardless of StreamingBubble ↔ MessageBubble
            // transitions, preventing layout shift when the bubble swaps.
            if show_thinking {
                ThinkingIndicator {}
            }

            // Scroll anchor
            div { class: "scroll-anchor", id: "message-scroll-anchor" }
        }
    }
}

/// Scroll the message list to the bottom using scrollIntoView on anchor
fn scroll_to_bottom() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            // Try scrollIntoView on the scroll anchor
            if let Some(anchor) = document.get_element_by_id("message-scroll-anchor") {
                // Use scrollIntoView with smooth behavior
                let options = web_sys::ScrollIntoViewOptions::new();
                options.set_behavior(web_sys::ScrollBehavior::Smooth);
                options.set_block(web_sys::ScrollLogicalPosition::End);
                anchor.scroll_into_view_with_scroll_into_view_options(&options);
            } else if let Some(element) = document.query_selector(".message-list").ok().flatten() {
                // Fallback: set scroll top directly
                let scroll_height = element.scroll_height();
                element.set_scroll_top(scroll_height);
            }
        }
    }
}

/// Inline thinking indicator shown in the message list below user messages.
///
/// Displays a prominent animated icon with dynamic status text that updates
/// based on agent state (thinking, executing tool, step progress).
#[component]
fn ThinkingIndicator() -> Element {
    let ctx = use_app_context();
    let status = ctx.agent_status.read();

    let state_class = match status.state {
        AgentState::Thinking => "thinking",
        AgentState::Executing | AgentState::ExecutingTool => "executing",
        AgentState::Waiting | AgentState::WaitingResult | AgentState::WaitingConfirmation => "waiting",
        AgentState::Error => "error",
        _ => "thinking",
    };

    let is_thinking_only = matches!(status.state, AgentState::Thinking) && status.current_tool.is_none();

    // Build status text only for non-thinking states
    let status_text = if is_thinking_only {
        String::new()
    } else {
        match &status.current_tool {
            Some(tool) => {
                if let Some(ref desc) = tool.description {
                    format!("{} {}", tool.name, desc)
                } else {
                    tool.name.clone()
                }
            }
            None => status.state_label().to_string(),
        }
    };

    // Step progress text
    let step_text = status.step.as_ref().map(|s| format!("Step {}/{}", s.current, s.total));

    rsx! {
        div {
            class: "thinking-indicator {state_class}",
            role: "status",
            aria_live: "polite",

            // Animated icon area
            div { class: "thinking-icon-wrapper",
                match status.state {
                    AgentState::Thinking => rsx! {
                        ThinkingAnimation {}
                    },
                    AgentState::Executing | AgentState::ExecutingTool => rsx! {
                        ExecutingAnimation {}
                    },
                    AgentState::Waiting | AgentState::WaitingResult | AgentState::WaitingConfirmation => rsx! {
                        WaitingAnimation {}
                    },
                    _ => rsx! {
                        ThinkingAnimation {}
                    },
                }
            }

            // Status content (hidden for pure thinking state)
            if !is_thinking_only {
                div { class: "thinking-content",
                    span { class: "thinking-label", "{status_text}" }

                    if let Some(ref step) = step_text {
                        span { class: "thinking-step", "{step}" }
                    }

                    // Tool chip (when executing a specific tool)
                    if let Some(ref tool) = status.current_tool {
                        span { class: "thinking-tool-chip",
                            span { class: "thinking-tool-icon", "{tool.icon}" }
                        }
                    }
                }
            }

        }
    }
}

/// Animated dots for the thinking state
#[component]
fn ThinkingAnimation() -> Element {
    rsx! {
        div { class: "thinking-anim",
            span { class: "thinking-dot dot-1" }
            span { class: "thinking-dot dot-2" }
            span { class: "thinking-dot dot-3" }
        }
    }
}

/// Spinner for executing state
#[component]
fn ExecutingAnimation() -> Element {
    rsx! {
        div { class: "executing-anim",
            svg {
                class: "executing-spinner",
                width: "20",
                height: "20",
                view_box: "0 0 20 20",
                circle {
                    class: "spinner-track",
                    cx: "10",
                    cy: "10",
                    r: "8",
                    fill: "none",
                    stroke_width: "2",
                }
                circle {
                    class: "spinner-head",
                    cx: "10",
                    cy: "10",
                    r: "8",
                    fill: "none",
                    stroke_width: "2",
                    stroke_dasharray: "20 30",
                    stroke_linecap: "round",
                }
            }
        }
    }
}

/// Pulsing ring for waiting state
#[component]
fn WaitingAnimation() -> Element {
    rsx! {
        div { class: "waiting-anim",
            span { class: "waiting-ring" }
            span { class: "waiting-dot" }
        }
    }
}

/// Streaming action area — always-present container styled like ActivityFeed.
///
/// Shows a header ("Working..." or tool count) and live tool chips.
/// Visually matches the final ActivityFeed so the transition from
/// streaming → final message is smooth.
#[component]
fn StreamingActionArea() -> Element {
    let ctx = use_app_context();
    let live_tools = ctx.live_tools.read();
    let tool_count = live_tools.len();
    let has_tools = tool_count > 0;

    // Header label — distinguish tools vs thinking items
    let act_count = live_tools.iter().filter(|e| matches!(e.kind, ActivityKind::Tool)).count();
    let tht_count = live_tools.iter().filter(|e| matches!(e.kind, ActivityKind::Thinking { .. })).count();

    let header_text = match (act_count, tht_count) {
        (0, 0) => "Working...".to_string(),
        (0, t) => format!("{} thought{}", t, if t != 1 { "s" } else { "" }),
        (a, 0) => format!("{} tool{}", a, if a != 1 { "s" } else { "" }),
        (a, t) => format!("{} tool{}, {} thought{}", a, if a != 1 { "s" } else { "" }, t, if t != 1 { "s" } else { "" }),
    };

    rsx! {
        div { class: "streaming-action-area activity-feed",

            // Header
            div { class: "streaming-action-header activity-feed-header",
                span { class: "streaming-action-icon", "\u{26A1}" }
                span { class: "streaming-action-label", "{header_text}" }
            }

            // Live tool chips (when tools are running)
            if has_tools {
                div { class: "live-tool-feed",
                    for tool in live_tools.iter() {
                        super::live_tool_feed::LiveToolChip {
                            key: "{tool.id}",
                            entry: tool.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// Streaming message bubble showing action area and streamed content.
///
/// Shows the StreamingActionArea (always present) above any streaming text
/// content. The ThinkingIndicator (dots) is rendered separately in MessageList
/// below this bubble to prevent layout shift on transitions.
#[component]
fn StreamingBubble(stream: Option<StreamingState>) -> Element {
    let has_content = stream.as_ref().is_some_and(|s| !s.content.is_empty());
    let rendered = stream
        .as_ref()
        .map(|s| super::render_simple_markdown(&s.content))
        .unwrap_or_default();

    rsx! {
        div {
            class: "message-row assistant",

            div {
                class: "message-bubble assistant streaming",
                aria_live: "polite",
                aria_atomic: "false",

                // Action area: always present during streaming
                StreamingActionArea {}

                // Show content area if we have streamed text
                if has_content {
                    div { class: "bubble-content",
                        div { class: "markdown-content",
                            dangerous_inner_html: "{rendered}"
                        }
                    }
                }
            }
        }
    }
}
