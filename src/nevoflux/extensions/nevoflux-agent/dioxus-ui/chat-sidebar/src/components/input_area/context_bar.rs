/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Context bar showing current tab information

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::TabContext;
use crate::utils::{truncate, extract_domain};

/// Context bar component showing current tab info
#[component]
pub fn ContextBar() -> Element {
    let mut ctx = use_app_context();
    let tab = ctx.tab_context.read();

    // Don't show if no context
    if tab.url.is_empty() {
        return rsx! {};
    }

    let handle_remove = move |_| {
        ctx.tab_context.set(TabContext::default());
    };

    rsx! {
        div { class: "context-bar",
            // Favicon
            if let Some(ref favicon) = tab.favicon_url {
                img {
                    class: "context-favicon",
                    src: "{favicon}",
                    alt: "",
                    width: "16",
                    height: "16",
                }
            }

            // Title (truncated)
            span { class: "context-title", "{truncate(&tab.title, 40)}" }

            // Domain
            span { class: "context-domain", "{extract_domain(&tab.url)}" }

            // Remove button
            button {
                class: "context-remove",
                onclick: handle_remove,
                aria_label: "Remove context",
                title: "Remove tab context",
                "×"
            }
        }
    }
}
