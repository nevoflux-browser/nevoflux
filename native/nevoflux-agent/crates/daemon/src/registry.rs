//! Registry for tracking proxies and active requests.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

/// Information about a connected proxy.
#[derive(Debug, Clone)]
pub struct ProxyInfo {
    /// Unique proxy identifier.
    pub proxy_id: String,
    /// Process ID of the proxy.
    pub pid: u32,
    /// Time of last heartbeat.
    pub last_heartbeat: Instant,
    /// Time when proxy registered.
    pub registered_at: Instant,
}

impl ProxyInfo {
    /// Create a new proxy info.
    pub fn new(proxy_id: impl Into<String>, pid: u32) -> Self {
        let now = Instant::now();
        Self {
            proxy_id: proxy_id.into(),
            pid,
            last_heartbeat: now,
            registered_at: now,
        }
    }

    /// Update the heartbeat timestamp.
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// Check if the proxy has timed out.
    pub fn is_timed_out(&self, timeout: std::time::Duration) -> bool {
        self.last_heartbeat.elapsed() > timeout
    }
}

/// Registry for tracking connected proxies.
pub struct ProxyRegistry {
    proxies: RwLock<HashMap<String, ProxyInfo>>,
}

impl Default for ProxyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyRegistry {
    /// Create a new proxy registry.
    pub fn new() -> Self {
        Self {
            proxies: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new proxy.
    pub fn register(&self, proxy_id: impl Into<String>, pid: u32) {
        let proxy_id = proxy_id.into();
        let info = ProxyInfo::new(&proxy_id, pid);
        self.proxies.write().unwrap().insert(proxy_id, info);
    }

    /// Unregister a proxy.
    pub fn unregister(&self, proxy_id: &str) -> Option<ProxyInfo> {
        self.proxies.write().unwrap().remove(proxy_id)
    }

    /// Check if a proxy is registered.
    pub fn is_registered(&self, proxy_id: &str) -> bool {
        self.proxies.read().unwrap().contains_key(proxy_id)
    }

    /// Get proxy info.
    pub fn get(&self, proxy_id: &str) -> Option<ProxyInfo> {
        self.proxies.read().unwrap().get(proxy_id).cloned()
    }

    /// Update heartbeat for a proxy.
    pub fn heartbeat(&self, proxy_id: &str) -> bool {
        if let Some(info) = self.proxies.write().unwrap().get_mut(proxy_id) {
            info.update_heartbeat();
            true
        } else {
            false
        }
    }

    /// Get all registered proxy IDs.
    pub fn all_proxy_ids(&self) -> Vec<String> {
        self.proxies.read().unwrap().keys().cloned().collect()
    }

    /// Get count of active proxies.
    pub fn active_count(&self) -> usize {
        self.proxies.read().unwrap().len()
    }

    /// Remove timed out proxies.
    pub fn remove_timed_out(&self, timeout: std::time::Duration) -> Vec<String> {
        let mut removed = Vec::new();
        let mut proxies = self.proxies.write().unwrap();

        proxies.retain(|id, info| {
            if info.is_timed_out(timeout) {
                removed.push(id.clone());
                false
            } else {
                true
            }
        });

        removed
    }
}

/// Tracks the most-recently-active sidebar proxy per session_id, so that
/// `/loop` iterations can borrow a connected sidebar to fulfill `browser_*`
/// tool calls. Iterations themselves have `proxy_id=""` (no inbound chat
/// connection), and without this tracker their browser requests get dropped
/// at the `No writer for proxy ""` check in `server.rs::browser request handler`.
///
/// Updated by `server.rs` on every Chat-channel message arrival; read by
/// `IterationExecutor` at iteration start.
pub struct SessionProxyTracker {
    map: RwLock<HashMap<String, SessionProxyEntry>>,
}

#[derive(Clone, Debug)]
pub struct SessionProxyEntry {
    pub proxy_id: String,
    pub client_identity: Vec<u8>,
    pub last_seen: Instant,
}

impl Default for SessionProxyTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionProxyTracker {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }

    /// Record that `session_id` was most recently seen on `proxy_id`.
    /// Empty proxy_id is ignored (iteration-internal messages can't borrow
    /// from themselves).
    pub fn note(&self, session_id: &str, proxy_id: &str, client_identity: &[u8]) {
        if session_id.is_empty() || proxy_id.is_empty() {
            return;
        }
        let entry = SessionProxyEntry {
            proxy_id: proxy_id.to_string(),
            client_identity: client_identity.to_vec(),
            last_seen: Instant::now(),
        };
        self.map
            .write()
            .unwrap()
            .insert(session_id.to_string(), entry);
    }

    /// Return the latest sidebar proxy info for `session_id`, if any.
    pub fn latest(&self, session_id: &str) -> Option<SessionProxyEntry> {
        self.map.read().unwrap().get(session_id).cloned()
    }
}

/// Information about an active request.
#[derive(Debug, Clone)]
pub struct ActiveRequest {
    /// Request ID.
    pub request_id: String,
    /// Proxy ID that initiated the request.
    pub proxy_id: String,
    /// Session ID for the request.
    pub session_id: String,
    /// Time when request started.
    pub started_at: Instant,
}

impl ActiveRequest {
    /// Create a new active request.
    pub fn new(
        request_id: impl Into<String>,
        proxy_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            proxy_id: proxy_id.into(),
            session_id: session_id.into(),
            started_at: Instant::now(),
        }
    }

    /// Get elapsed time since request started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

/// Registry for tracking active requests.
pub struct RequestRegistry {
    /// Active requests by request ID.
    requests: RwLock<HashMap<String, ActiveRequest>>,
    /// Request IDs by session ID (for checking if session is busy).
    sessions: RwLock<HashMap<String, String>>,
}

impl Default for RequestRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestRegistry {
    /// Create a new request registry.
    pub fn new() -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new active request.
    ///
    /// Returns false if the session already has an active request.
    pub fn register(
        &self,
        request_id: impl Into<String>,
        proxy_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> bool {
        let request_id = request_id.into();
        let proxy_id = proxy_id.into();
        let session_id = session_id.into();

        // Check if session is busy
        {
            let sessions = self.sessions.read().unwrap();
            if sessions.contains_key(&session_id) {
                return false;
            }
        }

        let request = ActiveRequest::new(&request_id, proxy_id, &session_id);

        self.requests
            .write()
            .unwrap()
            .insert(request_id.clone(), request);
        self.sessions
            .write()
            .unwrap()
            .insert(session_id, request_id);

        true
    }

    /// Complete and remove an active request.
    pub fn complete(&self, request_id: &str) -> Option<ActiveRequest> {
        let request = self.requests.write().unwrap().remove(request_id)?;
        self.sessions.write().unwrap().remove(&request.session_id);
        Some(request)
    }

    /// Get an active request.
    pub fn get(&self, request_id: &str) -> Option<ActiveRequest> {
        self.requests.read().unwrap().get(request_id).cloned()
    }

    /// Check if a session has an active request.
    pub fn is_session_busy(&self, session_id: &str) -> bool {
        self.sessions.read().unwrap().contains_key(session_id)
    }

    /// Get the active request ID for a session.
    pub fn get_request_for_session(&self, session_id: &str) -> Option<String> {
        self.sessions.read().unwrap().get(session_id).cloned()
    }

    /// Get count of active requests.
    pub fn active_count(&self) -> usize {
        self.requests.read().unwrap().len()
    }

    /// Remove all requests for a proxy (when proxy disconnects).
    pub fn remove_for_proxy(&self, proxy_id: &str) -> Vec<ActiveRequest> {
        let mut removed = Vec::new();
        let mut requests = self.requests.write().unwrap();
        let mut sessions = self.sessions.write().unwrap();

        let to_remove: Vec<String> = requests
            .iter()
            .filter(|(_, req)| req.proxy_id == proxy_id)
            .map(|(id, _)| id.clone())
            .collect();

        for request_id in to_remove {
            if let Some(request) = requests.remove(&request_id) {
                sessions.remove(&request.session_id);
                removed.push(request);
            }
        }

        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_proxy_info_new() {
        let info = ProxyInfo::new("proxy-001", 12345);

        assert_eq!(info.proxy_id, "proxy-001");
        assert_eq!(info.pid, 12345);
    }

    #[test]
    fn test_proxy_info_heartbeat() {
        let mut info = ProxyInfo::new("proxy-001", 12345);
        let old_heartbeat = info.last_heartbeat;

        std::thread::sleep(Duration::from_millis(10));
        info.update_heartbeat();

        assert!(info.last_heartbeat > old_heartbeat);
    }

    #[test]
    fn test_proxy_info_timeout() {
        let info = ProxyInfo::new("proxy-001", 12345);

        // Should not be timed out immediately
        assert!(!info.is_timed_out(Duration::from_secs(30)));

        // Should be timed out with zero timeout
        assert!(info.is_timed_out(Duration::ZERO));
    }

    #[test]
    fn test_proxy_registry_register() {
        let registry = ProxyRegistry::new();

        registry.register("proxy-001", 12345);

        assert!(registry.is_registered("proxy-001"));
        assert!(!registry.is_registered("proxy-002"));
        assert_eq!(registry.active_count(), 1);
    }

    #[test]
    fn test_proxy_registry_unregister() {
        let registry = ProxyRegistry::new();

        registry.register("proxy-001", 12345);
        let info = registry.unregister("proxy-001");

        assert!(info.is_some());
        assert!(!registry.is_registered("proxy-001"));
    }

    #[test]
    fn test_proxy_registry_heartbeat() {
        let registry = ProxyRegistry::new();

        registry.register("proxy-001", 12345);

        assert!(registry.heartbeat("proxy-001"));
        assert!(!registry.heartbeat("proxy-002"));
    }

    #[test]
    fn test_proxy_registry_all_proxy_ids() {
        let registry = ProxyRegistry::new();

        registry.register("proxy-001", 1);
        registry.register("proxy-002", 2);
        registry.register("proxy-003", 3);

        let ids = registry.all_proxy_ids();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_active_request_new() {
        let request = ActiveRequest::new("req-001", "proxy-001", "session-001");

        assert_eq!(request.request_id, "req-001");
        assert_eq!(request.proxy_id, "proxy-001");
        assert_eq!(request.session_id, "session-001");
    }

    #[test]
    fn test_request_registry_register() {
        let registry = RequestRegistry::new();

        assert!(registry.register("req-001", "proxy-001", "session-001"));

        let request = registry.get("req-001");
        assert!(request.is_some());
        assert_eq!(request.unwrap().proxy_id, "proxy-001");
    }

    #[test]
    fn test_request_registry_session_busy() {
        let registry = RequestRegistry::new();

        // First request should succeed
        assert!(registry.register("req-001", "proxy-001", "session-001"));

        // Second request for same session should fail
        assert!(!registry.register("req-002", "proxy-001", "session-001"));

        assert!(registry.is_session_busy("session-001"));
    }

    #[test]
    fn test_request_registry_complete() {
        let registry = RequestRegistry::new();

        registry.register("req-001", "proxy-001", "session-001");

        let completed = registry.complete("req-001");
        assert!(completed.is_some());

        // Session should no longer be busy
        assert!(!registry.is_session_busy("session-001"));

        // Request should no longer exist
        assert!(registry.get("req-001").is_none());
    }

    #[test]
    fn test_request_registry_remove_for_proxy() {
        let registry = RequestRegistry::new();

        registry.register("req-001", "proxy-001", "session-001");
        registry.register("req-002", "proxy-001", "session-002");
        registry.register("req-003", "proxy-002", "session-003");

        let removed = registry.remove_for_proxy("proxy-001");

        assert_eq!(removed.len(), 2);
        assert_eq!(registry.active_count(), 1);
    }
}
