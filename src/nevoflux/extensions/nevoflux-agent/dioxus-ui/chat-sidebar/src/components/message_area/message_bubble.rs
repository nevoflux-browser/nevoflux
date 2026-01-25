/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message bubble component

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::{Message, MessageContent, MessageRole, MessageStatus};
use super::{CodeBlock, ErrorCard};

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

    // Debug: log attachment count
    tracing::info!("MessageBubble render: role={:?}, attachments={}", message.role, message.attachments.len());

    // Get text content for copy/edit functionality
    let content_text = match &message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Markdown(md) => md.clone(),
        MessageContent::Code { code, .. } => code.clone(),
        MessageContent::Error { message, .. } => message.clone(),
    };

    rsx! {
        // Wrapper for user messages to position toolbar outside
        div {
            class: "message-row {role_class}",

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

                    // Text content
                    div { class: "bubble-content",
                        match &message.content {
                            MessageContent::Text(text) if !text.is_empty() => rsx! {
                                p { "{text}" }
                            },
                            MessageContent::Text(_) => rsx! {}, // Empty text with images
                            MessageContent::Markdown(md) => rsx! {
                                div { class: "markdown-content",
                                    dangerous_inner_html: "{render_simple_markdown(md)}"
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
                        }
                    }

                    // Error indicator for failed sends
                    if is_user && message.status == MessageStatus::Error {
                        span { class: "send-error", "Failed to send" }
                    }

                    // Bottom toolbar for assistant messages
                    if message.role == MessageRole::Assistant {
                        AssistantMessageToolbar { content: content_text.clone() }
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
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&text)
                ).await;
                copied.set(true);
                gloo::timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
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
        let tab_id = ctx.tab_context.read().tab_id;
        let mock_enabled = ctx.mock_enabled;
        let text = new_text;

        wasm_bindgen_futures::spawn_local(async move {
            if mock_enabled {
                crate::mock::mock_send_message(ctx, text).await;
            } else {
                ctx.agent_status.write().set_thinking();
                let _ = crate::messaging::send_chat_message(&session_id, text, vec![], Some(tab_id)).await;
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

/// Toolbar for assistant messages (reactions + copy)
#[component]
fn AssistantMessageToolbar(content: String) -> Element {
    let mut copied = use_signal(|| false);
    let mut reaction = use_signal(|| Option::<bool>::None); // Some(true)=good, Some(false)=bad
    let content_for_copy = content.clone();

    let handle_copy = move |_| {
        let text = content_for_copy.clone();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&text)
                ).await;
                copied.set(true);
                gloo::timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
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
    }
}

/// Simple markdown to HTML conversion
/// P2: Replace with proper markdown renderer
fn render_simple_markdown(md: &str) -> String {
    let mut html = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    let mut in_paragraph = false;

    while i < lines.len() {
        let line = lines[i];

        // Check for code block start (```)
        if line.trim().starts_with("```") {
            // Close any open paragraph
            if in_paragraph {
                html.push_str("</p>");
                in_paragraph = false;
            }

            // Extract language
            let lang = line.trim().trim_start_matches('`').trim();
            let lang_display = if lang.is_empty() { "code" } else { lang };

            // Collect code lines until closing ```
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(html_escape(lines[i]));
                i += 1;
            }

            // Generate code block HTML with copy button
            let code_id = format!("code-{}", i);
            let code_content = code_lines.join("\n");
            html.push_str(&format!(
                r#"<div class="code-block"><div class="code-header"><span class="code-language">{lang}</span><button class="code-copy-btn" onclick="(function(btn){{var code=document.getElementById('{id}').textContent;navigator.clipboard.writeText(code).then(function(){{btn.textContent='Copied!';btn.classList.add('copied');setTimeout(function(){{btn.textContent='Copy';btn.classList.remove('copied')}},2000)}})}})(this)">Copy</button></div><div class="code-content"><pre id="{id}">{code}</pre></div></div>"#,
                lang = lang_display,
                id = code_id,
                code = code_content
            ));
        } else if line.is_empty() {
            // Empty line - paragraph break
            if in_paragraph {
                html.push_str("</p>");
                in_paragraph = false;
            }
        } else {
            // Regular text line
            if !in_paragraph {
                html.push_str("<p>");
                in_paragraph = true;
            } else {
                html.push_str("<br>");
            }
            html.push_str(&render_inline_markdown(line));
        }

        i += 1;
    }

    // Close any open paragraph
    if in_paragraph {
        html.push_str("</p>");
    }

    html
}

/// Render inline markdown (bold, italic, inline code)
fn render_inline_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '*' {
            if chars.peek() == Some(&'*') {
                chars.next();
                let mut bold_text = String::new();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'*') {
                        chars.next();
                        break;
                    }
                    bold_text.push(c);
                }
                result.push_str(&format!("<strong>{}</strong>", html_escape(&bold_text)));
            } else {
                let mut italic_text = String::new();
                while let Some(c) = chars.next() {
                    if c == '*' {
                        break;
                    }
                    italic_text.push(c);
                }
                result.push_str(&format!("<em>{}</em>", html_escape(&italic_text)));
            }
        } else if ch == '`' {
            let mut code_text = String::new();
            while let Some(c) = chars.next() {
                if c == '`' {
                    break;
                }
                code_text.push(c);
            }
            result.push_str(&format!("<code>{}</code>", html_escape(&code_text)));
        } else {
            // Escape HTML special chars
            match ch {
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                _ => result.push(ch),
            }
        }
    }

    result
}

/// HTML escape helper
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
