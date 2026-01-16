/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message bubble component for displaying chat messages

use crate::state::{ChatMessage, MessageRole, MessageStatus, StreamingMessage};
use dioxus::prelude::*;

/// Message bubble props
#[derive(Props, Clone, PartialEq)]
pub struct MessageBubbleProps {
    pub message: ChatMessage,
    #[props(default = false)]
    pub is_streaming: bool,
}

/// Individual chat message bubble
#[component]
pub fn MessageBubble(props: MessageBubbleProps) -> Element {
    let role_class = match props.message.role {
        MessageRole::User => "message-user",
        MessageRole::Assistant => "message-assistant",
        MessageRole::System => "message-system",
    };

    let status_class = match props.message.status {
        MessageStatus::Sent => "status-sent",
        MessageStatus::Delivered => "status-delivered",
        MessageStatus::Error => "status-error",
    };

    rsx! {
        div {
            class: "message-bubble {role_class} {status_class}",
            key: "{props.message.id}",

            // Message content
            div {
                class: "message-content",

                // Render markdown for assistant messages
                if props.message.role == MessageRole::Assistant {
                    MarkdownContent {
                        content: props.message.content.clone(),
                    }
                } else {
                    p {
                        class: "message-text",
                        "{props.message.content}"
                    }
                }

                // Streaming cursor
                if props.is_streaming {
                    span {
                        class: "streaming-cursor",
                    }
                }
            }

            // Message footer
            div {
                class: "message-footer",

                // Timestamp
                span {
                    class: "message-timestamp",
                    "{format_timestamp(props.message.timestamp)}"
                }

                // Copy button for assistant messages
                if props.message.role == MessageRole::Assistant {
                    button {
                        class: "copy-btn",
                        title: "Copy message",
                        onclick: move |_| {
                            copy_to_clipboard(&props.message.content);
                        },

                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M4 1.5H3a2 2 0 0 0-2 2V14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V3.5a2 2 0 0 0-2-2h-1v1h1a1 1 0 0 1 1 1V14a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1h1v-1z"
                            }
                            path {
                                d: "M9.5 1a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h3zm-3-1A1.5 1.5 0 0 0 5 1.5v1A1.5 1.5 0 0 0 6.5 4h3A1.5 1.5 0 0 0 11 2.5v-1A1.5 1.5 0 0 0 9.5 0h-3z"
                            }
                        }
                    }
                }

                // Error indicator
                if props.message.status == MessageStatus::Error {
                    span {
                        class: "error-indicator",
                        title: "Failed to send",
                        "!"
                    }
                }
            }
        }
    }
}

/// Streaming message bubble (displays in-progress stream)
#[derive(Props, Clone, PartialEq)]
pub struct StreamingBubbleProps {
    pub stream: StreamingMessage,
}

#[component]
pub fn StreamingBubble(props: StreamingBubbleProps) -> Element {
    rsx! {
        div {
            class: "message-bubble message-assistant streaming",
            key: "streaming-{props.stream.stream_id}",

            div {
                class: "message-content",

                MarkdownContent {
                    content: props.stream.content.clone(),
                }

                if !props.stream.is_complete {
                    span {
                        class: "streaming-cursor",
                    }
                }
            }
        }
    }
}

/// Simple markdown content renderer
#[derive(Props, Clone, PartialEq)]
pub struct MarkdownContentProps {
    pub content: String,
}

#[component]
fn MarkdownContent(props: MarkdownContentProps) -> Element {
    // Basic markdown parsing - in production, use a proper markdown library
    let rendered = render_markdown(&props.content);

    rsx! {
        div {
            class: "markdown-content",
            dangerous_inner_html: "{rendered}",
        }
    }
}

/// Basic markdown to HTML conversion
fn render_markdown(md: &str) -> String {
    let mut html = html_escape(md);

    // Code blocks
    html = regex_replace(&html, r"```(\w*)\n([\s\S]*?)```", |caps: &[&str]| {
        let lang = caps.get(1).unwrap_or(&"");
        let code = caps.get(2).unwrap_or(&"");
        format!(
            r#"<pre class="code-block" data-lang="{}"><code>{}</code></pre>"#,
            lang, code
        )
    });

    // Inline code
    html = regex_replace(&html, r"`([^`]+)`", |caps: &[&str]| {
        format!("<code class=\"inline-code\">{}</code>", caps.get(1).unwrap_or(&""))
    });

    // Bold
    html = regex_replace(&html, r"\*\*([^*]+)\*\*", |caps: &[&str]| {
        format!("<strong>{}</strong>", caps.get(1).unwrap_or(&""))
    });

    // Italic
    html = regex_replace(&html, r"\*([^*]+)\*", |caps: &[&str]| {
        format!("<em>{}</em>", caps.get(1).unwrap_or(&""))
    });

    // Headers
    html = regex_replace(&html, r"^### (.+)$", |caps: &[&str]| {
        format!("<h3>{}</h3>", caps.get(1).unwrap_or(&""))
    });
    html = regex_replace(&html, r"^## (.+)$", |caps: &[&str]| {
        format!("<h2>{}</h2>", caps.get(1).unwrap_or(&""))
    });
    html = regex_replace(&html, r"^# (.+)$", |caps: &[&str]| {
        format!("<h1>{}</h1>", caps.get(1).unwrap_or(&""))
    });

    // Links
    html = regex_replace(&html, r"\[([^\]]+)\]\(([^)]+)\)", |caps: &[&str]| {
        format!(
            r#"<a href="{}" target="_blank" rel="noopener">{}</a>"#,
            caps.get(2).unwrap_or(&""),
            caps.get(1).unwrap_or(&"")
        )
    });

    // Line breaks
    html = html.replace('\n', "<br>");

    html
}

/// Basic HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Simple regex replacement (basic implementation for WASM)
fn regex_replace<F>(text: &str, _pattern: &str, _replacement: F) -> String
where
    F: Fn(&[&str]) -> String,
{
    // Note: In production, use wasm-compatible regex crate
    // For now, return text unchanged - actual regex will be handled by JS
    text.to_string()
}

/// Format timestamp to HH:MM
fn format_timestamp(ts: u64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64));
    format!(
        "{:02}:{:02}",
        date.get_hours(),
        date.get_minutes()
    )
}

/// Copy text to clipboard
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let _ = clipboard.write_text(text);
    }
}
