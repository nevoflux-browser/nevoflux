/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Header component with connection status and tab context

use crate::state::{ConnectionStatus, TabContext};
use dioxus::prelude::*;

/// Header component props
#[derive(Props, Clone, PartialEq)]
pub struct HeaderProps {
    pub connection_status: ConnectionStatus,
    pub tab_context: TabContext,
}

/// Sidebar header with branding and context info
#[component]
pub fn Header(props: HeaderProps) -> Element {
    let status_class = match props.connection_status {
        ConnectionStatus::Connected => "status-connected",
        ConnectionStatus::Connecting => "status-connecting",
        ConnectionStatus::Disconnected => "status-disconnected",
        ConnectionStatus::Error => "status-error",
    };

    let status_text = match props.connection_status {
        ConnectionStatus::Connected => "Connected",
        ConnectionStatus::Connecting => "Connecting...",
        ConnectionStatus::Disconnected => "Disconnected",
        ConnectionStatus::Error => "Error",
    };

    rsx! {
        header {
            class: "sidebar-header",

            div {
                class: "header-left",

                h1 {
                    class: "header-title",
                    "NevoFlux Agent"
                }

                div {
                    class: "connection-status {status_class}",

                    span {
                        class: "status-indicator",
                    }
                    span {
                        class: "status-text",
                        "{status_text}"
                    }
                }
            }

            div {
                class: "header-right",

                // Tab context indicator
                if !props.tab_context.url.is_empty() {
                    div {
                        class: "tab-context",
                        title: "{props.tab_context.url}",

                        if let Some(ref favicon) = props.tab_context.favicon_url {
                            img {
                                class: "favicon",
                                src: "{favicon}",
                                alt: "",
                            }
                        } else {
                            span {
                                class: "favicon-placeholder",
                                "{props.tab_context.title.chars().next().unwrap_or('?')}"
                            }
                        }

                        span {
                            class: "tab-title",
                            "{truncate(&props.tab_context.title, 20)}"
                        }
                    }
                }

                // Settings button
                button {
                    class: "settings-btn",
                    title: "Settings",
                    onclick: move |_| {
                        tracing::info!("Settings clicked");
                    },

                    // Gear icon SVG
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 16 16",
                        fill: "currentColor",

                        path {
                            d: "M8 4.754a3.246 3.246 0 1 0 0 6.492 3.246 3.246 0 0 0 0-6.492zM5.754 8a2.246 2.246 0 1 1 4.492 0 2.246 2.246 0 0 1-4.492 0z"
                        }
                        path {
                            d: "M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 0 1-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 0 1-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 0 1 .52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 0 1 1.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 0 1 1.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 0 1 .52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 0 1-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 0 1-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 0 0 2.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 0 0 1.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 0 0-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 0 0-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 0 0-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 0 0 1.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 0 0 3.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 0 0 2.692-1.115l.094-.319z"
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
