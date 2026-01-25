/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message list component

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::StreamingState;
use super::MessageBubble;

/// Message list component showing all chat messages
#[component]
pub fn MessageList() -> Element {
    let ctx = use_app_context();
    let messages_signal = ctx.messages;
    let streaming_signal = ctx.streaming;

    let messages = messages_signal.read();
    let streaming = streaming_signal.read();

    // Find the index of the last user message
    let last_user_index = messages.iter().enumerate()
        .filter(|(_, m)| m.role == crate::state::MessageRole::User)
        .map(|(i, _)| i)
        .last();

    // Auto-scroll to bottom when messages change or streaming updates
    use_effect(move || {
        // Read signals inside effect to track changes
        let msg_count = messages_signal.read().len();
        let stream_len = streaming_signal.read().as_ref().map(|s| s.content.len()).unwrap_or(0);

        // Log for debugging
        tracing::debug!("Scroll trigger: messages={}, stream_len={}", msg_count, stream_len);

        // Use spawn to scroll after a small delay for DOM to update
        spawn(async move {
            // Small delay to ensure DOM is rendered
            gloo::timers::future::TimeoutFuture::new(50).await;
            scroll_to_bottom();
        });
    });

    rsx! {
        div { class: "message-list",
            // Historical messages
            for (index, msg) in messages.iter().enumerate() {
                MessageBubble {
                    key: "{msg.id}",
                    message: msg.clone(),
                    is_last_user: last_user_index == Some(index),
                    message_index: index,
                }
            }

            // Currently streaming message
            if let Some(ref stream) = *streaming {
                StreamingBubble { stream: stream.clone() }
            }

            // Scroll anchor
            div { class: "scroll-anchor", id: "message-scroll-anchor" }
        }
    }
}

/// Scroll the message list to the bottom using scrollIntoView on anchor
fn scroll_to_bottom() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            // Try scrollIntoView on the scroll anchor
            if let Some(anchor) = document.get_element_by_id("message-scroll-anchor") {
                // Use scrollIntoView with smooth behavior
                let options = web_sys::ScrollIntoViewOptions::new();
                options.set_behavior(web_sys::ScrollBehavior::Smooth);
                options.set_block(web_sys::ScrollLogicalPosition::End);
                anchor.scroll_into_view_with_scroll_into_view_options(&options);
            } else if let Some(element) = document.query_selector(".message-list").ok().flatten() {
                // Fallback: set scroll top directly
                let scroll_height = element.scroll_height();
                element.set_scroll_top(scroll_height);
            }
        }
    }
}

/// Streaming message bubble with cursor animation
#[component]
fn StreamingBubble(stream: StreamingState) -> Element {
    rsx! {
        div {
            class: "message-bubble assistant streaming",
            aria_live: "polite",
            aria_atomic: "false",

            div { class: "bubble-content",
                "{stream.content}"
                span { class: "cursor", aria_hidden: "true" }
            }
        }
    }
}
