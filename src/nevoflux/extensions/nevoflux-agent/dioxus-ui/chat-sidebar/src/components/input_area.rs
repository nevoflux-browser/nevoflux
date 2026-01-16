/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Input area component with textarea and toolbar

use crate::state::TabContext;
use dioxus::prelude::*;

/// Input area props
#[derive(Props, Clone, PartialEq)]
pub struct InputAreaProps {
    pub disabled: bool,
    pub on_send: EventHandler<String>,
    pub tab_context: TabContext,
}

/// Chat input area with toolbar
#[component]
pub fn InputArea(props: InputAreaProps) -> Element {
    let mut input_value = use_signal(String::new);
    let mut is_composing = use_signal(|| false);

    let can_send = !props.disabled && !input_value.read().trim().is_empty();

    let handle_send = move |_| {
        let text = input_value.read().trim().to_string();
        if !text.is_empty() && !props.disabled {
            props.on_send.call(text);
            input_value.set(String::new());
        }
    };

    let handle_keydown = move |evt: KeyboardEvent| {
        // Send on Enter (without Shift)
        if evt.key() == Key::Enter && !evt.modifiers().shift() && !*is_composing.read() {
            evt.prevent_default();
            let text = input_value.read().trim().to_string();
            if !text.is_empty() && !props.disabled {
                props.on_send.call(text);
                input_value.set(String::new());
            }
        }
    };

    rsx! {
        div {
            class: "input-area",

            // Context section (showing current tab)
            if !props.tab_context.url.is_empty() {
                div {
                    class: "input-context",

                    div {
                        class: "context-info",

                        if let Some(ref favicon) = props.tab_context.favicon_url {
                            img {
                                class: "context-favicon",
                                src: "{favicon}",
                                alt: "",
                            }
                        } else {
                            span {
                                class: "context-favicon-placeholder",
                                "{props.tab_context.title.chars().next().unwrap_or('?')}"
                            }
                        }

                        span {
                            class: "context-title",
                            "{truncate(&props.tab_context.title, 30)}"
                        }

                        span {
                            class: "context-url",
                            "{extract_domain(&props.tab_context.url)}"
                        }
                    }

                    button {
                        class: "context-refresh",
                        title: "Refresh context",
                        onclick: move |_| {
                            tracing::info!("Refresh context clicked");
                        },

                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M11.534 7h3.932a.25.25 0 0 1 .192.41l-1.966 2.36a.25.25 0 0 1-.384 0l-1.966-2.36a.25.25 0 0 1 .192-.41zm-11 2h3.932a.25.25 0 0 0 .192-.41L2.692 6.23a.25.25 0 0 0-.384 0L.342 8.59A.25.25 0 0 0 .534 9z"
                            }
                            path {
                                fill_rule: "evenodd",
                                d: "M8 3c-1.552 0-2.94.707-3.857 1.818a.5.5 0 1 1-.771-.636A6.002 6.002 0 0 1 13.917 7H12.9A5.002 5.002 0 0 0 8 3zM3.1 9a5.002 5.002 0 0 0 8.757 2.182.5.5 0 1 1 .771.636A6.002 6.002 0 0 1 2.083 9H3.1z"
                            }
                        }
                    }
                }
            }

            // Input section
            div {
                class: "input-section",

                textarea {
                    class: "input-textarea",
                    class: if props.disabled { "disabled" } else { "" },
                    placeholder: if props.disabled { "Connecting..." } else { "Type your message..." },
                    disabled: props.disabled,
                    value: "{input_value}",
                    oninput: move |evt| {
                        input_value.set(evt.value());
                    },
                    onkeydown: handle_keydown,
                    oncompositionstart: move |_| {
                        is_composing.set(true);
                    },
                    oncompositionend: move |_| {
                        is_composing.set(false);
                    },
                }
            }

            // Toolbar section
            div {
                class: "input-toolbar",

                div {
                    class: "toolbar-left",

                    // Attach file button
                    button {
                        class: "toolbar-btn",
                        title: "Attach file",
                        disabled: props.disabled,

                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M4.5 3a2.5 2.5 0 0 1 5 0v9a1.5 1.5 0 0 1-3 0V5a.5.5 0 0 1 1 0v7a.5.5 0 0 0 1 0V3a1.5 1.5 0 1 0-3 0v9a2.5 2.5 0 0 0 5 0V5a.5.5 0 0 1 1 0v7a3.5 3.5 0 1 1-7 0V3z"
                            }
                        }
                    }

                    // Screenshot button
                    button {
                        class: "toolbar-btn",
                        title: "Take screenshot",
                        disabled: props.disabled,

                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M10.5 8.5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z"
                            }
                            path {
                                d: "M2 4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-1.172a2 2 0 0 1-1.414-.586l-.828-.828A2 2 0 0 0 9.172 2H6.828a2 2 0 0 0-1.414.586l-.828.828A2 2 0 0 1 3.172 4H2zm.5 2a.5.5 0 1 1 0-1 .5.5 0 0 1 0 1zm9 2.5a3.5 3.5 0 1 1-7 0 3.5 3.5 0 0 1 7 0z"
                            }
                        }
                    }

                    // Voice input button
                    button {
                        class: "toolbar-btn",
                        title: "Voice input",
                        disabled: props.disabled,

                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M3.5 6.5A.5.5 0 0 1 4 7v1a4 4 0 0 0 8 0V7a.5.5 0 0 1 1 0v1a5 5 0 0 1-4.5 4.975V15h3a.5.5 0 0 1 0 1h-7a.5.5 0 0 1 0-1h3v-2.025A5 5 0 0 1 3 8V7a.5.5 0 0 1 .5-.5z"
                            }
                            path {
                                d: "M10 8a2 2 0 1 1-4 0V3a2 2 0 1 1 4 0v5zM8 0a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V3a3 3 0 0 0-3-3z"
                            }
                        }
                    }
                }

                div {
                    class: "toolbar-right",

                    // Send button
                    button {
                        class: "send-btn",
                        class: if can_send { "active" } else { "" },
                        disabled: !can_send,
                        onclick: handle_send,

                        span {
                            class: "send-text",
                            "Send"
                        }

                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 16 16",
                            fill: "currentColor",

                            path {
                                d: "M15.854.146a.5.5 0 0 1 .11.54l-5.819 14.547a.75.75 0 0 1-1.329.124l-3.178-4.995L.643 7.184a.75.75 0 0 1 .124-1.33L15.314.037a.5.5 0 0 1 .54.11ZM6.636 10.07l2.761 4.338L14.13 2.576 6.636 10.07Zm6.787-8.201L1.591 6.602l4.339 2.76 7.494-7.493Z"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Truncate string with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Extract domain from URL
fn extract_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or(url)
        .to_string()
}
