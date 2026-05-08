/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Per-iteration card rendered inline in the message stream.

use crate::state::IterationRow;
use dioxus::prelude::*;

/// Per-iteration card rendered in the message stream.
/// Tap header to expand and inspect the tool-call summary.
#[component]
pub fn IterationCard(loop_id: String, row: IterationRow) -> Element {
    let mut expanded = use_signal(|| row.status == "running");
    let duration_ms = match row.ended_at {
        Some(ended) => Some((ended - row.started_at).max(0) * 1000),
        None => None,
    };

    let trace = match &row.tool_calls_summary {
        serde_json::Value::Array(arr) => arr.clone(),
        _ => Vec::new(),
    };

    rsx! {
        div { class: "loop-iter-card status-{row.status}",
            "data-loop-id": "{loop_id}",
            div {
                class: "loop-iter-header",
                onclick: move |_| {
                    let next = !expanded();
                    expanded.set(next);
                },
                span { class: "iter-seq", "iter {row.sequence_number}" }
                if !row.fire_reason.is_empty() {
                    span { class: "iter-trigger", "{row.fire_reason}" }
                }
                span { class: "iter-status", "{row.status}" }
                if let Some(ms) = duration_ms {
                    span { class: "iter-duration", "{ms}ms" }
                }
            }
            if expanded() && !trace.is_empty() {
                div { class: "iter-tool-trace",
                    for call in trace.iter() {
                        div { class: "iter-tool-row",
                            span { class: "tool-name",
                                "{call.get(\"name\").and_then(|v| v.as_str()).unwrap_or(\"?\")}"
                            }
                            span { class: "tool-ok",
                                if call.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                                    "ok"
                                } else {
                                    "fail"
                                }
                            }
                            span { class: "tool-ms",
                                "{call.get(\"ms\").and_then(|v| v.as_i64()).unwrap_or(0)}ms"
                            }
                        }
                    }
                }
            }
        }
    }
}
