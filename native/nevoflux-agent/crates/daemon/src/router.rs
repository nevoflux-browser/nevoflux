//! Message routing for the daemon.

use crate::registry::{ProxyRegistry, RequestRegistry};
use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use std::sync::Arc;

/// Message router for the daemon.
///
/// Routes messages between proxies and the daemon core based on:
/// - Request IDs (for responses to specific requests)
/// - Proxy IDs (for direct messages)
/// - Broadcast (for notifications to all proxies)
pub struct Router {
    /// Registry of connected proxies.
    proxy_registry: Arc<ProxyRegistry>,
    /// Registry of active requests.
    request_registry: Arc<RequestRegistry>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    /// Create a new router.
    pub fn new() -> Self {
        Self {
            proxy_registry: Arc::new(ProxyRegistry::new()),
            request_registry: Arc::new(RequestRegistry::new()),
        }
    }

    /// Create a router with existing registries.
    pub fn with_registries(
        proxy_registry: Arc<ProxyRegistry>,
        request_registry: Arc<RequestRegistry>,
    ) -> Self {
        Self {
            proxy_registry,
            request_registry,
        }
    }

    /// Get the proxy registry.
    pub fn proxy_registry(&self) -> &ProxyRegistry {
        &self.proxy_registry
    }

    /// Get the request registry.
    pub fn request_registry(&self) -> &RequestRegistry {
        &self.request_registry
    }

    /// Find the proxy ID for a request (for routing responses).
    pub fn find_proxy_for_request(&self, request_id: &str) -> Option<String> {
        self.request_registry
            .get(request_id)
            .map(|req| req.proxy_id)
    }

    /// Find the proxy ID for a session's active request.
    pub fn find_proxy_for_session(&self, session_id: &str) -> Option<String> {
        let request_id = self.request_registry.get_request_for_session(session_id)?;
        self.find_proxy_for_request(&request_id)
    }

    /// Get all proxy IDs for broadcast.
    pub fn all_proxy_ids(&self) -> Vec<String> {
        self.proxy_registry.all_proxy_ids()
    }

    /// Route an incoming message from a proxy.
    ///
    /// Returns the routing decision.
    pub fn route_incoming(&self, envelope: &ProxyEnvelope) -> RouteDecision {
        // Check if proxy is registered
        if !self.proxy_registry.is_registered(&envelope.proxy_id) {
            return RouteDecision::RejectUnregistered;
        }

        match envelope.channel {
            Channel::Chat => RouteDecision::ProcessChat {
                proxy_id: envelope.proxy_id.clone(),
                request_id: envelope.request_id.clone(),
            },
            Channel::Mcp => RouteDecision::ProcessMcp {
                proxy_id: envelope.proxy_id.clone(),
                request_id: envelope.request_id.clone(),
            },
        }
    }

    /// Create a response envelope for a specific proxy.
    pub fn create_response(
        &self,
        proxy_id: impl Into<String>,
        request_id: Option<String>,
        channel: Channel,
        payload: serde_json::Value,
    ) -> DaemonEnvelope {
        DaemonEnvelope {
            proxy_id: proxy_id.into(),
            request_id,
            channel,
            payload,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Create a broadcast envelope for all proxies.
    pub fn create_broadcast(&self, channel: Channel, payload: serde_json::Value) -> DaemonEnvelope {
        DaemonEnvelope::broadcast(channel, payload)
    }

    /// Register a new request.
    ///
    /// Returns false if the session is already busy.
    pub fn register_request(
        &self,
        request_id: impl Into<String>,
        proxy_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> bool {
        self.request_registry
            .register(request_id, proxy_id, session_id)
    }

    /// Complete a request.
    pub fn complete_request(&self, request_id: &str) {
        self.request_registry.complete(request_id);
    }

    /// Handle proxy disconnection.
    ///
    /// Cleans up the proxy and its active requests.
    pub fn handle_proxy_disconnect(&self, proxy_id: &str) {
        self.proxy_registry.unregister(proxy_id);
        self.request_registry.remove_for_proxy(proxy_id);
    }
}

/// Decision for routing an incoming message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// Process as a chat message.
    ProcessChat {
        /// The proxy that sent the message.
        proxy_id: String,
        /// The request ID.
        request_id: String,
    },
    /// Process as an MCP message.
    ProcessMcp {
        /// The proxy that sent the message.
        proxy_id: String,
        /// The request ID.
        request_id: String,
    },
    /// Reject because proxy is not registered.
    RejectUnregistered,
}

/// Get current timestamp in milliseconds.
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
    fn test_router_new() {
        let router = Router::new();

        assert_eq!(router.proxy_registry().active_count(), 0);
        assert_eq!(router.request_registry().active_count(), 0);
    }

    #[test]
    fn test_router_find_proxy_for_request() {
        let router = Router::new();

        router.proxy_registry().register("proxy-001", 12345);
        router.register_request("req-001", "proxy-001", "session-001");

        let proxy_id = router.find_proxy_for_request("req-001");
        assert_eq!(proxy_id, Some("proxy-001".to_string()));

        let missing = router.find_proxy_for_request("req-999");
        assert!(missing.is_none());
    }

    #[test]
    fn test_router_find_proxy_for_session() {
        let router = Router::new();

        router.proxy_registry().register("proxy-001", 12345);
        router.register_request("req-001", "proxy-001", "session-001");

        let proxy_id = router.find_proxy_for_session("session-001");
        assert_eq!(proxy_id, Some("proxy-001".to_string()));
    }

    #[test]
    fn test_router_all_proxy_ids() {
        let router = Router::new();

        router.proxy_registry().register("proxy-001", 1);
        router.proxy_registry().register("proxy-002", 2);

        let ids = router.all_proxy_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_router_route_incoming_unregistered() {
        let router = Router::new();

        let envelope = ProxyEnvelope::new(
            "proxy-unknown",
            "req-001",
            Channel::Chat,
            serde_json::json!({}),
        );

        let decision = router.route_incoming(&envelope);
        assert_eq!(decision, RouteDecision::RejectUnregistered);
    }

    #[test]
    fn test_router_route_incoming_chat() {
        let router = Router::new();
        router.proxy_registry().register("proxy-001", 12345);

        let envelope =
            ProxyEnvelope::new("proxy-001", "req-001", Channel::Chat, serde_json::json!({}));

        let decision = router.route_incoming(&envelope);
        assert!(matches!(decision, RouteDecision::ProcessChat { .. }));
    }

    #[test]
    fn test_router_route_incoming_mcp() {
        let router = Router::new();
        router.proxy_registry().register("proxy-001", 12345);

        let envelope =
            ProxyEnvelope::new("proxy-001", "req-001", Channel::Mcp, serde_json::json!({}));

        let decision = router.route_incoming(&envelope);
        assert!(matches!(decision, RouteDecision::ProcessMcp { .. }));
    }

    #[test]
    fn test_router_create_response() {
        let router = Router::new();

        let response = router.create_response(
            "proxy-001",
            Some("req-001".to_string()),
            Channel::Chat,
            serde_json::json!({"type": "stream_chunk"}),
        );

        assert_eq!(response.proxy_id, "proxy-001");
        assert_eq!(response.request_id, Some("req-001".to_string()));
        assert_eq!(response.channel, Channel::Chat);
    }

    #[test]
    fn test_router_create_broadcast() {
        let router = Router::new();

        let broadcast =
            router.create_broadcast(Channel::Chat, serde_json::json!({"type": "shutdown"}));

        assert_eq!(broadcast.proxy_id, "*");
        assert!(broadcast.request_id.is_none());
    }

    #[test]
    fn test_router_register_request_session_busy() {
        let router = Router::new();

        // First request should succeed
        assert!(router.register_request("req-001", "proxy-001", "session-001"));

        // Second request for same session should fail
        assert!(!router.register_request("req-002", "proxy-001", "session-001"));
    }

    #[test]
    fn test_router_complete_request() {
        let router = Router::new();

        router.register_request("req-001", "proxy-001", "session-001");
        router.complete_request("req-001");

        // Now session should not be busy
        assert!(router.register_request("req-002", "proxy-001", "session-001"));
    }

    #[test]
    fn test_router_handle_proxy_disconnect() {
        let router = Router::new();

        router.proxy_registry().register("proxy-001", 12345);
        router.register_request("req-001", "proxy-001", "session-001");

        router.handle_proxy_disconnect("proxy-001");

        assert!(!router.proxy_registry().is_registered("proxy-001"));
        assert!(router.find_proxy_for_request("req-001").is_none());
    }
}
