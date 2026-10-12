/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! NevoFlux Chat Sidebar - Dioxus WASM UI
//!
//! This crate implements the main chat interface displayed in the browser sidebar.
//! It uses Dioxus signals for reactive state management and communicates with
//! the background script via the WebExtension messaging API.
//!
//! ## Architecture
//!
//! - `state/` - State types (Session, Message, Agent, Permission, Connection)
//! - `context.rs` - Context Provider with global signals
//! - `components/` - UI components (Header, MessageArea, InputArea, etc.)
//! - `messaging/` - WebExtension messaging bridge and handlers
//! - `mock/` - Mock mode for development/testing
//! - `utils/` - Utility functions

pub mod bindings;
mod components;
mod context;
mod messaging;
mod mock;
mod state;
mod utils;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

pub use components::*;
pub use context::*;
pub use messaging::*;
pub use mock::*;
pub use state::*;
pub use utils::*;

/// Initialize and launch the Chat Sidebar application
/// (wasm-only: the exported `main` collides with the native test harness's
/// entry point, which would break `cargo test` on the host target)
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn main() {
    // Set up panic hook for better error messages in WASM
    console_error_panic_hook::set_once();

    // Initialize tracing for WASM
    tracing_wasm::set_as_global_default();
    tracing::info!("NevoFlux Chat Sidebar initializing...");

    // Launch Dioxus app
    dioxus::launch(App);
}

/// Root application component
#[component]
fn App() -> Element {
    // Check if mock mode is enabled via URL parameter
    let mock_enabled = is_mock_mode();

    rsx! {
        // Context Provider wraps the entire app
        ContextProvider {
            mock_enabled: mock_enabled,
            ChatSidebar {}
        }
    }
}

/// Main chat sidebar layout
#[component]
fn ChatSidebar() -> Element {
    let ctx = use_app_context();
    let show_mcp_config = *ctx.show_mcp_config.read();
    let is_minimized = *ctx.minimized.read();
    let is_maximized = ctx.maximize.read().is_maximized;
    let first_run = *ctx.first_run.read();
    let has_configured = *ctx.has_configured_provider.read();

    // First-launch onboarding: show OnboardingScreen instead of normal UI
    if first_run && !has_configured {
        return rsx! {
            OnboardingScreen {}
        };
    }

    rsx! {
        div {
            class: if is_minimized { "chat-sidebar chat-sidebar--minimized" } else { "chat-sidebar" },
            id: "nevoflux-chat-sidebar",

            if is_minimized {
                // Minimized rail mode
                Rail {}
            } else if show_mcp_config {
                // MCP Config Modal (full-screen when visible)
                McpConfigModal {}
            } else if is_maximized {
                // Maximized (tab) form: left rail + main column, horizontal flex.
                div { class: "sidebar-row",
                    LeftMenu {}
                    div { class: "sidebar-main-col",
                        SidebarContent {}
                    }
                }
            } else {
                // Sidebar form: main column only (no left rail).
                SidebarContent {}
            }
        }
    }
}

/// Shared sidebar body (status bar, header, content, input, dialogs).
///
/// Rendered directly in the sidebar form and wrapped in `.sidebar-main-col`
/// (next to `LeftMenu`) in the maximized form, so the Jobs panel and every
/// dialog work identically in both.
#[component]
fn SidebarContent() -> Element {
    rsx! {
        // Connection status bar at the top (shows reconnecting/error states)
        ConnectionStatusBar {}

        // Paired AI agents: state, pending approval, Pause/Resume/Unpair (M2b §3)
        PairedAgentBar {}

        // Header with connection status and controls
        Header {}

        // Main content area
        div { class: "chat-content",
            // Message display area
            MessageArea {}

            // History panel overlay
            HistoryPanel {}

            // Jobs panel overlay (scheduled background jobs)
            JobsPanel {}

            // Loop Jobs panel overlay (running loops; maximized mode only)
            LoopsPanel {}
        }

        // Input area with context bar
        InputArea {}

        // Permission dialog (modal, P0 priority)
        PermissionDialog {}

        // Tool authorization dialog (modal, for tool-level auth)
        ToolAuthDialog {}

        // Skills-update dialog (modal, replace/keep bundled skills)
        SkillsUpdateDialog {}

        // AskUser dialog (modal, for agent questions)
        AskUserDialog {}

        // EventBus notification toasts (bottom-right overlay)
        EventBusListener {}
    }
}
