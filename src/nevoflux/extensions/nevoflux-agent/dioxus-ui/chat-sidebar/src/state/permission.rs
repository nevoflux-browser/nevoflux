/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Permission request state for Human-in-the-Loop

use shared_protocol::{ResourceAction, ResourceType};

/// Permission request state for UI display
#[derive(Debug, Clone)]
pub struct PermissionRequestState {
    /// Unique request ID
    pub request_id: String,
    /// Type of resource being requested
    pub resource_type: ResourceType,
    /// Action being requested
    pub action: ResourceAction,
    /// Resource path or identifier
    pub resource: String,
    /// Name of the requester
    pub requester: String,
    /// Reason for the request
    pub reason: String,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
    /// Request creation timestamp
    pub created_at: u64,
}

impl PermissionRequestState {
    /// Calculate remaining time in milliseconds
    pub fn remaining_ms(&self) -> u64 {
        let elapsed = js_sys::Date::now() as u64 - self.created_at;
        self.timeout_ms.saturating_sub(elapsed)
    }

    /// Check if the request has expired
    pub fn is_expired(&self) -> bool {
        self.remaining_ms() == 0
    }

    /// Get remaining seconds for display
    pub fn remaining_secs(&self) -> u32 {
        (self.remaining_ms() / 1000) as u32
    }
}

/// Get display icon for resource type
pub fn get_resource_icon(resource_type: &ResourceType) -> &'static str {
    match resource_type {
        ResourceType::File => "📁",
        ResourceType::Script => "⚙️",
        ResourceType::Network => "🌐",
        ResourceType::Mcp => "🔌",
        ResourceType::Plugin => "🧩",
    }
}

/// Format countdown display
pub fn format_countdown(secs: u32) -> String {
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}
