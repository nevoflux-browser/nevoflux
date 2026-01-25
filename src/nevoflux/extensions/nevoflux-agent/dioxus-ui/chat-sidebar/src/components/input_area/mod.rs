/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Input area components

mod context_bar;
mod text_input;

pub use context_bar::ContextBar;
pub use text_input::TextInput;

use dioxus::prelude::*;
use crate::context::use_app_context;

/// Input area component
#[component]
pub fn InputArea() -> Element {
    let ctx = use_app_context();
    let is_connected = ctx.connection.read().is_connected();
    let is_agent_active = ctx.agent_status.read().is_active();

    // Only disable input when agent is actively processing
    // Allow typing even when not connected (can send when connection is ready)
    let disabled = is_agent_active;

    rsx! {
        div { class: "input-area",
            // Context bar (current tab info)
            ContextBar {}

            // Input row
            div { class: "input-row",
                TextInput { disabled }
            }

            // Disclaimer
            div { class: "input-disclaimer",
                "AI can make mistakes. Please verify important information."
            }
        }
    }
}
