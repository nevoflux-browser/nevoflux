// crates/protocol/src/envelope.rs

//! Envelope types for IPC message wrapping.
//!
//! These envelopes wrap all messages exchanged between Proxy/MCP bridges and Daemon.

use crate::Channel;
use serde::{Deserialize, Serialize};

/// Authentication information (reserved for enterprise features)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthInfo {
    /// Authentication token
    pub token: String,
    /// User ID
    pub user_id: Option<String>,
    /// Tenant ID for multi-tenant deployments
    pub tenant_id: Option<String>,
}

/// Envelope for messages from Proxy/MCP to Daemon
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyEnvelope {
    /// Router identifier for the proxy
    pub proxy_id: String,
    /// Unique request ID
    pub request_id: String,
    /// Authentication info (optional, for enterprise)
    pub auth: Option<AuthInfo>,
    /// Message channel
    pub channel: Channel,
    /// Raw message payload
    pub payload: serde_json::Value,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Envelope for messages from Daemon to Proxy/MCP
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonEnvelope {
    /// Target proxy ID for routing
    pub proxy_id: String,
    /// Associated request ID (None for broadcasts)
    pub request_id: Option<String>,
    /// Message channel
    pub channel: Channel,
    /// Raw message payload
    pub payload: serde_json::Value,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

impl ProxyEnvelope {
    /// Create a new ProxyEnvelope with current timestamp
    pub fn new(
        proxy_id: impl Into<String>,
        request_id: impl Into<String>,
        channel: Channel,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            proxy_id: proxy_id.into(),
            request_id: request_id.into(),
            auth: None,
            channel,
            payload,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Set authentication info
    pub fn with_auth(mut self, auth: AuthInfo) -> Self {
        self.auth = Some(auth);
        self
    }
}

impl DaemonEnvelope {
    /// Create a new DaemonEnvelope with current timestamp
    pub fn new(proxy_id: impl Into<String>, channel: Channel, payload: serde_json::Value) -> Self {
        Self {
            proxy_id: proxy_id.into(),
            request_id: None,
            channel,
            payload,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Set the associated request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Create a broadcast envelope (no specific proxy target)
    pub fn broadcast(channel: Channel, payload: serde_json::Value) -> Self {
        Self {
            proxy_id: "*".into(),
            request_id: None,
            channel,
            payload,
            timestamp_ms: current_timestamp_ms(),
        }
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_envelope_json_roundtrip() {
        let envelope = ProxyEnvelope {
            proxy_id: "proxy-001".into(),
            request_id: "req-001".into(),
            auth: None,
            channel: Channel::Chat,
            payload: serde_json::json!({"type": "chat_message", "payload": {}}),
            timestamp_ms: 1706000000000,
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: ProxyEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_proxy_envelope_messagepack_roundtrip() {
        let envelope = ProxyEnvelope {
            proxy_id: "proxy-001".into(),
            request_id: "req-001".into(),
            auth: Some(AuthInfo {
                token: "secret-token".into(),
                user_id: Some("user-001".into()),
                tenant_id: None,
            }),
            channel: Channel::Mcp,
            payload: serde_json::json!({"method": "browser_use/click"}),
            timestamp_ms: 1706000000000,
        };

        let encoded = rmp_serde::to_vec(&envelope).unwrap();
        let decoded: ProxyEnvelope = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_daemon_envelope_json_roundtrip() {
        let envelope = DaemonEnvelope {
            proxy_id: "proxy-001".into(),
            request_id: Some("req-001".into()),
            channel: Channel::Chat,
            payload: serde_json::json!({"type": "stream_chunk", "payload": {}}),
            timestamp_ms: 1706000000000,
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: DaemonEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_daemon_envelope_messagepack_roundtrip() {
        let envelope = DaemonEnvelope {
            proxy_id: "proxy-001".into(),
            request_id: None,
            channel: Channel::Chat,
            payload: serde_json::json!({"type": "agent_state"}),
            timestamp_ms: 1706000000000,
        };

        let encoded = rmp_serde::to_vec(&envelope).unwrap();
        let decoded: DaemonEnvelope = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_auth_info_serialization() {
        let auth = AuthInfo {
            token: "token123".into(),
            user_id: Some("user-001".into()),
            tenant_id: Some("tenant-001".into()),
        };

        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"token\":\"token123\""));
        assert!(json.contains("\"user_id\":\"user-001\""));
        assert!(json.contains("\"tenant_id\":\"tenant-001\""));
    }

    #[test]
    fn test_proxy_envelope_builder() {
        let envelope =
            ProxyEnvelope::new("proxy-001", "req-001", Channel::Chat, serde_json::json!({}));
        assert_eq!(envelope.proxy_id, "proxy-001");
        assert!(envelope.auth.is_none());
        assert!(envelope.timestamp_ms > 0);
    }

    #[test]
    fn test_daemon_envelope_builder() {
        let envelope = DaemonEnvelope::new("proxy-001", Channel::Chat, serde_json::json!({}))
            .with_request_id("req-001");
        assert_eq!(envelope.request_id, Some("req-001".into()));
    }

    #[test]
    fn test_daemon_envelope_broadcast() {
        let envelope = DaemonEnvelope::broadcast(Channel::Chat, serde_json::json!({}));
        assert_eq!(envelope.proxy_id, "*");
        assert!(envelope.request_id.is_none());
    }
}
