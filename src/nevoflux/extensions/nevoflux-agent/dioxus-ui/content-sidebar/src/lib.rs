/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! NevoFlux Content Sidebar - Invisible Background Agent
//!
//! This crate implements an invisible content agent that gets injected into web pages.
//! It operates silently in the background to:
//! - Execute DOM operations (click, type, scroll, etc.) from Chat Sidebar commands
//! - Extract page context (interactive elements, text content)
//! - Provide visual feedback via element highlighting (not UI rendering)
//!
//! No visible UI is rendered - only temporary element highlights during tool execution.

mod components;
mod messaging;
mod page_context;
mod shadow_host;
mod state;
mod tool_executor;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

// Only export what's needed for the invisible agent
pub use messaging::*;
pub use page_context::*;
pub use state::*;
pub use tool_executor::*;

/// Initialize the Content Sidebar (Invisible Agent Mode)
///
/// This function is called from the content script to launch the Dioxus app.
/// The agent runs invisibly - no UI is rendered, only message handling.
#[wasm_bindgen]
pub fn init_content_sidebar() -> Result<(), JsValue> {
    // Initialize tracing for WASM
    tracing_wasm::set_as_global_default();
    tracing::info!("NevoFlux Content Agent initializing (invisible mode)...");

    // No CSS injection - we're invisible

    // Launch Dioxus app - it will find and mount to #main element
    tracing::info!("Launching invisible agent...");
    dioxus::launch(ContentSidebar);

    Ok(())
}

/// Root Content Sidebar component (Invisible Agent)
///
/// This component renders nothing visible. It only:
/// - Initializes message handling with the background script
/// - Processes tool execution requests
/// - Reports page context when requested
#[component]
fn ContentSidebar() -> Element {
    // State signals for internal tracking (no UI display)
    let sidebar_state = use_signal(ContentSidebarState::default);
    let display_content = use_signal(|| None::<DisplayContent>);
    let is_visible = use_signal(|| true);

    // Initialize messaging on mount
    use_effect(move || {
        spawn(async move {
            if let Err(e) = init_content_messaging(
                sidebar_state,
                display_content,
                is_visible,
            ).await {
                tracing::error!("Failed to initialize content messaging: {:?}", e);
            }

            // Report ready status
            report_ready().await;
        });
    });

    // Report URL on page load
    use_effect(move || {
        spawn(async move {
            report_current_url().await;
        });
    });

    // Invisible agent - render nothing
    // All interactions happen through message handlers in init_content_messaging
    rsx! {}
}

/// Get current page URL
fn get_current_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_default()
}

/// Get current page title
fn get_current_title() -> String {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.title().into())
        .unwrap_or_default()
}
