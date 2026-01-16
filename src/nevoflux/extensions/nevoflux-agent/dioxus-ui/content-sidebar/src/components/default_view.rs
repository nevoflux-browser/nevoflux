/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Default view showing current URL

use dioxus::prelude::*;

/// Default view props
#[derive(Props, Clone, PartialEq)]
pub struct DefaultViewProps {
    pub url: String,
    pub title: String,
}

/// Default view - displays current browser URL
#[component]
pub fn DefaultView(props: DefaultViewProps) -> Element {
    let domain = extract_domain(&props.url);
    let protocol = extract_protocol(&props.url);

    rsx! {
        div {
            class: "default-view",

            // URL display section
            div {
                class: "url-display",

                // Protocol badge
                span {
                    class: "protocol-badge",
                    class: if protocol == "https" { "secure" } else { "" },
                    "{protocol}://"
                }

                // Domain
                span {
                    class: "domain",
                    "{domain}"
                }
            }

            // Page title
            if !props.title.is_empty() {
                p {
                    class: "page-title",
                    "{props.title}"
                }
            }

            // Status indicator
            div {
                class: "status-section",

                div {
                    class: "status-indicator active",
                }

                span {
                    class: "status-text",
                    "Monitoring page"
                }
            }

            // Quick actions
            div {
                class: "quick-actions",

                button {
                    class: "action-btn",
                    title: "Summarize page",
                    onclick: move |_| {
                        tracing::info!("Summarize clicked");
                    },

                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 16 16",
                        fill: "currentColor",
                        path {
                            d: "M2.5 3A1.5 1.5 0 0 0 1 4.5v.793c.026.009.051.02.076.032L7.674 8.51c.206.1.446.1.652 0l6.598-3.185A.755.755 0 0 1 15 5.293V4.5A1.5 1.5 0 0 0 13.5 3h-11Z"
                        }
                    }
                    span { "Summarize" }
                }

                button {
                    class: "action-btn",
                    title: "Extract data",
                    onclick: move |_| {
                        tracing::info!("Extract clicked");
                    },

                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 16 16",
                        fill: "currentColor",
                        path {
                            d: "M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6z"
                        }
                    }
                    span { "Extract" }
                }
            }
        }
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

/// Extract protocol from URL
fn extract_protocol(url: &str) -> String {
    url.split("://")
        .next()
        .unwrap_or("http")
        .to_lowercase()
}
