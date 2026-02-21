/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Welcome screen for empty chat state

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::context::use_app_context;
use crate::state::SessionSummary;

/// Welcome screen component shown when chat is empty
#[component]
pub fn WelcomeScreen() -> Element {
    let ctx = use_app_context();
    let history = ctx.history.read();
    let has_history = history.has_sessions();
    rsx! {
        div { class: "welcome-screen",
            // Logo - embedded SVG (cleaned, without outer white rectangle)
            div {
                class: "welcome-logo",
                dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 476.97 476.97"><defs><style>.cls-1{{fill:#fff;}}.cls-2{{fill:#006934;}}</style></defs><rect class="cls-2" width="476.97" height="476.97"/><path class="cls-1" d="M294.82,268.82c17.6-31.79,35.12-63.41,52.79-95.31l-30.47-17.27,144.34-66.67.61.73c-.68,1.25-1.35,2.5-2,3.74q-26.61,47.85-53.25,95.7c-11.51,20.62-23.18,41.15-34.63,61.81-11.93,21.53-23.6,43.21-35.54,64.74-4.59,8.28-9.13,16.66-14.54,24.39-10.83,15.48-35.52,17.63-49.51,5-7.13-6.45-11.48-14.75-16.16-22.87q-18.72-32.52-37.4-65.08c-1.94-3.37-3.66-6.86-5.52-10.26-.56-1-1.26-2-2.14-3.33-1.14,1.88-2.12,3.36-3,4.9-12.17,22.09-24.39,44.15-36.47,66.29-8.3,15.23-16.35,30.59-24.61,45.85q-8.25,15.25-16.83,30.35a4.73,4.73,0,0,1-3.33,2.19c-20.87.08-41.74,0-62.61-.07-1,0-1.92-.09-3.37-.17,2.56-4.62,4.92-8.84,7.25-13.08q20.8-37.91,41.6-75.82,20.91-38.16,41.79-76.35c10.39-19,20.54-38.09,31.23-56.9s34.12-22.4,49.8-7.69c3.75,3.51,6.28,8.46,8.9,13,15.52,27,30.88,54,46.32,81.05C290.13,261.38,292.44,264.99,294.82,268.82Zm78.81-102.36c-26.36,47.48-52.51,94.58-78.92,142.17-2.27-3.88-4.26-7.19-6.17-10.54q-14.18-24.94-28.33-49.9-15-26.4-30-52.76c-3.27-5.71-6.41-11.56-10.35-16.78-5.06-6.71-13.5-6.44-18.69.06a37.26,37.26,0,0,0-3.41,5.26q-24.9,45.81-49.85,91.63c-10.79,19.78-21.71,39.49-32.55,59.24q-9.22,16.8-18.36,33.61c-1,1.86-2,3.76-3.13,6,7.13,0,13.72-.16,20.29.08,2.75.1,4.09-1,5.3-3.22,5.15-9.58,10.45-19.06,15.67-28.6q13.71-25.05,27.41-50.12,21.63-39.48,43.3-78.93c1.65-3,3.46-5.91,5.41-9.22.79,1.3,1.32,2.13,1.81,3q15,26.37,30.08,52.76c6.87,12,13.71,24.09,20.63,36.1,5.9,10.52,11.9,17.38,18.16,27.78,4.25,7,12.76,9.51,19.48,5.49a24.84,24.84,0,0,0,8.1-8.88c11.65-20.44,23-41.05,34.44-61.63q19-34.26,38-68.55,16.7-30.09,33.44-60.17c.53-1,.89-2,1.62-3.66l-59,27.6Z"/><path class="cls-1" d="M14.1,393.66c1.44-2.43,2.47-4.08,3.41-5.78q18.87-34,37.71-68.05c6.58-11.86,13.25-23.67,19.79-35.55,9.65-17.55,19.15-35.18,28.84-52.71,15.15-27.4,30.19-54.87,45.73-82.06a65.68,65.68,0,0,1,27-25.62c17.38-9.14,35.61-11.23,54.3-6,18.16,5.06,32.61,15.49,42,32.42,10.18,18.42,20.67,36.66,31.12,54.93a4.31,4.31,0,0,1-.08,5.08c-3.13,5.2-6,10.57-9.16,16.26-4-6.71-7.94-13-11.68-19.45-9.84-16.89-19-34.21-29.59-50.61-9.34-14.47-23.62-21.63-41-22.09-21.5-.57-37.2,9.21-47.62,27.53-12.42,21.83-24.41,43.92-36.59,65.9-7.3,13.15-14.67,26.26-21.91,39.45-8.68,15.81-17.22,31.7-25.89,47.52q-15.81,28.86-31.73,57.64c-3.56,6.46-7.09,12.95-10.76,19.35a3.77,3.77,0,0,1-2.57,1.77C28.66,393.72,21.89,393.66,14.1,393.66Z"/><path class="cls-1" d="M211.53,285.23c2.84,5.07,5.4,9.67,8,14.25q12.6,22.29,25.21,44.55c12.81,22.55,34.1,33.5,59.72,29.69a50.65,50.65,0,0,0,36.53-23.74c7.49-11.88,14.14-24.29,21-36.55,10.6-18.86,21.06-37.79,31.61-56.68,4.23-7.56,8.41-15.16,12.9-22.57,1.86-3.07,2.34-5.74.86-9.13a20.12,20.12,0,0,1-.27-16.57c4.78-11,19.26-16.9,29.31-12.3,14.46,6.62,16.74,22.87,10.92,33.07-4.2,7.37-11.2,9.87-19,10.42a5.73,5.73,0,0,0-5.32,3.34q-17.78,32.21-35.69,64.32c-9.33,16.72-18.56,33.5-28.18,50.05-8.08,13.9-19.93,23.78-34.66,30.11-24.37,10.49-47.65,7.56-70.12-5.84-12.35-7.36-20.73-18.22-27.73-30.37s-13.71-24.1-20.57-36.14c-1.77-3.12-3.65-6.18-5.35-9.33a2.84,2.84,0,0,1,0-2.35C204.1,297.52,207.67,291.65,211.53,285.23Zm216.4-75.43a7.64,7.64,0,0,0-7.55,7.62,7.54,7.54,0,0,0,7.12,7.4,7.36,7.36,0,0,0,7.85-7.25A7.68,7.68,0,0,0,427.93,209.8Z"/><path class="cls-1" d="M427.93,209.8a7.68,7.68,0,0,1,7.42,7.77,7.36,7.36,0,0,1-7.85,7.25,7.54,7.54,0,0,1-7.12-7.4A7.64,7.64,0,0,1,427.93,209.8Z"/></svg>"#,
            }

            // Title
            h2 { class: "welcome-title", "How can I assist?" }

            // Description
            p { class: "welcome-subtitle",
                "Your AI assistant for browsing, research, and productivity."
            }

            // History section (shown when there are past sessions)
            if has_history {
                HistorySection {}
            }
        }
    }
}

/// History section showing recent sessions
#[component]
fn HistorySection() -> Element {
    let mut ctx = use_app_context();
    let history = ctx.history.read();
    let total = history.total;

    // Sort by updated_at descending to show most recent first
    let mut sorted: Vec<&SessionSummary> = history.sessions.iter().collect();
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let recent: Vec<SessionSummary> = sorted.into_iter().take(3).cloned().collect();

    let has_more = total > 3 || history.sessions.len() > 3;

    let handle_view_all = move |_| {
        ctx.show_history_panel.set(true);
        // Refresh history when opening panel
        ctx.history.write().set_loading();
        spawn_local(async move {
            let _ = crate::messaging::send_session_list(50, 0).await;
        });
    };

    rsx! {
        div { class: "history-section",
            h3 { class: "history-title", "Recent conversations" }

            if history.loading {
                div { class: "history-loading",
                    span { class: "loading-spinner" }
                    span { "Loading..." }
                }
            } else if let Some(ref error) = history.error {
                div { class: "history-error",
                    span { "Failed to load history: {error}" }
                }
            } else {
                div { class: "history-list",
                    for session in recent.iter() {
                        HistoryItem { session: session.clone() }
                    }
                }

                if has_more {
                    button {
                        class: "history-view-all",
                        onclick: handle_view_all,
                        "View all conversations"
                    }
                }
            }
        }
    }
}

/// Single history item
#[component]
fn HistoryItem(session: SessionSummary) -> Element {
    let ctx = use_app_context();
    let session_id = session.id.clone();
    let display_title = session.display_title();
    let relative_time = session.relative_time();
    let message_count = session.message_count;

    let handle_click = move |_| {
        let source_id = session_id.clone();
        tracing::info!("Loading historical session: {}", source_id);
        spawn_local(async move {
            if let Err(e) = crate::messaging::send_session_resolve(&source_id).await {
                tracing::error!("Failed to load session: {}", e);
            }
        });
    };

    rsx! {
        button {
            class: "history-item",
            onclick: handle_click,
            aria_label: "Restore conversation: {display_title}",

            div { class: "history-item-content",
                span { class: "history-item-title", "{display_title}" }
                div { class: "history-item-meta",
                    span { class: "history-item-time", "{relative_time}" }
                    span { class: "history-item-count", "{message_count} messages" }
                }
            }

            // Arrow icon
            svg {
                class: "history-item-arrow",
                width: "16",
                height: "16",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M9 18l6-6-6-6" }
            }
        }
    }
}

