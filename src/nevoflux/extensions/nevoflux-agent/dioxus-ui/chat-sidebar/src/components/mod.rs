/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! UI components for the Chat Sidebar

mod header;
mod message_area;
mod input_area;
mod agent_status;
mod permission_dialog;
pub mod loop_ui;
mod mcp_config;
mod ask_user_dialog;
mod plan_card;
mod render_progress_card;
mod artifact_card;
mod history_panel;
mod tool_auth_dialog;
mod onboarding_screen;
mod rail;
mod event_bus_listener;
pub mod connection_status_bar;

pub use header::Header;
pub use message_area::{ActivityFeed, LiveToolFeed, MessageArea, MessageBubble, WelcomeScreen};
pub use input_area::InputArea;
pub use agent_status::AgentStatusBar;
pub use permission_dialog::PermissionDialog;
pub use mcp_config::McpConfigModal;
pub use ask_user_dialog::AskUserDialog;
pub use plan_card::PlanCard;
pub use render_progress_card::RenderProgressCard;
pub use artifact_card::ArtifactCard;
pub use history_panel::HistoryPanel;
pub use tool_auth_dialog::ToolAuthDialog;
pub use onboarding_screen::OnboardingScreen;
pub use rail::Rail;
pub use event_bus_listener::EventBusListener;
pub use connection_status_bar::ConnectionStatusBar;
