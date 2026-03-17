/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Error card component for displaying errors in chat

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::{Message, MessageContent, MessageRole};

/// Error card component
#[component]
pub fn ErrorCard(code: String, message: String, recoverable: bool) -> Element {
    let mut ctx = use_app_context();

    let handle_retry = move |_| {
        // Find last user message text
        let last_user_text = {
            let messages = ctx.messages.read();
            messages.iter().rev()
                .find(|m| m.role == MessageRole::User)
                .and_then(|m| {
                    if let MessageContent::Text(text) = &m.content {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
        };

        if let Some(text) = last_user_text {
            let session_id = ctx.session.read().id.clone();
            let mock_enabled = ctx.mock_enabled;

            // Add new user message
            ctx.messages.write().push(Message::user(&text));
            ctx.agent_status.write().set_thinking();

            wasm_bindgen_futures::spawn_local(async move {
                if mock_enabled {
                    crate::mock::mock_send_message(ctx, text).await;
                } else {
                    let (tab_id, tab_ids) = crate::messaging::build_current_tab_ids().await;
                    let _ = crate::messaging::send_chat_message(&session_id, text, ctx.chat_mode.read().clone(), vec![], vec![], tab_id, tab_ids).await;
                }
            });
        }
    };

    let _handle_dismiss = move |_: MouseEvent| {
        ctx.agent_status.write().hide();
    };

    rsx! {
        div { class: "error-card",
            // Header
            div { class: "error-header",
                span { class: "error-icon", "!" }
                span { class: "error-title", "Request failed" }
            }

            // Body
            div { class: "error-content",
                code { class: "error-code", "{code}" }
                p { class: "error-message", "{message}" }
            }

            // Actions
            if recoverable {
                div { class: "error-actions",
                    button {
                        class: "error-retry",
                        onclick: handle_retry,
                        "Retry"
                    }
                }
            }
        }
    }
}
