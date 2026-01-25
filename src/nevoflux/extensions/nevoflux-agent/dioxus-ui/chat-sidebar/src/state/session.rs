/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Session state management

/// Session state
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Unique session ID
    pub id: String,
    /// Session creation timestamp
    pub created_at: u64,
    /// Whether the session is active
    pub is_active: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    /// Create a new session
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: js_sys::Date::now() as u64,
            is_active: true,
        }
    }
}
