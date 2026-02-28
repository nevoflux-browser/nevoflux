/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Rail component — minimized vertical toolbar (48px)

use dioxus::prelude::*;
use shared_protocol::AgentState;
use wasm_bindgen_futures::spawn_local;

use crate::context::use_app_context;

/// Compact vertical rail shown when sidebar is minimized
#[component]
pub fn Rail() -> Element {
    let ctx = use_app_context();
    let avatar = ctx.avatar_url.read();
    let status = ctx.agent_status.read();
    let is_streaming = ctx.streaming.read().is_some();

    let is_active = is_streaming
        || matches!(
            status.state,
            AgentState::Thinking | AgentState::Executing | AgentState::ExecutingTool
        );

    let handle_expand = move |_| {
        spawn_local(async move {
            if let Err(e) = crate::messaging::send_set_sidebar_width(500).await {
                tracing::error!("Failed to expand sidebar: {}", e);
            }
        });
    };

    rsx! {
        div { class: "rail",
            // Top: Avatar
            div { class: "rail-avatar",
                if let Some(ref url) = *avatar {
                    img {
                        src: "{url}",
                        alt: "Avatar",
                        class: "rail-avatar-img",
                    }
                } else {
                    // Default avatar placeholder
                    div { class: "rail-avatar-placeholder",
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "8", r: "4" }
                            path { d: "M20 21a8 8 0 1 0-16 0" }
                        }
                    }
                }
            }

            // Middle: Agent status indicator
            div {
                class: if is_active { "rail-status rail-status--active" } else { "rail-status" },
                title: if is_active { "Agent is working..." } else { "Agent idle" },
                div { class: "rail-status-dot" }
            }

            // Spacer
            div { class: "rail-spacer" }

            // Bottom: Expand button
            button {
                class: "rail-expand-btn",
                aria_label: "Expand sidebar",
                title: "Expand sidebar",
                onclick: handle_expand,
                // Left-pointing arrow (expand = open panel to the left)
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M15 18l-6-6 6-6" }
                }
            }
        }
    }
}
