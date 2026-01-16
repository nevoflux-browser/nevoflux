/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message list component displaying conversation history

use crate::components::message_bubble::{MessageBubble, StreamingBubble};
use crate::state::{ChatMessage, StreamingMessage};
use dioxus::prelude::*;

/// Message list props
#[derive(Props, Clone, PartialEq)]
pub struct MessageListProps {
    pub messages: Vec<ChatMessage>,
    pub streaming_message: Option<StreamingMessage>,
}

/// Scrollable message list with auto-scroll
#[component]
pub fn MessageList(props: MessageListProps) -> Element {
    let mut list_ref = use_signal(|| None::<web_sys::Element>);

    // Auto-scroll to bottom when messages change
    use_effect(move || {
        if let Some(element) = list_ref.read().as_ref() {
            element.set_scroll_top(element.scroll_height());
        }
    });

    rsx! {
        div {
            class: "message-list-container",

            div {
                class: "message-list",
                id: "message-list",
                onmounted: move |evt| {
                    if let Some(el) = evt.data().downcast::<web_sys::Element>() {
                        list_ref.set(Some(el.clone()));
                    }
                },

                // Welcome message if empty
                if props.messages.is_empty() && props.streaming_message.is_none() {
                    WelcomeMessage {}
                }

                // Render messages
                for message in props.messages.iter() {
                    MessageBubble {
                        message: message.clone(),
                        is_streaming: false,
                    }
                }

                // Render streaming message if present
                if let Some(stream) = props.streaming_message.as_ref() {
                    StreamingBubble {
                        stream: stream.clone(),
                    }
                }
            }
        }
    }
}

/// Welcome message shown when chat is empty
#[component]
fn WelcomeMessage() -> Element {
    rsx! {
        div {
            class: "welcome-message",

            div {
                class: "welcome-icon",

                svg {
                    width: "48",
                    height: "48",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.5",

                    path {
                        d: "M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"
                    }
                }
            }

            h2 {
                class: "welcome-title",
                "Welcome to NevoFlux Agent"
            }

            p {
                class: "welcome-text",
                "I'm your AI-powered browser assistant. I can help you navigate the web, "
                "extract information, and automate tasks. Just type a message to get started!"
            }

            div {
                class: "welcome-suggestions",

                SuggestionChip {
                    text: "Summarize this page",
                }
                SuggestionChip {
                    text: "Find all links",
                }
                SuggestionChip {
                    text: "Extract contact info",
                }
            }
        }
    }
}

/// Clickable suggestion chip
#[derive(Props, Clone, PartialEq)]
struct SuggestionChipProps {
    text: &'static str,
}

#[component]
fn SuggestionChip(props: SuggestionChipProps) -> Element {
    rsx! {
        button {
            class: "suggestion-chip",
            onclick: move |_| {
                tracing::info!("Suggestion clicked: {}", props.text);
                // TODO: Trigger message send
            },

            "{props.text}"
        }
    }
}
