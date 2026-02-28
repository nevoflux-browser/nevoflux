/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Activity feed component showing tool calls above message content

use dioxus::prelude::*;
use crate::state::{ToolCallData, ToolCallStatus, ActivityKind};

/// Collapsible activity feed showing tool calls with duration and status
#[component]
pub fn ActivityFeed(tool_calls: Vec<ToolCallData>) -> Element {
    let mut expanded = use_signal(|| false);
    let tool_count = tool_calls.iter().filter(|tc| matches!(tc.kind, ActivityKind::Tool)).count();
    let thought_count = tool_calls.iter().filter(|tc| matches!(tc.kind, ActivityKind::Thinking { .. })).count();

    let header_label = match (tool_count, thought_count) {
        (0, t) => {
            let s = if t != 1 { "s" } else { "" };
            format!("{} thought{}", t, s)
        }
        (a, 0) => {
            let s = if a != 1 { "s" } else { "" };
            format!("{} action{}", a, s)
        }
        (a, t) => {
            let as_ = if a != 1 { "s" } else { "" };
            let ts = if t != 1 { "s" } else { "" };
            format!("{} action{}, {} thought{}", a, as_, t, ts)
        }
    };

    // Calculate total duration from tools that have timing data
    let total_ms: u64 = tool_calls.iter()
        .filter_map(|tc| tc.duration_ms)
        .sum();
    let has_timing = total_ms > 0;

    rsx! {
        div {
            class: "activity-feed",
            class: if expanded() { "expanded" },

            // Header (clickable to toggle)
            button {
                class: "activity-feed-header",
                onclick: move |_| expanded.set(!expanded()),
                aria_expanded: "{expanded}",
                span { class: "activity-feed-icon", "\u{26A1}" }
                span { class: "activity-feed-label",
                    "{header_label}"
                }
                if has_timing {
                    span { class: "activity-feed-timing",
                        "(total: {format_duration(total_ms)})"
                    }
                }
                span {
                    class: "activity-feed-chevron",
                    if expanded() { "\u{25B4}" } else { "\u{25BE}" }
                }
            }

            // Expandable tool list
            if expanded() {
                div { class: "activity-feed-list",
                    for tc in tool_calls.iter() {
                        if matches!(tc.kind, ActivityKind::Thinking { .. }) {
                            ThinkingChip {
                                key: "{tc.id}",
                                tool_call: tc.clone(),
                            }
                        } else {
                            ToolCallChip {
                                key: "{tc.id}",
                                tool_call: tc.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Individual tool call chip with status, duration, and expandable details
#[component]
fn ToolCallChip(tool_call: ToolCallData) -> Element {
    let mut detail_expanded = use_signal(|| false);
    let has_args = !tool_call.arguments.is_empty();

    // Status icon
    let status_icon = match &tool_call.status {
        Some(ToolCallStatus::Success) => "\u{2705}",   // ✅
        Some(ToolCallStatus::Failed) => "\u{274C}",    // ❌
        None => "\u{2699}\u{FE0F}",                    // ⚙️
    };

    let status_class = match &tool_call.status {
        Some(ToolCallStatus::Success) => "status-success",
        Some(ToolCallStatus::Failed) => "status-failed",
        None => "status-unknown",
    };

    rsx! {
        div {
            class: "tool-call-chip {status_class}",
            class: if detail_expanded() { "detail-expanded" },

            // Chip row (clickable if has args)
            button {
                class: "tool-call-row",
                onclick: move |_| {
                    if has_args {
                        detail_expanded.set(!detail_expanded());
                    }
                },
                span { class: "tool-call-status", "{status_icon}" }
                span { class: "tool-call-icon", "{tool_call.icon}" }
                span { class: "tool-call-name", "{tool_call.name}" }
                if let Some(ref target) = tool_call.display_target {
                    span { class: "tool-call-target", "{target}" }
                }
                if let Some(ms) = tool_call.duration_ms {
                    span { class: "tool-call-duration", "{format_duration(ms)}" }
                }
                if has_args {
                    span {
                        class: "tool-call-expand",
                        if detail_expanded() { "\u{25B4}" } else { "\u{25B8}" }
                    }
                }
            }

            // Expanded arguments
            if detail_expanded() && has_args {
                div { class: "tool-call-detail",
                    pre { class: "tool-call-args",
                        "{format_json_args(&tool_call.arguments)}"
                    }
                }
            }
        }
    }
}

/// Collapsible thinking/reasoning chip in the activity feed
#[component]
fn ThinkingChip(tool_call: ToolCallData) -> Element {
    let mut detail_expanded = use_signal(|| false);
    let content = match &tool_call.kind {
        ActivityKind::Thinking { content } => content.clone(),
        _ => String::new(),
    };
    let has_content = !content.is_empty();

    // Truncate for summary display
    let mut chars = content.chars();
    let summary: String = chars.by_ref().take(60).collect();
    let summary = if chars.next().is_some() {
        format!("{}...", summary)
    } else {
        summary
    };

    rsx! {
        div {
            class: "tool-call-chip thinking-chip",
            class: if detail_expanded() { "detail-expanded" },

            button {
                class: "tool-call-row",
                onclick: move |_| {
                    if has_content {
                        detail_expanded.set(!detail_expanded());
                    }
                },
                span { class: "tool-call-status", "\u{1F4AD}" }
                span { class: "tool-call-icon", "\u{1F4AD}" }
                span { class: "tool-call-name", "Thinking" }
                span { class: "tool-call-target", "{summary}" }
                if let Some(ms) = tool_call.duration_ms {
                    span { class: "tool-call-duration", "{format_duration(ms)}" }
                }
                if has_content {
                    span {
                        class: "tool-call-expand",
                        if detail_expanded() { "\u{25B4}" } else { "\u{25B8}" }
                    }
                }
            }

            if detail_expanded() && has_content {
                div { class: "thinking-detail",
                    "{content}"
                }
            }
        }
    }
}

/// Format JSON arguments for display (pretty-print if valid JSON)
fn format_json_args(args: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(args) {
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| args.to_string())
    } else {
        args.to_string()
    }
}

/// Format duration for display (ms → human readable)
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// Static "Done" indicator for live assistant messages without tool calls.
/// Visually matches ActivityFeed header to prevent layout shift when
/// StreamingBubble is replaced by MessageBubble.
#[component]
pub fn DoneFeed() -> Element {
    rsx! {
        div { class: "activity-feed done-feed",
            div { class: "activity-feed-header done-feed-header",
                span { class: "done-feed-icon", "\u{2713}" }
                span { class: "activity-feed-label", "Done" }
            }
        }
    }
}
