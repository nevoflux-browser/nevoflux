/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Error view component

use dioxus::prelude::*;

/// Error view props
#[derive(Props, Clone, PartialEq)]
pub struct ErrorViewProps {
    pub message: String,
    pub on_dismiss: EventHandler<()>,
}

/// Error view - displays error message with dismiss button
#[component]
pub fn ErrorView(props: ErrorViewProps) -> Element {
    rsx! {
        div {
            class: "error-view",

            div {
                class: "error-icon",

                svg {
                    width: "32",
                    height: "32",
                    view_box: "0 0 16 16",
                    fill: "currentColor",
                    path {
                        d: "M8.982 1.566a1.13 1.13 0 0 0-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 5.995A.905.905 0 0 1 8 5zm.002 6a1 1 0 1 1 0 2 1 1 0 0 1 0-2z"
                    }
                }
            }

            h3 {
                class: "error-title",
                "Something went wrong"
            }

            p {
                class: "error-message",
                "{props.message}"
            }

            button {
                class: "dismiss-btn",
                onclick: move |_| {
                    props.on_dismiss.call(());
                },
                "Dismiss"
            }
        }
    }
}
