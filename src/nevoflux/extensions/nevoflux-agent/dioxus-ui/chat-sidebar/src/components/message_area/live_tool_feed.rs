/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Live tool feed component showing real-time tool execution status

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::{LiveToolEntry, LiveToolStatus};

/// Real-time tool execution feed displayed above streaming content
#[component]
pub fn LiveToolFeed() -> Element {
    let ctx = use_app_context();
    let live_tools = ctx.live_tools.read();

    if live_tools.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "live-tool-feed",
            for tool in live_tools.iter() {
                LiveToolChip {
                    key: "{tool.id}",
                    entry: tool.clone(),
                }
            }
        }
    }
}

/// Individual live tool execution chip
#[component]
pub fn LiveToolChip(entry: LiveToolEntry) -> Element {
    let status_icon = entry.status.icon();
    let status_class = entry.status.css_class();
    let is_running = matches!(entry.status, LiveToolStatus::Running);

    rsx! {
        div {
            class: "tool-chip tool-chip--{status_class}",

            span { class: "tool-chip__status",
                if is_running {
                    span { class: "tool-chip__spinner" }
                } else {
                    "{status_icon}"
                }
            }
            span { class: "tool-chip__icon", "{entry.icon}" }
            span { class: "tool-chip__name", "{entry.name}" }
            span { class: "tool-chip__summary", "{entry.summary}" }
            if let Some(ms) = entry.duration_ms {
                span { class: "tool-chip__duration",
                    "{format_duration(ms)}"
                }
            }
        }
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
