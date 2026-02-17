/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Text input and send button components

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use crate::context::use_app_context;
use crate::state::{Message, ImageAttachment, SkillItem};
use shared_protocol::{Attachment, BrowserToolAction, BrowserToolRequestPayload, ChatMode};

#[wasm_bindgen]
extern "C" {
    /// Query tabs from browser.tabs API
    #[wasm_bindgen(js_namespace = ["browser", "tabs"], js_name = query, catch)]
    fn browser_tabs_query(query_info: &JsValue) -> Result<js_sys::Promise, JsValue>;
}

#[wasm_bindgen(inline_js = "
export function base64_to_arraybuffer(base64_data) {
    const bin = atob(base64_data);
    const buf = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
    return buf.buffer;
}
")]
extern "C" {
    /// Decode a base64 string to an ArrayBuffer.
    fn base64_to_arraybuffer(base64_data: &str) -> js_sys::ArrayBuffer;
}

#[wasm_bindgen]
extern "C" {
    /// Copy image data to the system clipboard via Firefox extension API.
    #[wasm_bindgen(js_namespace = ["browser", "clipboard"], js_name = setImageData, catch)]
    fn browser_clipboard_set_image_data(data: &js_sys::ArrayBuffer, image_type: &str) -> Result<js_sys::Promise, JsValue>;
}

/// Attached file metadata and content
#[derive(Debug, Clone, PartialEq)]
struct AttachedFile {
    id: String,
    name: String,
    size: u64,
    file_type: String,
    /// Base64 encoded data (for screenshots or small files)
    data: Option<String>,
    /// Absolute file path (for files from native picker)
    file_path: Option<String>,
    /// Whether this is a directory (from native picker)
    is_directory: bool,
    /// Last modified timestamp (from native picker)
    modified: Option<u64>,
    // Fields for Tab Context
    is_tab: bool,
    tab_id: Option<i64>,
    fav_icon_url: Option<String>,
    /// Space/workspace the tab belongs to (for tabs only)
    tab_space: Option<String>,
}

impl AttachedFile {
    fn formatted_size(&self) -> String {
        if self.is_tab {
            return "Tab".to_string();
        }
        let size = self.size as f64;
        if size < 1024.0 {
            format!("{} B", size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else {
            format!("{:.1} MB", size / 1024.0 / 1024.0)
        }
    }

    /// Returns the icon element - thumbnail for screenshots, SVG for files/tabs
    fn icon(&self) -> Element {
        // Tab icon
        if self.is_tab {
            if let Some(ref icon_url) = self.fav_icon_url {
                return rsx! {
                    img {
                        class: "file-chip-thumbnail",
                        src: "{icon_url}",
                        alt: "{self.name}",
                    }
                };
            }
             return rsx! {
                svg {
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                    line { x1: "3", y1: "9", x2: "21", y2: "9" }
                    line { x1: "9", y1: "21", x2: "9", y2: "9" }
                }
            };
        }

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

/// Tab information for the selector
#[derive(Debug, Clone, PartialEq)]
struct TabItem {
    id: i64,
    title: String,
    url: String,
    fav_icon_url: Option<String>,
    /// The space/workspace this tab belongs to
    space: String,
}

/// Fetch open tabs using browser extension API
async fn fetch_open_tabs() -> Vec<TabItem> {
    tracing::info!("fetch_open_tabs: starting");

    // Build query object: {currentWindow: true}
    let query_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &query_obj,
        &JsValue::from_str("currentWindow"),
        &JsValue::from_bool(true),
    );

    // Call browser.tabs.query via wasm_bindgen binding
    let promise = match browser_tabs_query(&query_obj.into()) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("fetch_open_tabs: browser.tabs.query failed: {:?}", e);
            return Vec::new();
        }
    };

    // Await the promise
    let result = match JsFuture::from(promise).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("fetch_open_tabs: promise rejected: {:?}", e);
            return Vec::new();
        }
    };

    // Parse the array of tabs
    let array = match result.dyn_into::<js_sys::Array>() {
        Ok(a) => a,
        Err(_) => {
            tracing::warn!("fetch_open_tabs: result is not an array");
            return Vec::new();
        }
    };

    let mut items = Vec::new();
    for i in 0..array.length().min(10) {
        // Limit to 10 tabs
        let tab = array.get(i);
        if tab.is_undefined() || tab.is_null() {
            continue;
        }

        let id = js_sys::Reflect::get(&tab, &JsValue::from_str("id"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64;

        let title = js_sys::Reflect::get(&tab, &JsValue::from_str("title"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();

        let url = js_sys::Reflect::get(&tab, &JsValue::from_str("url"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();

        let fav_icon_url = js_sys::Reflect::get(&tab, &JsValue::from_str("favIconUrl"))
            .ok()
            .and_then(|v| v.as_string());

        // Get cookieStoreId as space identifier (Zen Browser uses this for workspaces)
        let space = js_sys::Reflect::get(&tab, &JsValue::from_str("cookieStoreId"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "default".to_string());

        items.push(TabItem {
            id,
            title,
            url,
            fav_icon_url,
            space,
        });
    }

    tracing::info!("fetch_open_tabs: returning {} tabs", items.len());
    items
}


/// Text input component with auto-expand
#[component]
pub fn TextInput(disabled: bool) -> Element {
    tracing::info!("TextInput component rendered (v5 - Tab Selection)");
    let mut ctx = use_app_context();
    let mut input_text = use_signal(String::new);
    let mut rows = use_signal(|| 1usize);
    let mut is_recording = use_signal(|| false);
    
    // File attachment state
    let mut attached_files = use_signal(|| Vec::<AttachedFile>::new());
    
    // Tab selection state
    let mut show_tab_selector = use_signal(|| false);
    let mut available_tabs = use_signal(|| Vec::<TabItem>::new());

    // Skill selection state
    let mut show_skill_selector = use_signal(|| false);
    let mut skill_filter = use_signal(String::new);
    let mut selected_skill_index = use_signal(|| 0usize);

    let has_text = !input_text.read().trim().is_empty();
    let has_files = !attached_files.read().is_empty();
    let can_send = !disabled && (has_text || has_files);

    // Watch for files picked via native file dialog
    use_effect(move || {
        let picked = ctx.picked_files.read().clone();
        if !picked.is_empty() {
            tracing::info!("Processing {} picked files from native dialog", picked.len());
            for file in picked {
                let mime_type = file.mime_type.clone().unwrap_or_else(|| get_mime_type(&file.path));
                attached_files.write().push(AttachedFile {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: file.name().to_string(),
                    size: file.size.unwrap_or(0),
                    file_type: mime_type,
                    data: None, // Native files don't need inline data
                    file_path: Some(file.path.clone()),
                    is_directory: file.is_directory,
                    modified: file.modified,
                    is_tab: false,
                    tab_id: None,
                    fav_icon_url: None,
                    tab_space: None,
                });
            }
            // Clear picked files after processing
            ctx.picked_files.write().clear();
        }
    });

    // Fetch tabs logic
    let refresh_tabs = move || {
        tracing::info!("refresh_tabs: triggered");
        spawn(async move {
            let tabs = fetch_open_tabs().await;
            tracing::info!("refresh_tabs: got {} tabs", tabs.len());
            available_tabs.set(tabs);
        });
    };

    // Fetch skills logic
    let fetch_skills = move || {
        web_sys::console::log_1(&"[NevoFlux] fetch_skills: triggered, sending skill.list".into());
        spawn(async move {
            web_sys::console::log_1(&"[NevoFlux] Calling send_skill_list()...".into());
            match crate::messaging::send_skill_list().await {
                Ok(_) => {
                    web_sys::console::log_1(&"[NevoFlux] send_skill_list() succeeded".into());
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("[NevoFlux] Failed to fetch skills: {}", e).into());
                }
            }
        });
    };

    // Filtered skills based on input (uses ctx.available_skills from handler)
    let filtered_skills = use_memo(move || {
        let filter = skill_filter.read().to_lowercase();
        let skills = ctx.available_skills.read();

        if filter.is_empty() {
            skills.clone()
        } else {
            skills
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().starts_with(&filter)
                        || s.name.to_lowercase().contains(&filter)
                        || s.description.to_lowercase().contains(&filter)
                })
                .cloned()
                .collect()
        }
    });

    // Handle file attachment via native file picker
    let handle_attach_click = move |_| {
        // Check if already waiting for file picker
        if ctx.pending_file_pick.read().is_some() {
            tracing::warn!("File picker already pending");
            return;
        }

        spawn(async move {
            match crate::messaging::send_pick_files_request(
                "both", // mode: files, directories, or both
                true,   // multiple
                Some("Select files".to_string()),
            ).await {
                Ok(request_id) => {
                    tracing::info!("File picker request sent: {}", request_id);
                    ctx.pending_file_pick.write().replace(crate::state::PendingFilePick { request_id });
                }
                Err(e) => {
                    tracing::error!("Failed to send pick files request: {}", e);
                }
            }
        });
    };

    let handle_screenshot_click = move |_| {
        web_sys::console::log_1(&"[NevoFlux] Screenshot button clicked".into());

        let tab_context = ctx.tab_context.read();
        let session_id = tab_context.zen_sync_id.clone()
            .unwrap_or_else(|| ctx.session.read().id.clone());
        let tab_id = tab_context.tab_id;
        drop(tab_context);

        web_sys::console::log_1(&format!("[NevoFlux] Screenshot: tab_id={}, session_id={}", tab_id, session_id).into());

        if tab_id == 0 {
            web_sys::console::warn_1(&"[NevoFlux] Cannot take screenshot: no active tab (tab_id=0)".into());
            tracing::warn!("Cannot take screenshot: no active tab");
            return;
        }

        spawn(async move {
            web_sys::console::log_1(&"[NevoFlux] Sending screenshot request...".into());

            let request = BrowserToolRequestPayload {
                request_id: uuid::Uuid::new_v4().to_string(),
                session_id,
                tab_id: Some(tab_id as i64),
                action: BrowserToolAction::Screenshot,
                params: serde_json::json!({}),
                timeout_ms: 10000,
            };

            match crate::messaging::exec_browser_tool(request).await {
                Ok(response) => {
                    web_sys::console::log_1(&format!("[NevoFlux] Screenshot response: success={}", response.success).into());

                    if response.success {
                        if let Some(result) = response.result {
                            web_sys::console::log_1(&format!("[NevoFlux] Screenshot result type: {:?}", result).into());

                            // Extract base64 data and detect image type from data URL
                            let mut detected_type = "jpeg"; // default: captureVisibleTab returns jpeg
                            let base64_data = if let Some(s) = result.as_str() {
                                if s.starts_with("data:image/") {
                                    // e.g. "data:image/jpeg;base64,/9j/4AAQ..."
                                    if let Some(header) = s.split(',').next() {
                                        if header.contains("image/png") { detected_type = "png"; }
                                    }
                                    s.split(',').nth(1).map(|d| d.to_string())
                                } else {
                                    Some(s.to_string())
                                }
                            } else if let Some(obj) = result.as_object() {
                                let raw = obj.get("data_url")
                                    .or_else(|| obj.get("dataUrl"))
                                    .or_else(|| obj.get("data"))
                                    .and_then(|v| v.as_str());
                                raw.and_then(|s| {
                                    if s.starts_with("data:image/") {
                                        if let Some(header) = s.split(',').next() {
                                            if header.contains("image/png") { detected_type = "png"; }
                                        }
                                        s.split(',').nth(1).map(|d| d.to_string())
                                    } else {
                                        Some(s.to_string())
                                    }
                                })
                            } else {
                                web_sys::console::warn_1(&"[NevoFlux] Screenshot result is neither string nor object".into());
                                None
                            };

                            let mime_type = format!("image/{}", detected_type);
                            let ext = detected_type;

                            if let Some(data) = base64_data {
                                let timestamp = js_sys::Date::now() as u64;
                                let size = (data.len() * 3 / 4) as u64;

                                web_sys::console::log_1(&format!("[NevoFlux] Screenshot captured: {} bytes, type={}", size, mime_type).into());

                                // Copy to clipboard as a real image via browser.clipboard.setImageData
                                let array_buffer = base64_to_arraybuffer(&data);
                                match browser_clipboard_set_image_data(&array_buffer, ext) {
                                    Ok(promise) => {
                                        match wasm_bindgen_futures::JsFuture::from(promise).await {
                                            Ok(_) => {
                                                web_sys::console::log_1(&"[NevoFlux] Screenshot copied to clipboard".into());
                                            }
                                            Err(e) => {
                                                web_sys::console::warn_1(&format!("[NevoFlux] Clipboard write failed: {:?}", e).into());
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        web_sys::console::warn_1(&format!("[NevoFlux] browser.clipboard.setImageData not available: {:?}", e).into());
                                    }
                                }

                                attached_files.write().push(AttachedFile {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    name: format!("Screenshot_{}.{}", timestamp, ext),
                                    size,
                                    file_type: mime_type,
                                    data: Some(data),
                                    file_path: None,
                                    is_directory: false,
                                    modified: None,
                                    is_tab: false,
                                    tab_id: None,
                                    fav_icon_url: None,
                                    tab_space: None,
                                });
                            } else {
                                web_sys::console::warn_1(&"[NevoFlux] Could not extract base64 data from screenshot result".into());
                            }
                        } else {
                            web_sys::console::warn_1(&"[NevoFlux] Screenshot response has no result".into());
                        }
                    } else {
                        let error_msg = response.error.map(|e| e.message).unwrap_or_else(|| "Unknown error".to_string());
                        web_sys::console::error_1(&format!("[NevoFlux] Screenshot failed: {}", error_msg).into());
                    }
                },
                Err(e) => {
                    web_sys::console::error_1(&format!("[NevoFlux] exec_browser_tool failed: {}", e).into());
                    tracing::error!("Failed to execute screenshot tool: {}", e);
                }
            }
        });
    };

    let mut handle_remove_file = move |id: String| {
        let mut files = attached_files.read().clone();
        files.retain(|f| f.id != id);
        attached_files.set(files);
    };

    let mut handle_select_tab = move |tab: TabItem| {
        // Remove trailing @ from input
        let current_text = input_text.read().clone();
        if current_text.ends_with('@') {
            input_text.set(current_text[..current_text.len() - 1].to_string());
        }

        // Add tab as attachment (no caching, just store tab info)
        attached_files.write().push(AttachedFile {
             id: uuid::Uuid::new_v4().to_string(),
             name: tab.title.clone(),
             size: 0,
             file_type: "tab/reference".to_string(),
             data: None,
             file_path: None,
             is_directory: false,
             modified: None,
             is_tab: true,
             tab_id: Some(tab.id),
             fav_icon_url: tab.fav_icon_url.clone(),
             tab_space: Some(tab.space.clone()),
        });

        show_tab_selector.set(false);
    };

    let mut handle_select_skill = move |skill: SkillItem| {
        // Set input to /skillname (with space for easy arg typing)
        input_text.set(format!("/{} ", skill.name));

        // Close skill selector
        show_skill_selector.set(false);
        skill_filter.set(String::new());
        selected_skill_index.set(0);
    };

    let mut handle_send = move |_: ()| {
        let text = input_text.read().trim().to_string();
        if text.is_empty() && attached_files.read().is_empty() {
            return;
        }

        input_text.set(String::new());
        rows.set(1);
        show_tab_selector.set(false);

        let files = attached_files.read().clone();
        attached_files.set(Vec::new());

        // Separate images (attachments), local files, and tab references
        let mut protocol_attachments = Vec::new(); // For images (base64)
        let mut local_files = Vec::new(); // For files from native picker
        let mut tab_ids = Vec::new(); // For tab references
        let mut message_attachments = Vec::new(); // For UI display

        for file in files {
            // Add to UI display list
            message_attachments.push(ImageAttachment {
                id: file.id.clone(),
                name: file.name.clone(),
                mime_type: file.file_type.clone(),
                data: file.data.clone().unwrap_or_default(),
            });

            if file.is_tab {
                // Tab reference: add to tab_ids
                if let Some(tab_id) = file.tab_id {
                    tab_ids.push(shared_protocol::chat::TabReference {
                        space: file.tab_space.clone().unwrap_or_else(|| "default".to_string()),
                        tab_id,
                        tab_title: file.name.clone(),
                    });
                }
            } else if file.file_type.starts_with("image/") && file.data.is_some() {
                // Image with inline data (screenshot/paste): send as base64 attachment
                protocol_attachments.push(Attachment {
                    name: file.name,
                    mime_type: file.file_type,
                    data: file.data,
                    file_path: None,
                });
            } else {
                // Local file from native picker (including images without inline data):
                // send path so agent can read the file directly
                if let Some(ref path) = file.file_path {
                    local_files.push(shared_protocol::FileInfo {
                        path: path.clone(),
                        is_directory: file.is_directory,
                        size: Some(file.size),
                        modified: file.modified,
                        mime_type: Some(file.file_type.clone()),
                    });
                }
            }
        }

        let display_text = if text.is_empty() && !message_attachments.is_empty() {
            String::from("") 
        } else {
            text.clone()
        };

        let message = if message_attachments.is_empty() {
            Message::user(&display_text)
        } else {
            Message::user_with_images(&display_text, message_attachments)
        };
        ctx.messages.write().push(message);

        let tab_context = ctx.tab_context.read();
        let session_id = tab_context.zen_sync_id.clone()
            .unwrap_or_else(|| ctx.session.read().id.clone());
        let tab_id = tab_context.tab_id;
        drop(tab_context);
        let mock_enabled = ctx.mock_enabled;
        let mode = ctx.chat_mode.read().clone();

        wasm_bindgen_futures::spawn_local(async move {
            if mock_enabled {
                crate::mock::mock_send_message(ctx, display_text).await;
            } else {
                ctx.agent_status.write().set_thinking();
                let _ = crate::messaging::send_chat_message(&session_id, text, mode, protocol_attachments, local_files, Some(tab_id), tab_ids).await;
            }
        });
    };

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

        // Check for @ trigger (tab selector)
        if value.ends_with('@') {
            tracing::info!("handle_input: @ detected, showing tab selector");
            refresh_tabs();
            show_tab_selector.set(true);
            show_skill_selector.set(false);
        } else if !value.contains('@') {
             // Close if @ is removed
             if show_tab_selector() {
                 tracing::info!("handle_input: @ removed, hiding tab selector");
             }
             show_tab_selector.set(false);
        }

        // Check for / trigger (skill selector)
        // Only trigger if starts with / and no space yet (still typing skill name)
        if value.starts_with('/') && !value.contains(' ') {
            // Extract filter text: "/s" -> "s", "/" -> ""
            let filter = if value.len() > 1 {
                value[1..].to_lowercase()
            } else {
                String::new()
            };

            skill_filter.set(filter);

            // First time entering / - fetch skills
            if value == "/" {
                web_sys::console::log_1(&"[NevoFlux] handle_input: / detected, calling fetch_skills()".into());
                fetch_skills();
            }

            show_skill_selector.set(true);
            selected_skill_index.set(0);
            show_tab_selector.set(false);
        } else if show_skill_selector() {
            // Close skill selector if no longer typing /skillname
            show_skill_selector.set(false);
        }

        input_text.set(value.clone());
        rows.set(calculate_rows(&value));
    };

    let handle_keydown = move |evt: KeyboardEvent| {
        // Skill selector keyboard navigation
        if show_skill_selector() {
            let skills = filtered_skills.read();
            let current_index = selected_skill_index();

            match evt.key() {
                Key::ArrowDown => {
                    evt.prevent_default();
                    if current_index < skills.len().saturating_sub(1) {
                        selected_skill_index.set(current_index + 1);
                    }
                }
                Key::ArrowUp => {
                    evt.prevent_default();
                    if current_index > 0 {
                        selected_skill_index.set(current_index - 1);
                    }
                }
                Key::Enter | Key::Tab => {
                    evt.prevent_default();
                    if let Some(skill) = skills.get(current_index) {
                        handle_select_skill(skill.clone());
                    }
                }
                Key::Escape => {
                    evt.prevent_default();
                    show_skill_selector.set(false);
                }
                _ => {}
            }
            return;
        }

        // Tab selector keyboard navigation
        if show_tab_selector() {
            if evt.key() == Key::Escape {
                evt.prevent_default();
                show_tab_selector.set(false);
                return;
            }
        }

        // Normal input handling
        if evt.key() == Key::Enter {
            if evt.modifiers().shift() {
                let current = input_text.read().clone();
                let new_rows = calculate_rows(&format!("{}\n", current));
                rows.set(new_rows);
            } else if can_send && !show_tab_selector() {
                evt.prevent_default();
                handle_send(());
            }
        } else if evt.key() == Key::Escape {
            if show_tab_selector() {
                show_tab_selector.set(false);
            }
        }
    };

    let current_rows = rows();

    rsx! {
        div { class: "text-input-wrapper",
            // Tab Selector Popup
            if show_tab_selector() {
                div { class: "tab-selector-popup",
                    for tab in available_tabs.read().iter().cloned() {
                        {
                            let tab_item = tab.clone();
                            let title = tab.title.clone();
                            let url = tab.url.clone();
                            let icon = tab.fav_icon_url.clone().unwrap_or_default();
                            rsx! {
                                div { class: "tab-selector-item",
                                    onclick: move |_| handle_select_tab(tab_item.clone()),
                                    if !icon.is_empty() {
                                        img { class: "tab-selector-icon", src: "{icon}" }
                                    } else {
                                        svg { class: "tab-selector-icon",
                                            view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                                            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                                        }
                                    }
                                    div { class: "tab-selector-info",
                                        span { class: "tab-selector-title", "{title}" }
                                        span { class: "tab-selector-url", "{url}" }
                                    }
                                }
                            }
                        }
                    }
                    if available_tabs.read().is_empty() {
                        div { class: "tab-selector-item", style: "cursor: default;",
                            span { class: "tab-selector-url", "No tabs found..." }
                        }
                    }
                }
            }

            // Skill Selector Popup
            if show_skill_selector() {
                div { class: "skill-selector-popup",
                    div { class: "skill-selector-header",
                        span { class: "skill-selector-title", "Skills" }
                        span { class: "skill-selector-hint", "↑↓ to navigate, Enter to select" }
                    }

                    div { class: "skill-selector-list",
                        for (index, skill) in filtered_skills.read().iter().enumerate() {
                            {
                                let skill_item = skill.clone();
                                let is_selected = index == selected_skill_index();
                                let name = skill.name.clone();
                                let description = skill.description.clone();

                                rsx! {
                                    div {
                                        class: if is_selected { "skill-selector-item selected" } else { "skill-selector-item" },
                                        onclick: move |_| handle_select_skill(skill_item.clone()),
                                        onmouseenter: move |_| selected_skill_index.set(index),

                                        div { class: "skill-selector-icon",
                                            svg {
                                                view_box: "0 0 24 24",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path { d: "M13 2L3 14h9l-1 8 10-12h-9l1-8z" }
                                            }
                                        }

                                        div { class: "skill-selector-info",
                                            span { class: "skill-selector-name", "/{name}" }
                                            span { class: "skill-selector-desc", "{description}" }
                                        }
                                    }
                                }
                            }
                        }

                        if filtered_skills.read().is_empty() {
                            div { class: "skill-selector-empty",
                                if ctx.available_skills.read().is_empty() {
                                    "Loading skills..."
                                } else {
                                    "No matching skills"
                                }
                            }
                        }
                    }
                }
            }

            // File Attachment List
            if !attached_files.read().is_empty() {
                div { class: "file-attachment-list",
                    for file in attached_files.read().iter().cloned() {
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

            // Text input area
            textarea {
                class: "text-input",
                class: if current_rows > 1 { "expanded" },
                placeholder: "Message Agentic Chat... (/ skills, @ tabs)",
                disabled: disabled,
                value: "{input_text}",
                oninput: handle_input,
                onkeydown: handle_keydown,
                rows: "{current_rows}",
                aria_label: "Message input",
            }

            // Input toolbar
            div { class: "input-toolbar",
                ModeSelector {}

                div { class: "toolbar-right",
                    div { class: "action-icons",
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
                                path { d: "M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" }
                                circle { cx: "12", cy: "13", r: "4" }
                            }
                        }
                    }

                    VoiceSendButton {
                        has_text: has_text || has_files,
                        is_recording: is_recording(),
                        disabled: disabled,
                        on_send: move |_| handle_send(()),
                        on_voice_toggle: move |_| {
                            is_recording.set(!is_recording());
                        },
                    }
                }
            }
        }
    }
}

/// Mode selector component - Chat, Browser Use, Agent buttons
/// Connects to ctx.chat_mode to persist the selected mode for outgoing messages
#[component]
fn ModeSelector() -> Element {
    let mut ctx = use_app_context();
    let current_mode = ctx.chat_mode.read().clone();

    rsx! {
        div { class: "mode-selector",
            button {
                class: "mode-btn",
                class: if current_mode == ChatMode::Chat { "active" },
                onclick: move |_| ctx.chat_mode.set(ChatMode::Chat),
                title: "Chat mode",
                aria_label: "Chat mode",
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

            button {
                class: "mode-btn",
                class: if current_mode == ChatMode::Browser { "active" },
                onclick: move |_| ctx.chat_mode.set(ChatMode::Browser),
                title: "Browser Use mode",
                aria_label: "Browser Use mode",
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

            button {
                class: "mode-btn",
                class: if current_mode == ChatMode::Agent { "active" },
                onclick: move |_| ctx.chat_mode.set(ChatMode::Agent),
                title: "Agent mode",
                aria_label: "Agent mode",
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

/// Combined voice/send/stop button component
///
/// Shows a Stop button while the agent is actively working (thinking, executing,
/// or streaming), a Send button when there is text input, and a Voice button
/// otherwise.
#[component]
fn VoiceSendButton(
    has_text: bool,
    is_recording: bool,
    disabled: bool,
    on_send: EventHandler<()>,
    on_voice_toggle: EventHandler<()>,
) -> Element {
    let mut ctx = use_app_context();
    let agent_active = ctx.agent_status.read().is_active();
    let is_streaming = ctx.streaming.read().is_some();
    let show_stop = agent_active || is_streaming;

    if show_stop {
        // Stop button mode: agent is working
        let handle_stop = move |_| {
            if ctx.mock_enabled {
                crate::mock::stop_mock_streaming();
            } else {
                let session_id = ctx.tab_context.read().zen_sync_id.clone()
                    .unwrap_or_else(|| ctx.session.read().id.clone());
                spawn(async move {
                    let _ = crate::messaging::send_stop_generation(&session_id).await;
                });
                ctx.agent_status.write().hide();
                ctx.streaming.set(None);
            }
        };

        rsx! {
            button {
                class: "voice-send-button stop-mode",
                onclick: handle_stop,
                aria_label: "Stop generation",
                title: "Stop generation",
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "currentColor",
                    rect {
                        x: "6",
                        y: "6",
                        width: "12",
                        height: "12",
                        rx: "2",
                    }
                }
            }
        }
    } else if has_text {
        rsx! {
            button {
                class: "voice-send-button send-mode",
                disabled: disabled,
                onclick: move |_| on_send.call(()),
                aria_label: "Send message",
                title: "Send message",
                svg {
                    width: "16",
                    height: "16",
                    view_box: "0 0 24 24",
                    fill: "currentColor",
                    path {
                        d: "M4 12l1.41 1.41L11 7.83V20h2V7.83l5.58 5.59L20 12l-8-8-8 8z"
                    }
                }
            }
        }
    } else {
        rsx! {
            button {
                class: "voice-send-button voice-mode",
                class: if is_recording { "recording" },
                disabled: disabled,
                onclick: move |_| on_voice_toggle.call(()),
                aria_label: if is_recording { "Stop recording" } else { "Start voice input" },
                title: if is_recording { "Stop recording" } else { "Voice input" },
                svg {
                    width: "16",
                    height: "16",
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
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "md" => "text/markdown",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" => "text/x-c++",
        "ts" => "text/typescript",
        "tsx" | "jsx" => "text/javascript",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        _ => "application/octet-stream",
    }.to_string()
}
