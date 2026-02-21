/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! History session state management

/// Summary of a historical session
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionSummary {
    /// Session ID
    pub id: String,
    /// Session title (may be auto-generated from first message)
    pub title: Option<String>,
    /// Last update timestamp
    pub updated_at: u64,
    /// Number of messages in the session
    pub message_count: u32,
    /// Whether the session is pinned
    pub pinned: bool,
}

impl SessionSummary {
    /// Get display title (falls back to truncated ID if no title)
    pub fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            if self.id.len() > 20 {
                format!("{}...", &self.id[..20])
            } else {
                self.id.clone()
            }
        })
    }

    /// Get relative time string (e.g., "2 hours ago")
    pub fn relative_time(&self) -> String {
        let now = js_sys::Date::now() as u64;
        let diff_ms = now.saturating_sub(self.updated_at * 1000); // updated_at is in seconds
        let diff_secs = diff_ms / 1000;

        if diff_secs < 60 {
            "Just now".to_string()
        } else if diff_secs < 3600 {
            let mins = diff_secs / 60;
            format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
        } else if diff_secs < 86400 {
            let hours = diff_secs / 3600;
            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
        } else {
            let days = diff_secs / 86400;
            format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
        }
    }

    /// Get date group label for session grouping
    pub fn date_group(&self) -> &'static str {
        let now = js_sys::Date::now() as u64;
        let diff_ms = now.saturating_sub(self.updated_at * 1000);
        let diff_secs = diff_ms / 1000;

        if diff_secs < 86400 {
            "Today"
        } else if diff_secs < 172800 {
            "Yesterday"
        } else if diff_secs < 604800 {
            "This Week"
        } else if diff_secs < 2592000 {
            "This Month"
        } else {
            "Older"
        }
    }
}

/// History list state
#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    /// List of session summaries
    pub sessions: Vec<SessionSummary>,
    /// Total number of sessions available
    pub total: u32,
    /// Whether we're currently loading (initial / replace)
    pub loading: bool,
    /// Whether we're loading more (append mode)
    pub loading_more: bool,
    /// Error message if loading failed
    pub error: Option<String>,
}

impl HistoryState {
    /// Create new empty history state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set loading state (replaces sessions on response)
    pub fn set_loading(&mut self) {
        self.loading = true;
        self.loading_more = false;
        self.error = None;
    }

    /// Set loading-more state (appends sessions on response)
    pub fn set_loading_more(&mut self) {
        self.loading_more = true;
        self.error = None;
    }

    /// Set loaded sessions (replace mode)
    pub fn set_sessions(&mut self, sessions: Vec<SessionSummary>, total: u32) {
        self.sessions = sessions;
        self.total = total;
        self.loading = false;
        self.loading_more = false;
        self.error = None;
    }

    /// Append loaded sessions (load-more mode)
    pub fn append_sessions(&mut self, sessions: Vec<SessionSummary>, total: u32) {
        self.total = total;
        self.loading = false;
        self.loading_more = false;
        self.error = None;
        // Deduplicate by ID before appending
        for session in sessions {
            if !self.sessions.iter().any(|s| s.id == session.id) {
                self.sessions.push(session);
            }
        }
    }

    /// Set error state
    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.loading_more = false;
        self.error = Some(error);
    }

    /// Check if there are any sessions
    pub fn has_sessions(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Remove a session by ID
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.retain(|s| s.id != session_id);
        if self.total > 0 {
            self.total -= 1;
        }
    }

    /// Update the title of a session
    pub fn update_title(&mut self, session_id: &str, title: &str) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.title = Some(title.to_string());
        }
    }

    /// Update the pinned status of a session
    pub fn update_pinned(&mut self, session_id: &str, pinned: bool) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.pinned = pinned;
        }
    }
}
