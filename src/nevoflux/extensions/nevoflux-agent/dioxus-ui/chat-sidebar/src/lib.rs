/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! NevoFlux Chat Sidebar - Dioxus WASM UI
//!
//! This crate implements the main chat interface displayed in the browser sidebar.
//! It uses Dioxus signals for reactive state management and communicates with
//! the background script via the WebExtension messaging API.

mod components;
mod hooks;
mod messaging;
mod state;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

pub use components::*;
pub use hooks::*;
pub use messaging::*;
pub use state::*;

/// Initialize and launch the Chat Sidebar application
#[wasm_bindgen(start)]
pub fn main() {
    // Initialize tracing for WASM
    tracing_wasm::set_as_global_default();
    tracing::info!("NevoFlux Chat Sidebar initializing...");

    // Launch Dioxus app
    dioxus::launch(App);
}

/// Root application component
#[component]
fn App() -> Element {
    // Global application state using signals
    let app_state = use_signal(AppState::default);
    let messages = use_signal(Vec::<ChatMessage>::new);
    let streaming_message = use_signal(|| None::<StreamingMessage>);
    let tab_context = use_signal(TabContext::default);
    let connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let agent_status = use_signal(AgentStatus::new);

    // Initialize messaging on mount
    use_effect(move || {
        spawn(async move {
            if let Err(e) = init_messaging(
                app_state,
                messages,
                streaming_message,
                tab_context,
                connection_status,
                agent_status,
            ).await {
                tracing::error!("Failed to initialize messaging: {:?}", e);
            }
        });
    });

    // Request initial tab context
    use_effect(move || {
        spawn(async move {
            request_tab_context().await;
        });
    });

    rsx! {
        div {
            class: "chat-sidebar",
            id: "nevoflux-chat-sidebar",

            // Header with connection status
            Header {
                connection_status: connection_status(),
                tab_context: tab_context(),
            }

            // Message list
            MessageList {
                messages: messages(),
                streaming_message: streaming_message(),
            }

            // Agent status display
            AgentStatusDisplay {
                status: agent_status,
            }

            // Input area
            InputArea {
                disabled: connection_status() != ConnectionStatus::Connected,
                on_send: move |text: String| {
                    spawn(async move {
                        send_chat_message(text, messages, app_state, agent_status).await;
                    });
                },
                tab_context: tab_context(),
            }

            // Status bar
            StatusBar {
                status: connection_status(),
            }
        }
    }
}
