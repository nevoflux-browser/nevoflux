/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message bubble component

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::{Message, MessageContent, MessageRole, MessageStatus};
use super::{ActivityFeed, DoneFeed, CodeBlock, ErrorCard, copy_text_fallback};

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
        MessageContent::Plan(plan) => plan.summary.clone(),
        MessageContent::Artifact(data) => data.title.clone(),
        MessageContent::QA { answer, .. } => answer.clone(),
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
                            MessageContent::QA { question, answer } => rsx! {
                                div { class: "qa-message",
                                    div { class: "qa-question",
                                        span { class: "qa-label", "Q: " }
                                        "{question}"
                                    }
                                    div { class: "qa-answer",
                                        span { class: "qa-label", "A: " }
                                        "{answer}"
                                    }
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

                        rsx! {
                            if message.role == MessageRole::Assistant && has_text {
                                AssistantMessageToolbar { content: content_text.clone() }
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
                let _ = crate::messaging::send_chat_message(&session_id, text, ctx.chat_mode.read().clone(), vec![], vec![], tab_id, tab_ids).await;
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
///
/// Supports: code blocks, headings, horizontal rules, unordered/ordered lists,
/// bold, italic, inline code, and links.
pub fn render_simple_markdown(md: &str) -> String {
    let mut html = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    let mut in_paragraph = false;
    let mut in_ul = false;
    let mut in_ol = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Table: | col | col |
        if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.contains(" | ") {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);

            html.push_str("<table>");
            // Parse header row
            let header_cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim())
                .collect();
            html.push_str("<thead><tr>");
            for cell in &header_cells {
                html.push_str(&format!("<th>{}</th>", render_inline_markdown(cell)));
            }
            html.push_str("</tr></thead>");

            // Skip separator row (e.g., |---|---|)
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if next.starts_with('|') && next.contains("---") {
                    i += 1;
                }
            }

            // Parse body rows
            html.push_str("<tbody>");
            while i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if !(next.starts_with('|') && next.ends_with('|')) {
                    break;
                }
                i += 1;
                let cells: Vec<&str> = next
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim())
                    .collect();
                html.push_str("<tr>");
                for cell in &cells {
                    html.push_str(&format!("<td>{}</td>", render_inline_markdown(cell)));
                }
                html.push_str("</tr>");
            }
            html.push_str("</tbody></table>");
        }
        // Fenced code block
        else if trimmed.starts_with("```") {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);

            let lang = trimmed.trim_start_matches('`').trim();
            let lang_display = if lang.is_empty() { "code" } else { lang };

            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(html_escape(lines[i]));
                i += 1;
            }

            let code_content = code_lines.join("\n");
            html.push_str(&format!(
                r#"<div class="code-block"><div class="code-header"><span class="code-language">{lang}</span><button class="code-copy-btn">Copy</button></div><div class="code-content"><pre>{code}</pre></div></div>"#,
                lang = lang_display,
                code = code_content
            ));
        }
        // Horizontal rule
        else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
            html.push_str("<hr>");
        }
        // Headings
        else if trimmed.starts_with("### ") {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
            html.push_str(&format!("<h3>{}</h3>", render_inline_markdown(&trimmed[4..])));
        } else if trimmed.starts_with("## ") {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
            html.push_str(&format!("<h2>{}</h2>", render_inline_markdown(&trimmed[3..])));
        } else if trimmed.starts_with("# ") {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
            html.push_str(&format!("<h1>{}</h1>", render_inline_markdown(&trimmed[2..])));
        }
        // Unordered list item
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            // Close paragraph/ol if open
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            if in_ol { html.push_str("</ol>"); in_ol = false; }
            if !in_ul { html.push_str("<ul>"); in_ul = true; }
            html.push_str(&format!("<li>{}</li>", render_inline_markdown(&trimmed[2..])));
        }
        // Ordered list item (e.g. "1. item")
        else if is_ordered_list_item(trimmed) {
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            if in_ul { html.push_str("</ul>"); in_ul = false; }
            if !in_ol { html.push_str("<ol>"); in_ol = true; }
            let content = trimmed.splitn(2, ". ").nth(1).unwrap_or("");
            html.push_str(&format!("<li>{}</li>", render_inline_markdown(content)));
        }
        // Empty line
        else if trimmed.is_empty() {
            close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
        }
        // Regular text (paragraph)
        else {
            // Close list contexts if we hit regular text
            if in_ul { html.push_str("</ul>"); in_ul = false; }
            if in_ol { html.push_str("</ol>"); in_ol = false; }

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

    close_open_blocks(&mut html, &mut in_paragraph, &mut in_ul, &mut in_ol);
    html
}

/// Close any open block-level elements (paragraph, ul, ol)
fn close_open_blocks(html: &mut String, in_paragraph: &mut bool, in_ul: &mut bool, in_ol: &mut bool) {
    if *in_paragraph { html.push_str("</p>"); *in_paragraph = false; }
    if *in_ul { html.push_str("</ul>"); *in_ul = false; }
    if *in_ol { html.push_str("</ol>"); *in_ol = false; }
}

/// Check if a line is an ordered list item (e.g., "1. ", "12. ")
fn is_ordered_list_item(line: &str) -> bool {
    if let Some(dot_pos) = line.find(". ") {
        let prefix = &line[..dot_pos];
        !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Render inline markdown (bold, italic, inline code, links)
fn render_inline_markdown(text: &str) -> String {
    let mut result = String::new();
    let chars_vec: Vec<char> = text.chars().collect();
    let len = chars_vec.len();
    let mut i = 0;

    while i < len {
        let ch = chars_vec[i];

        // Bold: **text**
        if ch == '*' && i + 1 < len && chars_vec[i + 1] == '*' {
            i += 2;
            let mut bold_text = String::new();
            while i < len {
                if chars_vec[i] == '*' && i + 1 < len && chars_vec[i + 1] == '*' {
                    i += 2;
                    break;
                }
                bold_text.push(chars_vec[i]);
                i += 1;
            }
            result.push_str(&format!("<strong>{}</strong>", html_escape(&bold_text)));
        }
        // Italic: *text*
        else if ch == '*' {
            i += 1;
            let mut italic_text = String::new();
            while i < len {
                if chars_vec[i] == '*' {
                    i += 1;
                    break;
                }
                italic_text.push(chars_vec[i]);
                i += 1;
            }
            result.push_str(&format!("<em>{}</em>", html_escape(&italic_text)));
        }
        // Inline code: `text`
        else if ch == '`' {
            i += 1;
            let mut code_text = String::new();
            while i < len {
                if chars_vec[i] == '`' {
                    i += 1;
                    break;
                }
                code_text.push(chars_vec[i]);
                i += 1;
            }
            result.push_str(&format!("<code>{}</code>", html_escape(&code_text)));
        }
        // Image: ![alt](url)
        else if ch == '!' && i + 1 < len && chars_vec[i + 1] == '[' {
            let start = i;
            i += 2; // skip ![
            let mut alt_text = String::new();
            let mut found_image = false;

            // Collect alt text up to ]
            while i < len && chars_vec[i] != ']' {
                alt_text.push(chars_vec[i]);
                i += 1;
            }

            // Check for ](url)
            if i < len && chars_vec[i] == ']' && i + 1 < len && chars_vec[i + 1] == '(' {
                i += 2; // skip ](
                let mut url = String::new();
                while i < len && chars_vec[i] != ')' {
                    url.push(chars_vec[i]);
                    i += 1;
                }
                if i < len && chars_vec[i] == ')' {
                    i += 1; // skip )
                    result.push_str(&format!(
                        "<img src=\"{}\" alt=\"{}\" style=\"max-width:100%;border-radius:8px;\" />",
                        html_escape(&url),
                        html_escape(&alt_text)
                    ));
                    found_image = true;
                }
            }

            if !found_image {
                let literal: String = chars_vec[start..i.min(len)].iter().collect();
                result.push_str(&html_escape(&literal));
            }
        }
        // Link: [text](url)
        else if ch == '[' {
            let start = i;
            i += 1;
            let mut link_text = String::new();
            let mut found_link = false;

            // Collect link text up to ]
            while i < len && chars_vec[i] != ']' {
                link_text.push(chars_vec[i]);
                i += 1;
            }

            // Check for ](url)
            if i < len && chars_vec[i] == ']' && i + 1 < len && chars_vec[i + 1] == '(' {
                i += 2; // skip ](
                let mut url = String::new();
                while i < len && chars_vec[i] != ')' {
                    url.push(chars_vec[i]);
                    i += 1;
                }
                if i < len && chars_vec[i] == ')' {
                    i += 1; // skip )
                    result.push_str(&format!(
                        "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a>",
                        html_escape(&url),
                        html_escape(&link_text)
                    ));
                    found_link = true;
                }
            }

            if !found_link {
                // Not a valid link, output as literal text
                let literal: String = chars_vec[start..i.min(len)].iter().collect();
                result.push_str(&html_escape(&literal));
            }
        }
        // HTML special chars
        else {
            match ch {
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                _ => result.push(ch),
            }
            i += 1;
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
