/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Application state types for the Chat Sidebar

mod session;
pub mod soul;
mod message;
mod agent;
pub mod permission;
mod connection;
mod history;
pub mod loop_state;
mod mcp;
mod ask_user;
mod file_picker;
mod tools;
pub mod render_job;
mod schedule;
pub mod paired_agents;
mod session_route;
mod session_badges;

pub use session::*;
pub use message::*;
pub use agent::*;
pub use permission::*;
pub use connection::*;
pub use history::*;
pub use loop_state::{IterationRow, LoopProposalUi, LoopState};
pub use mcp::*;
pub use ask_user::*;
pub use file_picker::*;
pub use tools::*;
pub use render_job::RenderJobEntry;
pub use schedule::ScheduleJobState;
pub use session_route::{SessionRouting, StreamRoute};
pub use session_badges::{
    is_unread, leave_session, load_seen, mark_seen, mark_watched_end, origin_label, parse_origin,
    parse_runtime, parse_seen, reconcile_seen, save_seen, session_badge, SeenMap, SessionBadge,
    SessionOrigin, SessionRuntime, SEEN_CAP, SEEN_KEY, WATCHED_GRACE_SECS,
};
pub use paired_agents::{
    agent_label, approval_group_name, bar_rows, command_error, for_agent, confirms_guarded, confirms_paused, parse_paired_agents,
    presence_word, rows_from_status, task_banner, task_command_reason, task_for_session, task_line,
    tool_word,
    unpair_click, unpair_label, AgentRow, AgentTask, PairedAgentsSnapshot, TaskApproval,
    TaskApprovalLine, TaskLine, next_paired_agents, AGENT_STATUS_BUFFER, AGENT_STATUS_TOPIC,
    ERROR_LINE_MS, GUARDED_TOGGLE_HELP, GUARDED_TOGGLE_LABEL, MUSE_MAY_ALSO_ANSWER,
    UNPAIR_CONFIRM_MS,
};

/// Skill information for the skill selector
#[derive(Debug, Clone, PartialEq)]
pub struct SkillItem {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Maximize state for sidebar <-> tab mode switching
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaximizeState {
    /// Whether we're in maximized (tab) mode
    pub is_maximized: bool,
    /// The tab ID where the sidebar was opened from (for restore)
    pub source_tab_id: Option<i32>,
    /// The tab ID that the AI agent operates on
    pub target_tab_id: Option<i32>,
    /// Deep-link panel to open on boot (e.g. `panel=jobs` opens the Jobs
    /// panel). Set by the header calendar button's maximize-jump.
    pub panel: Option<String>,
}

pub use soul::{ActiveSoul, SoulSummary};
