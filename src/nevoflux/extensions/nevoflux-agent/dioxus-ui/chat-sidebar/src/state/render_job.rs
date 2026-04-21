//! Client-side view of a render job: what the sidebar knows about it
//! right now. Built incrementally from jobs:render:{id} EventBus
//! deliveries handled in messaging/handler.rs.

use shared_protocol::RenderJobState;

#[derive(Debug, Clone, PartialEq)]
pub struct RenderJobEntry {
    pub job_id: String,
    /// Most recent progress count. For pending jobs (no progress event
    /// yet) this stays 0.
    pub current: u32,
    /// Total frames declared by the render. 0 before first progress
    /// event is seen.
    pub total: u32,
    pub state: RenderJobState,
    /// Set on Succeeded terminal.
    pub output_path: Option<String>,
    /// Set on Failed terminal.
    pub error: Option<String>,
    /// Milliseconds since unix epoch, updated on any delivery. Used
    /// by the card's soft "stalled > 180s" UX indicator.
    pub last_update_ms: u64,
}

impl RenderJobEntry {
    pub fn new_running(job_id: impl Into<String>, now_ms: u64) -> Self {
        Self {
            job_id: job_id.into(),
            current: 0,
            total: 0,
            state: RenderJobState::Running,
            output_path: None,
            error: None,
            last_update_ms: now_ms,
        }
    }

    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            0
        } else {
            ((self.current as u64 * 100) / self.total as u64) as u32
        }
    }
}
