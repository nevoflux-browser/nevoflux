/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Session Management
//!
//! Manages browser sessions, each bound to a tab or window

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Session state
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID (typically corresponds to a browser tab)
    pub session_id: String,

    /// Associated tab ID (if any)
    pub tab_id: Option<u32>,

    /// Current URL
    pub url: Option<String>,

    /// Page title
    pub title: Option<String>,

    /// Session-specific context data
    pub context: HashMap<String, serde_json::Value>,

    /// Creation timestamp
    pub created_at: std::time::SystemTime,

    /// Last activity timestamp
    pub last_activity: std::time::SystemTime,
}

impl Session {
    /// Create a new session
    pub fn new(session_id: String) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            session_id,
            tab_id: None,
            url: None,
            title: None,
            context: HashMap::new(),
            created_at: now,
            last_activity: now,
        }
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = std::time::SystemTime::now();
    }

    /// Update tab information
    pub fn update_tab(&mut self, tab_id: u32, url: String, title: String) {
        self.tab_id = Some(tab_id);
        self.url = Some(url);
        self.title = Some(title);
        self.touch();
    }

    /// Set context value
    pub fn set_context(&mut self, key: String, value: serde_json::Value) {
        self.context.insert(key, value);
    }

    /// Get context value
    pub fn get_context(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(key)
    }
}

/// Session manager for tracking active sessions
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a session
    pub async fn get_or_create(&self, session_id: String) -> Session {
        let mut sessions = self.sessions.write().await;

        sessions
            .entry(session_id.clone())
            .or_insert_with(|| {
                info!("Creating new session: {}", session_id);
                Session::new(session_id)
            })
            .clone()
    }

    /// Update session
    pub async fn update<F>(&self, session_id: &str, update_fn: F)
    where
        F: FnOnce(&mut Session),
    {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            update_fn(session);
        }
    }

    /// Get session (read-only)
    pub async fn get(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Remove session
    pub async fn remove(&self, session_id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write().await;
        let removed = sessions.remove(session_id);

        if removed.is_some() {
            info!("Removed session: {}", session_id);
        }

        removed
    }

    /// Clean up inactive sessions (older than timeout)
    pub async fn cleanup_inactive(&self, timeout: std::time::Duration) {
        let mut sessions = self.sessions.write().await;
        let now = std::time::SystemTime::now();

        sessions.retain(|session_id, session| {
            if let Ok(duration) = now.duration_since(session.last_activity) {
                if duration > timeout {
                    debug!("Removing inactive session: {}", session_id);
                    return false;
                }
            }
            true
        });
    }

    /// Get all active session IDs
    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Get session count
    pub async fn count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let manager = SessionManager::new();
        let session = manager.get_or_create("session-1".to_string()).await;

        assert_eq!(session.session_id, "session-1");
        assert_eq!(manager.count().await, 1);
    }

    #[tokio::test]
    async fn test_session_update() {
        let manager = SessionManager::new();
        manager.get_or_create("session-1".to_string()).await;

        manager
            .update("session-1", |session| {
                session.update_tab(
                    123,
                    "https://example.com".to_string(),
                    "Example".to_string(),
                );
            })
            .await;

        let session = manager.get("session-1").await.unwrap();
        assert_eq!(session.tab_id, Some(123));
        assert_eq!(session.url, Some("https://example.com".to_string()));
    }

    #[tokio::test]
    async fn test_session_removal() {
        let manager = SessionManager::new();
        manager.get_or_create("session-1".to_string()).await;

        assert_eq!(manager.count().await, 1);

        manager.remove("session-1").await;

        assert_eq!(manager.count().await, 0);
    }
}
