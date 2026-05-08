/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Sticky stack of loop cards rendered above the message list.

use crate::context::use_app_context;
use crate::messaging::send_loop_cancel;
use dioxus::prelude::*;

/// Sticky stack of loop cards, one per active loop in the current session.
/// Loops in `cancelled` state are hidden after rendering once
/// (the AppContext.loops signal still holds them; callers can clear later).
#[component]
pub fn StickyLoopCards() -> Element {
    let ctx = use_app_context();
    let loops = ctx.loops.read();
    let session = ctx.session.read();
    let active_session_id = session.id.clone();
    drop(session);

    let visible: Vec<crate::state::LoopState> = loops
        .values()
        .filter(|s| s.session_id == active_session_id && s.state != "cancelled")
        .cloned()
        .collect();
    drop(loops);

    if visible.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "loop-sticky-stack",
            for state in visible.iter() {
                StickyLoopCard {
                    key: "{state.loop_id}",
                    state: state.clone(),
                }
            }
        }
    }
}

#[component]
fn StickyLoopCard(state: crate::state::LoopState) -> Element {
    let session_id = state.session_id.clone();
    let loop_id = state.loop_id.clone();

    let on_cancel = move |_| {
        let s = session_id.clone();
        let l = loop_id.clone();
        spawn(async move {
            let _ = send_loop_cancel(&s, &l, false).await;
        });
    };

    let scratch_visible = state.scratchpad_bytes > 0;

    rsx! {
        div { class: "loop-sticky-card",
            div { class: "loop-sticky-row",
                span { class: "loop-state state-{state.state}", "{state.state}" }
                span { class: "loop-trigger", "{state.trigger_expr}" }
                span { class: "loop-counts",
                    "iter {state.iteration_count}"
                    if state.skipped_triggers > 0 {
                        " · skipped {state.skipped_triggers}"
                    }
                }
                button {
                    class: "loop-cancel-btn",
                    onclick: on_cancel,
                    "cancel"
                }
            }
            if scratch_visible {
                div { class: "loop-scratchpad-preview",
                    "scratch ({state.scratchpad_bytes}b): {state.scratchpad_preview}"
                }
            }
        }
    }
}
