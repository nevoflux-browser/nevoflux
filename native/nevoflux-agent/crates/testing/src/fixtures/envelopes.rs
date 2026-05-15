//! Envelope builders and fixtures.

use nevoflux_protocol::{AuthInfo, Channel, ProxyEnvelope};

/// Builder for creating test envelopes.
#[derive(Debug, Clone)]
pub struct EnvelopeBuilder {
    proxy_id: String,
    request_id: String,
    auth: Option<AuthInfo>,
    channel: Channel,
    payload: serde_json::Value,
    timestamp_ms: u64,
}

impl Default for EnvelopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvelopeBuilder {
    /// Create a new envelope builder with default values.
    pub fn new() -> Self {
        Self {
            proxy_id: "test-proxy".to_string(),
            request_id: "test-request".to_string(),
            auth: None,
            channel: Channel::Chat,
            payload: serde_json::json!({}),
            timestamp_ms: 1706000000000,
        }
    }

    /// Set the proxy ID.
    pub fn with_proxy_id(mut self, proxy_id: impl Into<String>) -> Self {
        self.proxy_id = proxy_id.into();
        self
    }

    /// Set the request ID.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    /// Set the authentication info.
    pub fn with_auth(mut self, auth: AuthInfo) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Set the channel.
    pub fn with_channel(mut self, channel: Channel) -> Self {
        self.channel = channel;
        self
    }

    /// Set the payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Set the timestamp.
    pub fn with_timestamp_ms(mut self, timestamp_ms: u64) -> Self {
        self.timestamp_ms = timestamp_ms;
        self
    }

    /// Build the ProxyEnvelope.
    pub fn build(self) -> ProxyEnvelope {
        ProxyEnvelope {
            proxy_id: self.proxy_id,
            request_id: self.request_id,
            auth: self.auth,
            channel: self.channel,
            payload: self.payload,
            timestamp_ms: self.timestamp_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_builder_defaults() {
        let envelope = EnvelopeBuilder::new().build();

        assert_eq!(envelope.proxy_id, "test-proxy");
        assert_eq!(envelope.request_id, "test-request");
        assert!(envelope.auth.is_none());
        assert_eq!(envelope.channel, Channel::Chat);
    }

    #[test]
    fn test_envelope_builder_custom_values() {
        let envelope = EnvelopeBuilder::new()
            .with_proxy_id("custom-proxy")
            .with_request_id("custom-request")
            .with_channel(Channel::Mcp)
            .with_payload(serde_json::json!({"key": "value"}))
            .build();

        assert_eq!(envelope.proxy_id, "custom-proxy");
        assert_eq!(envelope.request_id, "custom-request");
        assert_eq!(envelope.channel, Channel::Mcp);
        assert_eq!(envelope.payload["key"], "value");
    }

    #[test]
    fn test_envelope_builder_with_auth() {
        let auth = AuthInfo {
            token: "test-token".into(),
            user_id: Some("user-001".into()),
            tenant_id: None,
        };

        let envelope = EnvelopeBuilder::new().with_auth(auth.clone()).build();

        assert!(envelope.auth.is_some());
        let envelope_auth = envelope.auth.unwrap();
        assert_eq!(envelope_auth.token, "test-token");
        assert_eq!(envelope_auth.user_id, Some("user-001".into()));
    }
}
