/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Text input and send button components

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use crate::context::use_app_context;
use crate::state::{Message, ImageAttachment};
use web_sys::HtmlInputElement;
use shared_protocol::{Attachment, BrowserToolAction, BrowserToolRequestPayload};

/// Attached file metadata and content
#[derive(Debug, Clone, PartialEq)]
struct AttachedFile {
    id: String,
    name: String,
    size: u64,
    file_type: String,
    /// Base64 encoded data (for screenshots or small files)
    data: Option<String>,
}

impl AttachedFile {
    fn formatted_size(&self) -> String {
        let size = self.size as f64;
        if size < 1024.0 {
            format!("{} B", size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else {
            format!("{:.1} MB", size / 1024.0 / 1024.0)
        }
    }

    /// Returns the icon element - thumbnail for screenshots, SVG for files
    fn icon(&self) -> Element {
        // For screenshots with base64 data, show actual thumbnail
        if let Some(ref base64_data) = self.data {
            if self.file_type.starts_with("image/") {
                let src = format!("data:{};base64,{}", self.file_type, base64_data);
                return rsx! {
                    img {
                        class: "file-chip-thumbnail",
                        src: "{src}",
                        alt: "{self.name}",
                    }
                };
            }
        }

        // For regular files, show SVG icon
        rsx! {
            svg {
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                polyline { points: "14 2 14 8 20 8" }
                line { x1: "16", y1: "13", x2: "8", y2: "13" }
                line { x1: "16", y1: "17", x2: "8", y2: "17" }
                polyline { points: "10 9 9 9 8 9" }
            }
        }
    }
}

/// Text input component with auto-expand
#[component]
pub fn TextInput(disabled: bool) -> Element {
    tracing::info!("TextInput component rendered (v4 - Screenshot)");
    let mut ctx = use_app_context();
    let mut input_text = use_signal(String::new);
    let mut rows = use_signal(|| 1usize);
    let mut is_recording = use_signal(|| false);
    
    // File attachment state
    let mut attached_files = use_signal(|| Vec::<AttachedFile>::new());
    
    let has_text = !input_text.read().trim().is_empty();
    let has_files = !attached_files.read().is_empty();
    let can_send = !disabled && (has_text || has_files);

    // Handle file selection - just store file metadata (no content reading)
    let handle_file_change = move |evt: Event<FormData>| {
        let files = evt.files();
        if files.is_empty() {
            return;
        }

        let mut new_files = attached_files.read().clone();
        for file_data in files.iter() {
            let file_name = file_data.name();
            let mime_type = get_mime_type(&file_name);

            new_files.push(AttachedFile {
                id: uuid::Uuid::new_v4().to_string(),
                name: file_name,
                size: file_data.size() as u64,
                file_type: mime_type,
                data: None, // Files don't need base64 data, just metadata
            });
        }
        attached_files.set(new_files);
    };

    // Helper to trigger file input click
    let handle_attach_click = move |_| {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id("hidden-file-input") {
                    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                        input.click();
                    }
                }
            }
        }
    };

    // Handle screenshot click
    let handle_screenshot_click = move |_| {
        let session_id = ctx.session.read().id.clone();
        let tab_id = ctx.tab_context.read().tab_id;

        // Check if tab exists (tab_id should be > 0 for valid tabs)
        if tab_id == 0 {
            tracing::warn!("Cannot take screenshot: no active tab");
            return;
        }

        spawn(async move {
            let request = BrowserToolRequestPayload {
                request_id: uuid::Uuid::new_v4().to_string(),
                session_id,
                tab_id: Some(tab_id as i64),
                action: BrowserToolAction::Screenshot,
                params: serde_json::json!({}),
                timeout_ms: 10000,
            };

            tracing::info!("Requesting screenshot for tab {}...", tab_id);
            match crate::messaging::exec_browser_tool(request).await {
                Ok(response) => {
                    if response.success {
                        if let Some(result) = response.result {
                            // Extract base64 data from various formats
                            let base64_data = if let Some(s) = result.as_str() {
                                // Direct string (either base64 or data URL)
                                if s.starts_with("data:image/") {
                                    // data URL format: data:image/png;base64,<data>
                                    s.split(',').nth(1).map(|d| d.to_string())
                                } else {
                                    Some(s.to_string())
                                }
                            } else if let Some(obj) = result.as_object() {
                                // Object format: try "data_url", "data", or "dataUrl"
                                obj.get("data_url")
                                    .or_else(|| obj.get("dataUrl"))
                                    .or_else(|| obj.get("data"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| {
                                        if s.starts_with("data:image/") {
                                            s.split(',').nth(1).map(|d| d.to_string())
                                        } else {
                                            Some(s.to_string())
                                        }
                                    })
                            } else {
                                None
                            };

                            if let Some(data) = base64_data {
                                let timestamp = js_sys::Date::now() as u64;
                                // Estimate size from base64 (base64 is ~4/3 of original)
                                let size = (data.len() * 3 / 4) as u64;

                                let file = AttachedFile {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    name: format!("Screenshot_{}.png", timestamp),
                                    size,
                                    file_type: "image/png".to_string(),
                                    data: Some(data),
                                };

                                attached_files.write().push(file);
                                tracing::info!("Screenshot added to list, size: {} bytes", size);
                            } else {
                                tracing::warn!("Screenshot result format unknown: {:?}", result);
                            }
                        }
                    } else {
                        tracing::error!("Screenshot failed: {:?}", response.error);
                    }
                },
                Err(e) => tracing::error!("Failed to execute screenshot tool: {}", e),
            }
        });
    };

    let mut handle_remove_file = move |id: String| {
        let mut files = attached_files.read().clone();
        files.retain(|f| f.id != id);
        attached_files.set(files);
    };

    let mut handle_send = move |_: ()| {
        let text = input_text.read().trim().to_string();
        if text.is_empty() && attached_files.read().is_empty() {
            return;
        }

        // Clear input and reset rows
        input_text.set(String::new());
        rows.set(1);

        // Prepare attachment info for display and sending
        let files = attached_files.read().clone();
        attached_files.set(Vec::new()); // Clear attachments

        let mut protocol_attachments = Vec::new();
        let mut message_attachments = Vec::new();

        for file in files {
            // For display in message bubble (all files, with or without data)
            message_attachments.push(ImageAttachment {
                id: file.id.clone(),
                name: file.name.clone(),
                mime_type: file.file_type.clone(),
                data: file.data.clone().unwrap_or_default(), // Empty string for files without data
            });

            // For sending to agent (only files with actual data)
            if let Some(ref data) = file.data {
                protocol_attachments.push(Attachment {
                    name: file.name,
                    mime_type: file.file_type,
                    data: data.clone(),
                });
            }
        }

        // Display text
        let display_text = if text.is_empty() && !message_attachments.is_empty() {
            String::from("") // Empty text, just attachments
        } else {
            text.clone()
        };

        // Debug: log attachment info
        tracing::info!("handle_send: message_attachments count = {}", message_attachments.len());

        // Add user message with attachments
        let message = if message_attachments.is_empty() {
            Message::user(&display_text)
        } else {
            Message::user_with_images(&display_text, message_attachments)
        };
        ctx.messages.write().push(message);

        // Send message (let mock_send_message handle status)
        let session_id = ctx.session.read().id.clone();
        let tab_id = ctx.tab_context.read().tab_id;
        let mock_enabled = ctx.mock_enabled;

        wasm_bindgen_futures::spawn_local(async move {
            if mock_enabled {
                crate::mock::mock_send_message(ctx, display_text).await;
            } else {
                // For real mode, set thinking before sending
                ctx.agent_status.write().set_thinking();
                let _ = crate::messaging::send_chat_message(&session_id, text, protocol_attachments, Some(tab_id)).await;
            }
        });
    };

    // Helper to calculate rows from text
    let calculate_rows = |text: &str| -> usize {
        let line_count = text.lines().count().max(1);
        let trailing = if text.ends_with('\n') { 1 } else { 0 };
        let longest_line = text.lines().map(|l| l.len()).max().unwrap_or(0);
        let estimated_wrap_lines = longest_line / 40;
        let total_rows = (line_count + trailing + estimated_wrap_lines).min(6).max(1);
        total_rows
    };

    let handle_input = move |evt: Event<FormData>| {
        let value = evt.value();
        input_text.set(value.clone());
        rows.set(calculate_rows(&value));
    };

    let handle_keydown = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter {
            if evt.modifiers().shift() {
                let current = input_text.read().clone();
                let new_rows = calculate_rows(&format!("{}\n", current));
                rows.set(new_rows);
            } else if can_send {
                evt.prevent_default();
                handle_send(());
            }
        }
    };

    let current_rows = rows();

    rsx! {
        div { class: "text-input-wrapper",
            // Hidden file input
            input {
                r#type: "file",
                id: "hidden-file-input",
                multiple: true,
                style: "display: none;",
                onchange: handle_file_change,
            }

            // File Attachment List
            {
                let files_list = attached_files.read().clone();
                if !files_list.is_empty() {
                    rsx! {
                        div { class: "file-attachment-list",
                            for file in files_list.iter() {
                                {
                                    let file_id = file.id.clone();
                                    let file_name = file.name.clone();
                                    let file_icon = file.icon();
                                    rsx! {
                                        div { class: "file-chip", key: "{file_id}",
                                            span { class: "file-chip-icon",
                                                {file_icon}
                                            }
                                            div { class: "file-chip-info",
                                                span { class: "file-chip-name", "{file_name}" }
                                            }
                                            button {
                                                class: "file-chip-remove",
                                                onclick: move |_| handle_remove_file(file_id.clone()),
                                                aria_label: "Remove file",
                                                svg {
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! {}
                }
            }

            // Text input area
            textarea {
                class: "text-input",
                class: if current_rows > 1 { "expanded" },
                placeholder: "Message Agentic Chat...",
                disabled: disabled,
                value: "{input_text}",
                oninput: handle_input,
                onkeydown: handle_keydown,
                rows: "{current_rows}",
                aria_label: "Message input",
            }

            // Input toolbar
            div { class: "input-toolbar",
                // Left side: mode selector buttons
                ModeSelector {}

                // Right side: action icons + char count + voice/send button
                div { class: "toolbar-right",
                    // Action icons
                    div { class: "action-icons",
                        // Attachment button
                        button {
                            class: "input-action-btn",
                            disabled: disabled,
                            title: "Attach file",
                            aria_label: "Attach file",
                            onclick: handle_attach_click,
                            svg {
                                width: "16px",
                                height: "16px",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path {
                                    d: "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"
                                }
                            }
                        }

                        // Screenshot button
                        button {
                            class: "input-action-btn",
                            disabled: disabled,
                            title: "Screenshot",
                            aria_label: "Screenshot",
                            onclick: handle_screenshot_click,
                            svg {
                                width: "16px",
                                height: "16px",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                // Camera/Screenshot icon
                                path { d: "M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" }
                                circle { cx: "12", cy: "13", r: "4" }
                            }
                        }
                    }

                    // Combined voice/send button
                    VoiceSendButton {
                        has_text: has_text || has_files,
                        is_recording: is_recording(),
                        disabled: disabled,
                        on_send: move |_| handle_send(()),
                        on_voice_toggle: move |_| {
                            is_recording.set(!is_recording());
                            // TODO: Implement actual voice recording
                        },
                    }
                }
            }
        }
    }
}

/// Agent mode enum
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Chat,
    BrowserUse,
    Agent,
}

/// Mode selector component - Chat, Browser Use, Agent buttons
#[component]
fn ModeSelector() -> Element {
    let mut selected_mode = use_signal(|| AgentMode::Chat);

    rsx! {
        div { class: "mode-selector",
            // Chat mode button
            button {
                class: "mode-btn",
                class: if selected_mode() == AgentMode::Chat { "active" },
                onclick: move |_| selected_mode.set(AgentMode::Chat),
                title: "Chat mode",
                aria_label: "Chat mode",
                // Chat/search icon
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "11", cy: "11", r: "8" }
                    line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                }
            }

            // Browser Use mode button
            button {
                class: "mode-btn",
                class: if selected_mode() == AgentMode::BrowserUse { "active" },
                onclick: move |_| selected_mode.set(AgentMode::BrowserUse),
                title: "Browser Use mode",
                aria_label: "Browser Use mode",
                // Cursor/pointer icon
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 3l7.07 16.97 2.51-7.39 7.39-2.51L3 3z" }
                    path { d: "M13 13l6 6" }
                }
            }

            // Agent mode button
            button {
                class: "mode-btn",
                class: if selected_mode() == AgentMode::Agent { "active" },
                onclick: move |_| selected_mode.set(AgentMode::Agent),
                title: "Agent mode",
                aria_label: "Agent mode",
                // Grid/workflow icon
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect { x: "3", y: "3", width: "7", height: "7" }
                    rect { x: "14", y: "3", width: "7", height: "7" }
                    rect { x: "14", y: "14", width: "7", height: "7" }
                    rect { x: "3", y: "14", width: "7", height: "7" }
                }
            }
        }
    }
}

/// Combined voice/send button component
/// Shows voice icon when no text, send icon when has text
#[component]
fn VoiceSendButton(
    has_text: bool,
    is_recording: bool,
    disabled: bool,
    on_send: EventHandler<()>, 
    on_voice_toggle: EventHandler<()>, 
) -> Element {
    if has_text {
        // Show send button when there's text
        rsx! {
            button {
                class: "voice-send-button send-mode",
                disabled: disabled,
                onclick: move |_| on_send.call(()),
                aria_label: "Send message",
                title: "Send message",

                // Send icon (arrow up)
                svg {
                    width: "20",
                    height: "20",
                    view_box: "0 0 24 24",
                    fill: "currentColor",
                    path {
                        d: "M4 12l1.41 1.41L11 7.83V20h2V7.83l5.58 5.59L20 12l-8-8-8 8z"
                    }
                }
            }
        }
    } else {
        // Show voice button when no text
        rsx! {
            button {
                class: "voice-send-button voice-mode",
                class: if is_recording { "recording" },
                disabled: disabled,
                onclick: move |_| on_voice_toggle.call(()),
                aria_label: if is_recording { "Stop recording" } else { "Start voice input" },
                title: if is_recording { "Stop recording" } else { "Voice input" },

                // Microphone icon
                svg {
                    width: "20",
                    height: "20",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" }
                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                    line { x1: "12", y1: "19", x2: "12", y2: "23" }
                    line { x1: "8", y1: "23", x2: "16", y2: "23" }
                }
            }
        }
    }
}

/// Get MIME type from file extension
fn get_mime_type(filename: &str) -> String {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        // Documents
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Text
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "md" => "text/markdown",
        // Code
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" => "text/x-c++",
        "ts" => "text/typescript",
        "tsx" | "jsx" => "text/javascript",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        // Default
        _ => "application/octet-stream",
    }.to_string()
}
