/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! History panel component for session management
//!
//! Full-panel overlay showing all historical sessions grouped by date,
//! with pinned sessions at the top. Supports restoring, deleting, and
//! creating new sessions.

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::bindings::nevoflux_api;
use crate::context::use_app_context;
use crate::state::SessionSummary;

/// Full-panel history view for session management
///
/// Displays all sessions grouped by date with a pinned section at top.
/// Provides back navigation, new chat creation, session restore, and delete.
#[component]
pub fn HistoryPanel() -> Element {
    let ctx = use_app_context();
    let show = ctx.show_history_panel.read();

    if !*show {
        return rsx! {};
    }

    let history = ctx.history.read();
    let active_session_id = ctx.session.read().id.clone();

    // Separate pinned sessions from unpinned
    let pinned: Vec<&SessionSummary> = history.sessions.iter().filter(|s| s.pinned).collect();
    let unpinned: Vec<&SessionSummary> = history.sessions.iter().filter(|s| !s.pinned).collect();

    // Group unpinned sessions by date
    let groups = group_by_date(&unpinned);

    rsx! {
        div {
            class: "history-panel",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Session history",

            // Header
            HistoryPanelHeader {}

            // Scrollable list area
            div { class: "history-panel-list",
                // Loading state
                if history.loading {
                    div { class: "history-panel-loading",
                        span { class: "loading-spinner" }
                        span { "Loading history..." }
                    }
                } else if let Some(ref error) = history.error {
                    // Error state
                    div { class: "history-panel-error",
                        span { class: "history-error-icon", "!" }
                        span { "Error: {error}" }
                    }
                } else if history.sessions.is_empty() {
                    // Empty state
                    div { class: "history-panel-empty",
                        // Empty chat icon
                        svg {
                            width: "48",
                            height: "48",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                        }
                        p { "No conversations yet" }
                        p { class: "history-empty-hint", "Start a chat to see it here." }
                    }
                } else {
                    // Pinned section
                    if !pinned.is_empty() {
                        div { class: "history-group",
                            div { class: "history-group-header",
                                // Pin icon
                                svg {
                                    width: "12",
                                    height: "12",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    stroke: "none",
                                    path { d: "M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z" }
                                }
                                span { "Pinned" }
                            }
                            for session in pinned.iter() {
                                SessionItem {
                                    key: "{session.id}",
                                    session: (*session).clone(),
                                    active_session_id: active_session_id.clone(),
                                }
                            }
                        }
                    }

                    // Date groups
                    for (group_label, sessions) in groups.iter() {
                        div { class: "history-group",
                            div { class: "history-group-header",
                                span { "{group_label}" }
                            }
                            for session in sessions.iter() {
                                SessionItem {
                                    key: "{session.id}",
                                    session: (*session).clone(),
                                    active_session_id: active_session_id.clone(),
                                }
                            }
                        }
                    }

                    // Load more button
                    if (history.sessions.len() as u32) < history.total {
                        LoadMoreButton {}
                    }
                }
            }
        }
    }
}

/// "Load more" button for paginated history
#[component]
fn LoadMoreButton() -> Element {
    let mut ctx = use_app_context();
    let is_loading = ctx.history.read().loading_more;

    let handle_load_more = move |_| {
        let offset = ctx.history.read().sessions.len() as u32;
        ctx.history.write().set_loading_more();
        spawn_local(async move {
            let _ = crate::messaging::send_session_list(50, offset).await;
        });
    };

    rsx! {
        div { class: "history-load-more",
            if is_loading {
                div { class: "history-panel-loading",
                    span { class: "loading-spinner" }
                    span { "Loading..." }
                }
            } else {
                button {
                    class: "history-load-more-btn",
                    onclick: handle_load_more,
                    "Load more conversations"
                }
            }
        }
    }
}

/// Header bar for the history panel with back button, title, and new chat button
#[component]
fn HistoryPanelHeader() -> Element {
    let mut ctx = use_app_context();

    let handle_back = move |_| {
        ctx.show_history_panel.set(false);
    };

    let handle_new_chat = move |_| {
        let mut ctx = ctx.clone();
        spawn_local(async move {
            match nevoflux_api::new_window_session().await {
                Ok(session_id) => {
                    ctx.session.write().id = session_id.clone();
                    ctx.session.write().title = None;
                    ctx.messages.set(Vec::new());
                    ctx.streaming.set(None);
                    ctx.agent_status.write().hide();
                    let _ = crate::messaging::send_session_resolve(&session_id).await;
                }
                Err(e) => tracing::error!("Failed to create new session: {}", e),
            }
            ctx.show_history_panel.set(false);
        });
    };

    rsx! {
        header { class: "history-panel-header",
            // Back button
            button {
                class: "history-back-btn",
                onclick: handle_back,
                aria_label: "Back to chat",
                // Left arrow icon
                svg {
                    width: "20",
                    height: "20",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M19 12H5" }
                    path { d: "M12 19l-7-7 7-7" }
                }
            }

            h2 { class: "history-panel-title", "History" }

            // New chat button
            button {
                class: "history-new-btn",
                onclick: handle_new_chat,
                aria_label: "New conversation",
                title: "Start a new conversation",
                // Plus icon
                svg {
                    width: "18",
                    height: "18",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 5v14" }
                    path { d: "M5 12h14" }
                }
            }
        }
    }
}

/// Individual session item in the history list
#[component]
fn SessionItem(session: SessionSummary, active_session_id: String) -> Element {
    let mut ctx = use_app_context();
    let is_active = session.id == active_session_id;

    let display_title = session.display_title();
    let relative_time = session.relative_time();
    let message_count = session.message_count;
    let session_id = session.id.clone();
    let session_id_for_delete = session.id.clone();

    // Click to restore session
    let handle_click = {
        let id = session_id.clone();
        move |_| {
            let id = id.clone();
            let mut ctx = ctx.clone();
            spawn_local(async move {
                ctx.session.write().id = id.clone();
                ctx.messages.set(Vec::new());
                let _ = crate::messaging::send_session_resolve(&id).await;
                ctx.show_history_panel.set(false);
            });
        }
    };

    // Delete button handler
    let handle_delete = {
        let id = session_id_for_delete.clone();
        let is_active = is_active;
        move |evt: MouseEvent| {
            evt.stop_propagation(); // Prevent triggering the row click
            let id = id.clone();
            let mut ctx = ctx.clone();
            ctx.history.write().remove_session(&id);
            spawn_local(async move {
                let _ = crate::messaging::send_session_delete(&id).await;
                if is_active {
                    // Deleted the active session - create a new one
                    if let Ok(new_id) = nevoflux_api::new_window_session().await {
                        ctx.session.write().id = new_id.clone();
                        ctx.session.write().title = None;
                        ctx.messages.set(Vec::new());
                        let _ = crate::messaging::send_session_resolve(&new_id).await;
                    }
                }
            });
        }
    };

    let item_class = if is_active {
        "history-item active"
    } else {
        "history-item"
    };

    rsx! {
        div {
            class: "{item_class}",
            onclick: handle_click,
            role: "button",
            tabindex: "0",
            aria_label: "Open session: {display_title}",

            div { class: "history-item-content",
                span { class: "history-item-title", "{display_title}" }
                div { class: "history-item-meta",
                    span { class: "history-item-time", "{relative_time}" }
                    if message_count > 0 {
                        {
                            let suffix = if message_count == 1 { "" } else { "s" };
                            rsx! {
                                span { class: "history-item-count",
                                    "{message_count} msg{suffix}"
                                }
                            }
                        }
                    }
                }
            }

            // Delete button (visible on hover via CSS)
            button {
                class: "history-item-delete",
                onclick: handle_delete,
                aria_label: "Delete session",
                title: "Delete this conversation",
                // X icon
                svg {
                    width: "14",
                    height: "14",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M18 6L6 18" }
                    path { d: "M6 6l12 12" }
                }
            }
        }
    }
}

/// Group sessions by their date group label, preserving order
fn group_by_date<'a>(sessions: &[&'a SessionSummary]) -> Vec<(&'static str, Vec<&'a SessionSummary>)> {
    // Ordered list of possible group labels
    let group_order = ["Today", "Yesterday", "This Week", "This Month", "Older"];
    let mut groups: Vec<(&'static str, Vec<&'a SessionSummary>)> = Vec::new();

    for &label in &group_order {
        let matching: Vec<&'a SessionSummary> = sessions
            .iter()
            .filter(|s| s.date_group() == label)
            .copied()
            .collect();

        if !matching.is_empty() {
            groups.push((label, matching));
        }
    }

    groups
}
