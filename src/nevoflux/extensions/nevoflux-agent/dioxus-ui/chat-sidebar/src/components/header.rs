/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Header component

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::bindings::nevoflux_api;
use crate::context::{use_app_context, AppContext};
use crate::state::{Message, MessageContent, MessageRole};

/// Render the current conversation as a markdown transcript and derive a title.
///
/// Title = the first user message (trimmed/truncated), or a timestamped
/// fallback if there is none. Content = a `**Role:** text` transcript of all
/// text-bearing messages.
fn build_conversation_markdown(messages: &[Message]) -> Option<(String, String)> {
    let mut lines: Vec<String> = Vec::new();
    let mut title: Option<String> = None;

    for msg in messages {
        let text = match &msg.content {
            MessageContent::Text(t) | MessageContent::Markdown(t) => t.clone(),
            MessageContent::Code { language, code } => {
                format!("```{}\n{}\n```", language, code)
            }
            // Skip non-textual cards (plan/artifact/error) in the transcript.
            _ => continue,
        };
        if text.trim().is_empty() {
            continue;
        }

        let role = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };

        if title.is_none() && msg.role == MessageRole::User {
            let t = text.trim();
            let truncated: String = t.chars().take(80).collect();
            title = Some(if t.chars().count() > 80 {
                format!("{}…", truncated)
            } else {
                truncated
            });
        }

        lines.push(format!("**{}:** {}", role, text.trim()));
    }

    if lines.is_empty() {
        return None;
    }

    let title = title.unwrap_or_else(|| {
        let iso = String::from(js_sys::Date::new_0().to_iso_string());
        format!("Conversation {}", iso)
    });
    Some((title, lines.join("\n\n")))
}

/// Header component with History and Maximize buttons
#[component]
pub fn Header() -> Element {
    let ctx = use_app_context();

    // Read maximize state
    let is_maximized = ctx.maximize.read().is_maximized;

    let toggle_history = {
        let mut ctx = ctx.clone();
        move |_| {
            let current = *ctx.show_history_panel.read();
            ctx.show_history_panel.set(!current);

            // Refresh history when opening
            if !current {
                ctx.history.write().set_loading();
                spawn_local(async move {
                    let _ = crate::messaging::send_session_list(50, 0).await;
                });
            }
        }
    };

    // Handle minimize: shrink sidebar to 48px rail
    let handle_minimize = {
        move |_| {
            tracing::info!("Minimize to rail requested");
            spawn_local(async move {
                if let Err(e) = crate::messaging::send_set_sidebar_width(48).await {
                    tracing::error!("Failed to minimize sidebar: {}", e);
                }
            });
        }
    };

    // Handle maximize: open in new tab, close sidebar
    let handle_maximize = {
        let ctx = ctx.clone();
        move |_| {
            tracing::info!("Maximize requested");

            // Try to close sidebar IMMEDIATELY (sync) to preserve user gesture context
            nevoflux_api::try_close_sidebar_sync();

            let ctx = ctx.clone();
            spawn_local(async move {
                if let Err(e) = do_maximize(ctx).await {
                    tracing::error!("Failed to maximize: {}", e);
                }
            });
        }
    };

    // Handle restore: close tab, activate source tab, open sidebar
    let handle_restore = {
        let ctx = ctx.clone();
        move |_| {
            tracing::info!("Restore requested");
            let ctx = ctx.clone();
            spawn_local(async move {
                if let Err(e) = do_restore(ctx).await {
                    tracing::error!("Failed to restore: {}", e);
                }
            });
        }
    };

    // "Save as concept" feedback state: None = idle, Some(true) = saving,
    // Some(false) handled via title text below.
    let mut kb_saving = use_signal(|| false);
    let mut kb_status: Signal<Option<String>> = use_signal(|| None);

    // Handle "Save as concept": serialize the conversation and call the daemon
    // brain.save_conversation RPC via the existing bg:system_command path.
    let handle_save_concept = {
        let ctx = ctx.clone();
        move |_| {
            if *kb_saving.read() {
                return;
            }
            let messages = ctx.messages.read().clone();
            let session_id = ctx.session.read().id.clone();

            let Some((title, content)) = build_conversation_markdown(&messages) else {
                kb_status.set(Some("Nothing to save yet".to_string()));
                return;
            };

            kb_saving.set(true);
            kb_status.set(Some("Saving…".to_string()));
            spawn_local(async move {
                match crate::messaging::save_conversation_to_kb(
                    &title,
                    &content,
                    Some(&session_id),
                )
                .await
                {
                    Ok(slug) => {
                        tracing::info!("Saved conversation to KB: {}", slug);
                        kb_status.set(Some(format!("Saved: {}", slug)));
                    }
                    Err(e) => {
                        tracing::error!("Save as concept failed: {}", e);
                        kb_status.set(Some(e));
                    }
                }
                kb_saving.set(false);
            });
        }
    };

    let kb_title = kb_status
        .read()
        .clone()
        .unwrap_or_else(|| "Save conversation as concept".to_string());

    // Read avatar
    let avatar = ctx.avatar_url.read();

    rsx! {
        header { class: "header",
            // Left side: Avatar (shown when configured)
            div { class: "header-left",
                if let Some(ref url) = *avatar {
                    div { class: "header-avatar",
                        img {
                            src: "{url}",
                            alt: "Avatar",
                            class: "header-avatar-img",
                        }
                    }
                }
            }

            // Right side: Action buttons
            div { class: "header-right",
                // Save as concept button (M4-5 A3): save conversation to KB
                button {
                    class: "header-btn save-concept-btn",
                    aria_label: "Save as concept",
                    title: "{kb_title}",
                    disabled: *kb_saving.read(),
                    onclick: handle_save_concept,
                    // Bookmark / save icon
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
                    }
                }

                // History button
                button {
                    class: "header-btn history-btn",
                    aria_label: "History",
                    title: "Conversation history",
                    onclick: toggle_history,
                    // Clock/history icon
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "10" }
                        path { d: "M12 6v6l4 2" }
                    }
                }

                // Maximize/Restore button
                if is_maximized {
                    // Restore button (in tab mode)
                    button {
                        class: "header-btn restore-btn",
                        aria_label: "Restore to sidebar",
                        title: "Restore to sidebar",
                        onclick: handle_restore,
                        // Arrows pointing inward icon (restore)
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M4 14h6v6" }
                            path { d: "M20 10h-6V4" }
                            path { d: "M14 10l7-7" }
                            path { d: "M3 21l7-7" }
                        }
                    }
                } else {
                    // Maximize button (in sidebar mode)
                    button {
                        class: "header-btn maximize-btn",
                        aria_label: "Open in new tab",
                        title: "Open in new tab",
                        onclick: handle_maximize,
                        // Arrows pointing outward icon (maximize)
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 3h6v6" }
                            path { d: "M9 21H3v-6" }
                            path { d: "M21 3l-7 7" }
                            path { d: "M3 21l7-7" }
                        }
                    }
                }

                // Minimize button (only in normal sidebar mode)
                if !is_maximized {
                    button {
                        class: "header-btn minimize-btn",
                        aria_label: "Minimize to rail",
                        title: "Minimize to rail",
                        onclick: handle_minimize,
                        // Panel-right-close icon (vertical line + right chevron)
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                            line { x1: "15", y1: "3", x2: "15", y2: "21" }
                            path { d: "M19 10l2-2-2-2" }
                        }
                    }
                }
            }
        }
    }
}

// ==================== Maximize/Restore Logic ====================

/// Maximize: open chat in new tab, close sidebar
async fn do_maximize(ctx: AppContext) -> Result<(), String> {
    // Get session_id from current session
    let session_id = ctx.session.read().id.clone();

    // Get target_tab_id from tab_context (the tab AI operates on)
    let target_tab_id = ctx.tab_context.read().tab_id;

    // Get source_tab_id (current active tab where sidebar is shown)
    let source_tab = nevoflux_api::get_active_tab().await?;
    let source_tab_id = source_tab.id as i32;

    // Build URL with parameters
    let base_url = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_else(|| "moz-extension://unknown/wasm/chat-sidebar/index.html".to_string());

    // Extract base path (remove any existing query params)
    let base_path = base_url.split('?').next().unwrap_or(&base_url);

    let url = format!(
        "{}?mode=maximized&session_id={}&target_tab_id={}&source_tab_id={}",
        base_path,
        session_id,
        target_tab_id,
        source_tab_id
    );

    tracing::info!("Opening maximized view: {}", url);

    // Create new tab with the URL
    // Note: sidebar close is attempted synchronously in the click handler
    // to preserve user gesture context (Firefox security requirement)
    nevoflux_api::create_tab(&url, true).await?;

    Ok(())
}

/// Restore: close current tab, activate source tab, open sidebar
async fn do_restore(ctx: AppContext) -> Result<(), String> {
    let maximize_state = ctx.maximize.read();
    let source_tab_id = maximize_state.source_tab_id;
    drop(maximize_state);

    // Get current tab ID (we're in a tab, not sidebar)
    let current_tab = nevoflux_api::get_current_tab().await?
        .ok_or_else(|| "Could not get current tab".to_string())?;
    let current_tab_id = current_tab.id as i32;

    // Activate source tab (if it still exists)
    if let Some(source_id) = source_tab_id {
        if let Err(e) = nevoflux_api::update_tab(source_id, true).await {
            tracing::warn!("Failed to activate source tab {}: {}", source_id, e);
            // Tab might have been closed - continue anyway
        }
    }

    // Open the sidebar
    if let Err(e) = nevoflux_api::open_sidebar().await {
        tracing::warn!("Failed to open sidebar: {}", e);
        // Continue anyway
    }

    // Close current tab
    nevoflux_api::remove_tab(current_tab_id).await?;

    Ok(())
}
