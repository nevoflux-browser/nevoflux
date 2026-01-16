/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Status bar component showing connection and activity state

use crate::state::ConnectionStatus;
use dioxus::prelude::*;

/// Status bar props
#[derive(Props, Clone, PartialEq)]
pub struct StatusBarProps {
    pub status: ConnectionStatus,
}

/// Bottom status bar
#[component]
pub fn StatusBar(props: StatusBarProps) -> Element {
    let (status_text, status_class) = match props.status {
        ConnectionStatus::Connected => ("Ready", "status-ready"),
        ConnectionStatus::Connecting => ("Connecting...", "status-connecting"),
        ConnectionStatus::Disconnected => ("Disconnected", "status-disconnected"),
        ConnectionStatus::Error => ("Connection Error", "status-error"),
    };

    rsx! {
        div {
            class: "status-bar {status_class}",

            span {
                class: "status-indicator",
            }

            span {
                class: "status-text",
                "{status_text}"
            }

            // Version info
            span {
                class: "version-info",
                "v0.2.0"
            }
        }
    }
}
