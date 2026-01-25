/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Code block component

use dioxus::prelude::*;

/// Code block component with language label and copy button
#[component]
pub fn CodeBlock(language: String, code: String) -> Element {
    let mut copied = use_signal(|| false);
    let code_for_copy = code.clone();

    let handle_copy = move |_| {
        // Copy to clipboard
        let code_clone = code_for_copy.clone();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&code_clone)
                ).await;
                copied.set(true);

                // Reset after 2 seconds
                gloo::timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
            }
        });
    };

    let copy_text = if copied() { "Copied!" } else { "Copy" };

    rsx! {
        div { class: "code-block",
            // Header with language and copy button
            div { class: "code-header",
                span { class: "code-language", "{language}" }
                button {
                    class: "code-copy-btn",
                    class: if copied() { "copied" },
                    onclick: handle_copy,
                    aria_label: "Copy code",
                    title: "Copy to clipboard",
                    "{copy_text}"
                }
            }

            // Code content
            div { class: "code-content",
                pre {
                    "{code}"
                }
            }
        }
    }
}
