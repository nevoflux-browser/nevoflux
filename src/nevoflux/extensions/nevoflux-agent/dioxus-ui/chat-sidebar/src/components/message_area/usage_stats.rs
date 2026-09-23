/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Token stats shown at the right end of an assistant message's toolbar.

use super::usage_format::{detail_lines, is_estimated, summary_line, summary_segments, StatSegment};
use dioxus::prelude::*;
use shared_protocol::chat::TurnUsage;

/// Ever-increasing counter so each rendered stats block can point its
/// `aria-describedby` at its own detail element.
static DETAIL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// One reply's token stats: a quiet line in the toolbar, with the full
/// breakdown on hover or keyboard focus.
#[component]
pub fn UsageStats(usage: TurnUsage) -> Element {
    let segments = summary_segments(&usage);
    let lines = detail_lines(&usage);
    let estimated = is_estimated(&usage);
    // Stable for the life of this component instance, so focus does not
    // re-point the description at a different element on every render.
    let detail_id = use_hook(|| {
        format!(
            "usage-detail-{}",
            DETAIL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    // Screen readers get the summary plus the same breakdown the tooltip
    // shows, since the spans themselves are hidden from them.
    let aria = format!("{}; {}", summary_line(&usage), lines.join("; "));

    rsx! {
        div {
            class: "usage-stats",
            tabindex: "0",
            aria_describedby: "{detail_id}",
            aria_label: "{aria}",

            if estimated {
                span { class: "usage-estimated", aria_hidden: "true", "≈" }
            }
            span { class: "usage-summary", aria_hidden: "true",
                for (i, segment) in segments.iter().enumerate() {
                    match segment {
                        StatSegment::Label(text) => rsx! {
                            span { key: "{i}", class: "usage-label", "{text}" }
                        },
                        StatSegment::Value(text) => rsx! {
                            span { key: "{i}", class: "usage-value", "{text}" }
                        },
                        StatSegment::Sep => rsx! {
                            span { key: "{i}", class: "usage-sep", "·" }
                        },
                    }
                }
            }

            div {
                class: "usage-detail",
                id: "{detail_id}",
                role: "tooltip",
                for line in lines.iter() {
                    div { class: "usage-detail-line", "{line}" }
                }
            }
        }
    }
}
