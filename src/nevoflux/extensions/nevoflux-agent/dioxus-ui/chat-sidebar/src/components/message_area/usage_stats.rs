/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Token stats shown at the right end of an assistant message's toolbar.

use super::usage_format::{detail_lines, is_estimated, summary_line};
use dioxus::prelude::*;
use shared_protocol::chat::TurnUsage;

/// Ever-increasing counter so each rendered stats block can point its
/// `aria-describedby` at its own detail element.
static DETAIL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// One reply's token stats: a quiet line in the toolbar, with the full
/// breakdown on hover or keyboard focus.
#[component]
pub fn UsageStats(usage: TurnUsage) -> Element {
    let summary = summary_line(&usage);
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
    // Screen readers get the same breakdown the tooltip shows.
    let aria = lines.join("; ");

    rsx! {
        div {
            class: "usage-stats",
            tabindex: "0",
            aria_describedby: "{detail_id}",
            aria_label: "{aria}",

            if estimated {
                span { class: "usage-estimated", aria_hidden: "true", "≈" }
            }
            span { class: "usage-summary", aria_hidden: "true", "{summary}" }

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
