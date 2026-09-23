/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message bubble component

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::components::RenderProgressCard;
use crate::state::{Message, MessageContent, MessageRole, MessageStatus};
use super::{ActivityFeed, DoneFeed, CodeBlock, ErrorCard, UsageStats, copy_text_fallback};

/// Single message bubble component
#[component]
pub fn MessageBubble(
    message: Message,
    #[props(default = false)] is_last_user: bool,
    #[props(default = 0)] message_index: usize,
) -> Element {
    let is_user = message.role == MessageRole::User;
    let role_class = if is_user { "user" } else { "assistant" };
    let mut is_editing = use_signal(|| false);
    let ctx = use_app_context();

    // Collect job_ids bound to this message so each bound
    // `canvas_render_video` tool_use gets a `RenderProgressCard` below the
    // bubble. Bindings are populated by `handle_render_progress_delivery`
    // when a job_id first appears on the `jobs:render:*` EventBus channel,
    // so a card only appears once progress begins streaming.
    let render_job_ids: Vec<String> = {
        let has_canvas_tool = message
            .tool_calls
            .iter()
            .any(|tc| tc.name == "canvas_render_video");
        if !has_canvas_tool {
            Vec::new()
        } else {
            let map = ctx.message_render_job_ids.read();
            map.get(&message.id).cloned().unwrap_or_default()
        }
    };

    // Debug: log attachment count
    tracing::info!("MessageBubble render: role={:?}, attachments={}", message.role, message.attachments.len());

    // Get text content for copy/edit functionality
    let content_text = match &message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Markdown(md) => md.clone(),
        MessageContent::Code { code, .. } => code.clone(),
        MessageContent::Error { message, .. } => message.clone(),
        MessageContent::Plan(plan) => plan.summary.clone(),
        MessageContent::Artifact(data) => data.title.clone(),
    };

    // Live messages skip the slideIn animation to prevent layout shift
    // when replacing the StreamingBubble.
    let animate_class = if message.is_live { "no-animate" } else { "" };

    rsx! {
        // Wrapper for user messages to position toolbar outside
        div {
            class: "message-row {role_class} {animate_class}",

            // Left toolbar for user messages (outside the bubble)
            if is_user && !is_editing() {
                UserMessageToolbar {
                    content: content_text.clone(),
                    is_last: is_last_user,
                    on_edit: move |_| is_editing.set(true),
                }
            }

            // Message bubble
            div {
                class: "message-bubble {role_class}",
                class: if is_editing() { "editing" },

                // Message content or edit form
                if is_editing() {
                    EditMessageForm {
                        original_content: content_text.clone(),
                        message_index: message_index,
                        on_cancel: move |_| is_editing.set(false),
                    }
                } else {
                    // Attachments display logic:
                    // - Only images WITH data: show full image content
                    // - Mixed, files only, or images without data: show as file chips (icon + filename)
                    {
                        let has_attachments = !message.attachments.is_empty();
                        // All images must have mime_type starting with "image/" AND have non-empty data
                        let all_images_with_data = has_attachments && message.attachments.iter().all(|a| {
                            a.mime_type.starts_with("image/") && !a.data.is_empty()
                        });
                        let max_visible = 3;
                        let total = message.attachments.len();
                        let overflow_count = if total > max_visible { total - max_visible } else { 0 };
                        let has_overflow_class = if overflow_count > 0 { "has-overflow" } else { "" };

                        rsx! {
                            if has_attachments {
                                if all_images_with_data {
                                    // Only images with data: show full image content
                                    div {
                                        class: "message-images",
                                        {message.attachments.iter().map(|attachment| {
                                            let src = format!("data:{};base64,{}", attachment.mime_type, attachment.data);
                                            let alt = attachment.name.clone();
                                            let id = attachment.id.clone();

                                            rsx! {
                                                img {
                                                    key: "{id}",
                                                    class: "message-image",
                                                    src: "{src}",
                                                    alt: "{alt}",
                                                    title: "{alt}",
                                                }
                                            }
                                        })}
                                    }
                                } else {
                                    // Mixed, files only, or images without data: show as file chips
                                    div {
                                        class: "message-attachments {has_overflow_class}",
                                        {message.attachments.iter().take(max_visible).map(|attachment| {
                                            let is_image_with_data = attachment.mime_type.starts_with("image/") && !attachment.data.is_empty();
                                            let name = attachment.name.clone();
                                            let id = attachment.id.clone();

                                            rsx! {
                                                div {
                                                    key: "{id}",
                                                    class: "message-file-chip",
                                                    title: "{name}",

                                                    if is_image_with_data {
                                                        {
                                                            let src = format!("data:{};base64,{}", attachment.mime_type, attachment.data);
                                                            rsx! {
                                                                img {
                                                                    class: "message-file-thumbnail",
                                                                    src: "{src}",
                                                                    alt: "{name}",
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        span { class: "message-file-icon", "📄" }
                                                    }

                                                    span { class: "message-file-name", "{name}" }
                                                }
                                            }
                                        })}

                                        if overflow_count > 0 {
                                            div {
                                                class: "message-attachments-overflow",
                                                "+{overflow_count}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Activity feed (tool calls) or done indicator for assistant messages
                    {
                        let tc_count = message.tool_calls.len();
                        if tc_count > 0 {
                            web_sys::console::log_1(&format!(
                                "[WASM] MessageBubble RENDER: id={}, tc_count={}, names=[{}]",
                                message.id,
                                tc_count,
                                message.tool_calls.iter().map(|tc| tc.name.as_str()).collect::<Vec<_>>().join(", ")
                            ).into());
                        }
                        rsx! {
                            if !is_user && tc_count > 0 {
                                ActivityFeed { tool_calls: message.tool_calls.clone() }
                            } else if !is_user && message.is_live {
                                DoneFeed {}
                            }
                        }
                    }

                    // Render progress cards for every `canvas_render_video`
                    // tool_use in this message that has a bound job_id.
                    if !render_job_ids.is_empty() {
                        div { class: "render-progress-cards",
                            for jid in render_job_ids.iter() {
                                RenderProgressCard {
                                    key: "{jid}",
                                    job_id: jid.clone(),
                                }
                            }
                        }
                    }

                    // Text content
                    div { class: "bubble-content",
                        match &message.content {
                            MessageContent::Text(text) if !text.is_empty() && is_user => rsx! {
                                CollapsibleUserText { text: text.clone() }
                            },
                            MessageContent::Text(text) if !text.is_empty() => rsx! {
                                p { "{text}" }
                            },
                            MessageContent::Text(_) => rsx! {}, // Empty text with images
                            MessageContent::Markdown(md) => rsx! {
                                div { class: "markdown-content",
                                    dangerous_inner_html: "{markdown_render::render_markdown(md)}"
                                }
                            },
                            MessageContent::Code { language, code } => rsx! {
                                CodeBlock {
                                    language: language.clone(),
                                    code: code.clone(),
                                }
                            },
                            MessageContent::Error { code, message, recoverable } => rsx! {
                                ErrorCard {
                                    code: code.clone(),
                                    message: message.clone(),
                                    recoverable: *recoverable,
                                }
                            },
                            MessageContent::Plan(_) => rsx! {},
                            MessageContent::Artifact(_) => rsx! {},
                        }
                    }

                    // Error indicator for failed sends
                    if is_user && message.status == MessageStatus::Error {
                        span { class: "send-error", "Failed to send" }
                    }

                    // Bottom toolbar for assistant messages (only when message has text content)
                    {
                        let has_text = match &message.content {
                            MessageContent::Text(t) => !t.is_empty(),
                            MessageContent::Markdown(md) => !md.is_empty(),
                            MessageContent::Code { .. } => true,
                            _ => false,
                        };

                        // The toolbar also carries the reply's token stats, so
                        // it appears for a reply that is nothing but tool calls
                        // too — there the buttons have nothing to act on and
                        // only the stats show.
                        let has_usage = message.usage.is_some();

                        rsx! {
                            if message.role == MessageRole::Assistant && (has_text || has_usage) {
                                AssistantMessageToolbar {
                                    content: content_text.clone(),
                                    usage: message.usage.clone(),
                                    show_actions: has_text,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Toolbar for user messages (positioned to the left of the message)
#[component]
fn UserMessageToolbar(
    content: String,
    is_last: bool,
    on_edit: EventHandler<()>,
) -> Element {
    let mut copied = use_signal(|| false);
    let content_for_copy = content.clone();

    let handle_copy = move |_| {
        let text = content_for_copy.clone();
        if copy_text_fallback(&text) {
            copied.set(true);
            spawn(async move {
                gloo::timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
            });
            return;
        }
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                if wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&text)
                ).await.is_ok() {
                    copied.set(true);
                    gloo::timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "message-toolbar user-toolbar-left",
            // Copy button
            button {
                class: "toolbar-btn copy-btn",
                class: if copied() { "copied" },
                onclick: handle_copy,
                title: "Copy message",
                aria_label: "Copy message",
                if copied() { "✓" } else { "📋" }
            }

            // Edit button (only for last user message)
            if is_last {
                button {
                    class: "toolbar-btn edit-btn",
                    onclick: move |_| on_edit.call(()),
                    title: "Edit message",
                    aria_label: "Edit message",
                    "✏️"
                }
            }
        }
    }
}

/// Edit message form
#[component]
fn EditMessageForm(
    original_content: String,
    message_index: usize,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut ctx = use_app_context();
    let mut edit_text = use_signal(|| original_content.clone());

    let handle_update = move |_| {
        let new_text = edit_text.read().trim().to_string();
        if new_text.is_empty() {
            return;
        }

        // Remove this message and all messages after it
        ctx.messages.with_mut(|messages| {
            messages.truncate(message_index);
        });

        // Add updated user message
        ctx.messages.write().push(Message::user(&new_text));

        // Send updated message
        let session_id = ctx.session.read().id.clone();
        let mock_enabled = ctx.mock_enabled;
        let text = new_text;

        wasm_bindgen_futures::spawn_local(async move {
            if mock_enabled {
                crate::mock::mock_send_message(ctx, text).await;
            } else {
                ctx.agent_status.write().set_thinking();
                let (tab_id, tab_ids) = crate::messaging::build_current_tab_ids().await;
                let _ = crate::messaging::send_chat_message(&session_id, text, ctx.chat_mode.read().clone(), vec![], vec![], tab_id, tab_ids, None).await;
            }
        });
    };

    rsx! {
        div { class: "edit-form",
            textarea {
                class: "edit-textarea",
                value: "{edit_text}",
                oninput: move |evt| edit_text.set(evt.value()),
                rows: "3",
                aria_label: "Edit message",
            }
            div { class: "edit-actions",
                button {
                    class: "edit-btn cancel-btn",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                button {
                    class: "edit-btn update-btn",
                    onclick: handle_update,
                    "Update"
                }
            }
        }
    }
}

/// Toolbar for assistant messages (reactions + copy + token stats)
///
/// `show_actions` is false for replies with no text: react-to-this and
/// copy-this have nothing to point at, but the token stats still do.
#[component]
fn AssistantMessageToolbar(
    content: String,
    usage: Option<shared_protocol::chat::TurnUsage>,
    #[props(default = true)] show_actions: bool,
) -> Element {
    let mut copied = use_signal(|| false);
    let mut reaction = use_signal(|| Option::<bool>::None); // Some(true)=good, Some(false)=bad
    let content_for_copy = content.clone();

    let handle_copy = move |_| {
        let text = content_for_copy.clone();
        if copy_text_fallback(&text) {
            copied.set(true);
            spawn(async move {
                gloo::timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
            });
            return;
        }
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                if wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&text)
                ).await.is_ok() {
                    copied.set(true);
                    gloo::timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
            }
        });
    };

    let handle_good = move |_| {
        if reaction() == Some(true) {
            reaction.set(None);
        } else {
            reaction.set(Some(true));
        }
    };

    let handle_bad = move |_| {
        if reaction() == Some(false) {
            reaction.set(None);
        } else {
            reaction.set(Some(false));
        }
    };

    rsx! {
        div { class: "message-toolbar assistant-toolbar",
            if show_actions {
                // Good response
                button {
                    class: "toolbar-btn reaction-btn",
                    class: if reaction() == Some(true) { "active good" },
                    onclick: handle_good,
                    title: "Good response",
                    aria_label: "Good response",
                    "👍"
                }

                // Bad response
                button {
                    class: "toolbar-btn reaction-btn",
                    class: if reaction() == Some(false) { "active bad" },
                    onclick: handle_bad,
                    title: "Bad response",
                    aria_label: "Bad response",
                    "👎"
                }

                // Copy
                button {
                    class: "toolbar-btn copy-btn",
                    class: if copied() { "copied" },
                    onclick: handle_copy,
                    title: "Copy response",
                    aria_label: "Copy response",
                    if copied() { "✓" } else { "📋" }
                }
            }

            // Token stats, right-aligned
            if let Some(usage) = usage.clone() {
                UsageStats { usage }
            }
        }
    }
}

/// Maximum visible lines for collapsed user messages
const MAX_COLLAPSED_LINES: usize = 5;
/// Maximum chars on the last visible line before truncation
const TRUNCATE_LAST_LINE_CHARS: usize = 20;

/// Truncate text to at most `MAX_COLLAPSED_LINES` lines.
/// If the text exceeds that, the last visible line is truncated with "...".
/// Returns `(truncated_text, was_truncated)`.
fn truncate_user_text(text: &str) -> (String, bool) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_COLLAPSED_LINES {
        return (text.to_string(), false);
    }
    let mut result: Vec<&str> = lines[..MAX_COLLAPSED_LINES - 1].to_vec();
    let last_line = lines[MAX_COLLAPSED_LINES - 1];
    // Truncate the last line, respecting character boundaries (safe for CJK)
    let truncated: String = last_line.chars().take(TRUNCATE_LAST_LINE_CHARS).collect();
    let last = if truncated.len() < last_line.len() {
        format!("{}...", truncated)
    } else {
        truncated
    };
    result.push(&""); // placeholder, we'll build the final string manually
    let mut out = result[..MAX_COLLAPSED_LINES - 1].join("\n");
    out.push('\n');
    out.push_str(&last);
    (out, true)
}

/// Collapsible text component for user messages.
/// Shows at most 5 lines with "..." truncation; expand button in top-right corner.
#[component]
fn CollapsibleUserText(text: String) -> Element {
    let mut expanded = use_signal(|| false);
    let (truncated, needs_collapse) = truncate_user_text(&text);

    let display_text = if *expanded.read() || !needs_collapse {
        text.clone()
    } else {
        truncated
    };

    let is_expanded = *expanded.read();

    rsx! {
        div { class: "collapsible-user-text",
            class: if needs_collapse && !is_expanded { "collapsed" },
            p { "{display_text}" }
            if needs_collapse {
                button {
                    class: "collapse-toggle-btn",
                    class: if is_expanded { "expanded" },
                    title: if is_expanded { "Collapse" } else { "Expand" },
                    aria_label: if is_expanded { "Collapse message" } else { "Expand message" },
                    onclick: move |_| expanded.set(!is_expanded),
                    // Chevron SVG icon
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        polyline { points: "6 9 12 15 18 9" }
                    }
                }
            }
        }
    }
}
