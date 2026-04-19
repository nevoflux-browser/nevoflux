/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Artifact card component for displaying canvas artifacts in chat

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::{Message, MessageContent, ArtifactState};

/// Artifact card component displaying a canvas artifact preview
#[component]
pub fn ArtifactCard(message: Message) -> Element {
    let data = match &message.content {
        MessageContent::Artifact(data) => data.clone(),
        _ => return rsx! {},
    };

    let is_streaming = data.state == ArtifactState::Streaming;
    let artifact_id = data.id.clone();
    let artifact_id_for_pin = data.id.clone();
    let is_persistent = data.is_persistent;

    // Access the messages signal so the pin handler can flip is_persistent locally.
    let ctx = use_app_context();

    let handle_click = move |_| {
        let id = artifact_id.clone();
        spawn(async move {
            if let Err(e) = crate::messaging::send_open_artifact(&id).await {
                tracing::error!("Failed to open artifact: {}", e);
            }
        });
    };

    let type_icon = match data.content_type.as_str() {
        "text/html" | "html" | "image/svg+xml" | "svg" => "\u{1F310}",
        "text/markdown" | "markdown" => "\u{1F4DD}",
        "application/json" | "json" => "\u{1F4C4}",
        "react" => "\u{269B}",
        "mermaid" => "\u{1F4CA}",
        "project" => "\u{1F4E6}",
        _ => "\u{1F4C4}",
    };

    // Short display label from MIME type
    let type_label = match data.content_type.as_str() {
        "text/html" => "HTML",
        "text/markdown" => "Markdown",
        "image/svg+xml" => "SVG",
        "application/json" => "JSON",
        "project" => "Project",
        other => other,
    };

    let card_class = if is_streaming { "artifact-card streaming" } else { "artifact-card" };

    rsx! {
        div {
            class: "{card_class}",
            onclick: handle_click,
            role: "button",
            tabindex: "0",

            div { class: "artifact-header",
                span { class: "artifact-icon", "{type_icon}" }
                span { class: "artifact-title", "{data.title}" }
                span { class: "artifact-type-badge", "{type_label}" }

                // Pin button: outline when unpinned (clickable), filled when already saved.
                if is_persistent {
                    // Already in My Canvas — non-interactive filled pin.
                    span {
                        class: "artifact-pin-btn pinned",
                        title: "Already in My Canvas",
                        role: "img",
                        aria_label: "Saved to My Canvas",
                        "\u{1F4CD}" // filled location pin emoji
                    }
                } else if !is_streaming {
                    // Not yet saved and artifact is complete — show outline pin.
                    button {
                        class: "artifact-pin-btn",
                        title: "Save to My Canvas",
                        aria_label: "Save to My Canvas",
                        // stop_propagation prevents the parent div's onclick (open canvas)
                        // from firing when the user clicks the pin button.
                        onclick: move |evt| {
                            evt.stop_propagation();
                            let canvas_id = artifact_id_for_pin.clone();
                            let messages_signal = ctx.messages;
                            spawn(async move {
                                crate::messaging::send_save_to_my_canvas(canvas_id, messages_signal).await;
                            });
                        },
                        "\u{1F4CC}" // outline pushpin emoji
                    }
                }
            }

            div { class: "artifact-footer",
                if is_streaming {
                    span { class: "artifact-status streaming",
                        span { class: "artifact-spinner" }
                        "Generating..."
                    }
                } else {
                    span { class: "artifact-status ready", "Ready" }
                }
                span { class: "artifact-open-hint", "Click to open" }
            }
        }
    }
}
