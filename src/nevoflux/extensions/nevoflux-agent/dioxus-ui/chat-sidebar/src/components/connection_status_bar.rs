/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Connection status bar component
//!
//! Shown at the top of the sidebar to indicate agent connection status.
//! Hidden when connected; shows spinner when connecting/reconnecting;
//! shows error with retry button on error or disconnect.

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::ConnectionState;

/// Thin status bar rendered at the top of the sidebar for non-first-launch sessions.
///
/// - `Connected` → renders nothing
/// - `Connecting` → spinner + "Connecting to Agent..."
/// - `Reconnecting` → spinner + "Reconnecting to Agent (attempt N)..."
/// - `Error` → error message + Retry button
/// - `Disconnected` → "Agent disconnected" + Retry button
#[component]
pub fn ConnectionStatusBar() -> Element {
    let ctx = use_app_context();
    let connection = ctx.connection.read();

    match &*connection {
        ConnectionState::Connected => rsx! {},

        ConnectionState::Connecting => {
            rsx! {
                div {
                    class: "connection-status-bar connecting",
                    role: "status",
                    aria_live: "polite",
                    span { class: "connection-status-spinner" }
                    span { class: "connection-status-text", "Connecting to Agent..." }
                }
            }
        }

        ConnectionState::Reconnecting { attempt, .. } => {
            let attempt = *attempt;
            rsx! {
                div {
                    class: "connection-status-bar reconnecting",
                    role: "status",
                    aria_live: "polite",
                    span { class: "connection-status-spinner" }
                    span {
                        class: "connection-status-text",
                        "Reconnecting to Agent (attempt {attempt})..."
                    }
                }
            }
        }

        ConnectionState::Error { message } => {
            let message = message.clone();
            rsx! {
                div {
                    class: "connection-status-bar error",
                    role: "alert",
                    aria_live: "assertive",
                    span { class: "connection-status-text", "{message}" }
                    RetryButton {}
                }
            }
        }

        ConnectionState::Disconnected => {
            rsx! {
                div {
                    class: "connection-status-bar disconnected",
                    role: "status",
                    aria_live: "polite",
                    span { class: "connection-status-text", "Agent disconnected" }
                    RetryButton {}
                }
            }
        }
    }
}

/// Retry button that triggers a new connection attempt
#[component]
fn RetryButton() -> Element {
    let handle_retry = move |_| {
        spawn(async move {
            let _ = crate::messaging::request_connect().await;
        });
    };

    rsx! {
        button {
            class: "connection-status-retry-btn",
            onclick: handle_retry,
            aria_label: "Retry connection",
            "Retry"
        }
    }
}
