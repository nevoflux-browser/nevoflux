/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Connection state management

/// Connection status with native agent
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Connecting to agent
    Connecting,
    /// Connected and ready
    Connected,
    /// Reconnecting after disconnect
    Reconnecting {
        /// Current attempt number
        attempt: u32,
        /// Time until next retry in ms
        next_retry_ms: u64,
    },
    /// Connection error
    Error {
        /// Error message
        message: String,
    },
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ConnectionState {
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if in a connecting state
    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting { .. })
    }

    /// Get display class for CSS
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting | Self::Reconnecting { .. } => "connecting",
            Self::Disconnected | Self::Error { .. } => "disconnected",
        }
    }

    /// Get display icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Connected => "●",
            _ => "○",
        }
    }
}
