/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Artifact card component for displaying canvas artifacts in chat

use dioxus::prelude::*;
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
        _ => "\u{1F4C4}",
    };

    // Short display label from MIME type
    let type_label = match data.content_type.as_str() {
        "text/html" => "HTML",
        "text/markdown" => "Markdown",
        "image/svg+xml" => "SVG",
        "application/json" => "JSON",
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
