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

    rsx! {
        div {
            class: "chat-sidebar",
            id: "nevoflux-chat-sidebar",

            // MCP Config Modal (full-screen when visible)
            if show_mcp_config {
                McpConfigModal {}
            } else {
                // Header with connection status and controls
                Header {}

                // Main content area
                div { class: "chat-content",
                    // Message display area
                    MessageArea {}

                    // Agent status bar (shows when agent is active)
                    AgentStatusBar {}
                }

                // Input area with context bar
                InputArea {}

                // Permission dialog (modal, P0 priority)
                PermissionDialog {}

                // AskUser dialog (modal, for agent questions)
                AskUserDialog {}
            }
        }
    }
}
