/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Content view for displaying received content from Chat Sidebar

use crate::state::DisplayContent;
use dioxus::prelude::*;
use shared_protocol::DisplayContentType;

/// Content view props
#[derive(Props, Clone, PartialEq)]
pub struct ContentViewProps {
    pub content: DisplayContent,
    pub on_close: EventHandler<()>,
}

/// Content view - displays content from Chat Sidebar
#[component]
pub fn ContentView(props: ContentViewProps) -> Element {
    rsx! {
        div {
            class: "content-view",

            // Header with title and close button
            div {
                class: "content-header",

                h3 {
                    class: "content-title",
                    "{props.content.title.as_deref().unwrap_or(\"Content\")}"
                }

                button {
                    class: "close-btn",
                    title: "Close",
                    onclick: move |_| {
                        props.on_close.call(());
                    },

                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 16 16",
                        fill: "currentColor",
                        path {
                            d: "M4.646 4.646a.5.5 0 0 1 .708 0L8 7.293l2.646-2.647a.5.5 0 0 1 .708.708L8.707 8l2.647 2.646a.5.5 0 0 1-.708.708L8 8.707l-2.646 2.647a.5.5 0 0 1-.708-.708L7.293 8 4.646 5.354a.5.5 0 0 1 0-.708z"
                        }
                    }
                }
            }

            // Content body
            div {
                class: "content-body",

                match &props.content.content_type {
                    DisplayContentType::Markdown => rsx! {
                        MarkdownContent {
                            content: props.content.content.clone(),
                        }
                    },
                    DisplayContentType::Html => rsx! {
                        div {
                            class: "html-content",
                            dangerous_inner_html: "{sanitize_html(&props.content.content)}",
                        }
                    },
                    DisplayContentType::Text => rsx! {
                        pre {
                            class: "text-content",
                            "{props.content.content}"
                        }
                    },
                    DisplayContentType::Json => rsx! {
                        JsonContent {
                            content: props.content.content.clone(),
                        }
                    },
                    DisplayContentType::Code { language } => rsx! {
                        CodeContent {
                            content: props.content.content.clone(),
                            language: language.clone(),
                        }
                    },
                    DisplayContentType::Iframe => rsx! {
                        iframe {
                            class: "iframe-content",
                            src: "{props.content.content}",
                            "sandbox": "allow-scripts allow-same-origin",
                        }
                    },
                }
            }

            // Footer with actions
            div {
                class: "content-footer",

                button {
                    class: "footer-btn",
                    title: "Copy content",
                    onclick: move |_| {
                        copy_to_clipboard(&props.content.content);
                    },

                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 16 16",
                        fill: "currentColor",
                        path {
                            d: "M4 1.5H3a2 2 0 0 0-2 2V14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V3.5a2 2 0 0 0-2-2h-1v1h1a1 1 0 0 1 1 1V14a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1h1v-1z"
                        }
                    }
                    span { "Copy" }
                }
            }
        }
    }
}

/// Markdown content renderer
#[derive(Props, Clone, PartialEq)]
pub struct MarkdownContentProps {
    pub content: String,
}

#[component]
fn MarkdownContent(props: MarkdownContentProps) -> Element {
    let html = render_markdown(&props.content);

    rsx! {
        div {
            class: "markdown-content",
            dangerous_inner_html: "{html}",
        }
    }
}

/// JSON content renderer (tree view)
#[derive(Props, Clone, PartialEq)]
pub struct JsonContentProps {
    pub content: String,
}

#[component]
fn JsonContent(props: JsonContentProps) -> Element {
    let formatted = match serde_json::from_str::<serde_json::Value>(&props.content) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or(props.content.clone()),
        Err(_) => props.content.clone(),
    };

    rsx! {
        pre {
            class: "json-content",
            code {
                "{formatted}"
            }
        }
    }
}

/// Code content renderer with syntax highlighting
#[derive(Props, Clone, PartialEq)]
pub struct CodeContentProps {
    pub content: String,
    pub language: String,
}

#[component]
fn CodeContent(props: CodeContentProps) -> Element {
    rsx! {
        div {
            class: "code-content",

            div {
                class: "code-header",
                span {
                    class: "language-badge",
                    "{props.language}"
                }
            }

            pre {
                code {
                    class: "language-{props.language}",
                    "{props.content}"
                }
            }
        }
    }
}

/// Basic markdown to HTML conversion
fn render_markdown(md: &str) -> String {
    let mut html = html_escape(md);

    // Code blocks
    html = simple_replace(&html, "```", "<pre><code>", "</code></pre>");

    // Bold
    html = html.replace("**", "<strong>").replace("</strong><strong>", "");

    // Headers
    html = html.lines()
        .map(|line| {
            if line.starts_with("### ") {
                format!("<h3>{}</h3>", &line[4..])
            } else if line.starts_with("## ") {
                format!("<h2>{}</h2>", &line[3..])
            } else if line.starts_with("# ") {
                format!("<h1>{}</h1>", &line[2..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("<br>");

    html
}

/// Simple paired replacement
fn simple_replace(text: &str, marker: &str, open_tag: &str, close_tag: &str) -> String {
    let parts: Vec<&str> = text.split(marker).collect();
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        result.push_str(part);
        if i < parts.len() - 1 {
            if i % 2 == 0 {
                result.push_str(open_tag);
            } else {
                result.push_str(close_tag);
            }
        }
    }
    result
}

/// Basic HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Basic HTML sanitization (very basic - production would use a proper sanitizer)
fn sanitize_html(html: &str) -> String {
    // Remove script tags
    let mut result = html.to_string();

    // Very basic sanitization - in production use a proper library
    result = result.replace("<script", "<!-- script");
    result = result.replace("</script>", "script -->");
    result = result.replace("javascript:", "");
    result = result.replace("onerror=", "data-removed=");
    result = result.replace("onclick=", "data-removed=");

    result
}

/// Copy text to clipboard
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let _ = clipboard.write_text(text);
    }
}
