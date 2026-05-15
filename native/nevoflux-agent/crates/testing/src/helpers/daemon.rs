//! Test daemon builder for integration testing.
//!
//! Provides a builder pattern for creating test daemon instances with
//! pre-configured mocks and automatic cleanup.

use crate::mocks::{MockBrowserActions, MockLlmProvider, MockMcpClient, MockPermissionChecker};
use nevoflux_daemon::{AgentConfig, ProxyRegistry, RequestRegistry, Router, SessionManager};
use std::sync::Arc;

/// A test daemon configuration.
#[derive(Debug, Clone)]
pub struct TestDaemonConfig {
    /// Whether to use in-memory storage.
    pub in_memory: bool,
    /// Custom agent configuration.
    pub agent_config: Option<AgentConfig>,
}

impl Default for TestDaemonConfig {
    fn default() -> Self {
        Self {
            in_memory: true,
            agent_config: None,
        }
    }
}

/// Builder for creating test daemon instances.
///
/// # Example
///
/// ```rust,ignore
/// use nevoflux_testing::helpers::TestDaemonBuilder;
///
/// let daemon = TestDaemonBuilder::new()
///     .with_mock_llm("Hello from mock!")
///     .with_permission_checker(MockPermissionChecker::allow_all())
///     .build()
///     .await?;
///
/// // Use the daemon...
/// assert!(daemon.router().proxy_registry().count() == 0);
/// ```
pub struct TestDaemonBuilder {
    config: TestDaemonConfig,
    mock_llm: Option<Arc<MockLlmProvider>>,
    mock_mcp: Option<Arc<MockMcpClient>>,
    mock_permissions: Option<MockPermissionChecker>,
    mock_browser: Option<Arc<MockBrowserActions>>,
}

impl Default for TestDaemonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDaemonBuilder {
    /// Create a new test daemon builder with default settings.
    pub fn new() -> Self {
        Self {
            config: TestDaemonConfig::default(),
            mock_llm: None,
            mock_mcp: None,
            mock_permissions: None,
            mock_browser: None,
        }
    }

    /// Use a custom agent configuration.
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config.agent_config = Some(config);
        self
    }

    /// Use persistent storage at the given path.
    pub fn with_persistent_storage(mut self, _path: impl Into<String>) -> Self {
        self.config.in_memory = false;
        self
    }

    /// Configure a mock LLM provider with a fixed response.
    pub fn with_mock_llm(mut self, response: impl Into<String>) -> Self {
        self.mock_llm = Some(Arc::new(MockLlmProvider::with_fixed_response(response)));
        self
    }

    /// Configure a mock LLM provider with custom configuration.
    pub fn with_mock_llm_provider(mut self, provider: MockLlmProvider) -> Self {
        self.mock_llm = Some(Arc::new(provider));
        self
    }

    /// Configure a mock MCP client.
    pub fn with_mock_mcp(mut self, client: MockMcpClient) -> Self {
        self.mock_mcp = Some(Arc::new(client));
        self
    }

    /// Configure a permission checker.
    pub fn with_permission_checker(mut self, checker: MockPermissionChecker) -> Self {
        self.mock_permissions = Some(checker);
        self
    }

    /// Allow all permissions (convenience method).
    pub fn allow_all_permissions(mut self) -> Self {
        self.mock_permissions = Some(MockPermissionChecker::allow_all());
        self
    }

    /// Deny all permissions (convenience method).
    pub fn deny_all_permissions(mut self) -> Self {
        self.mock_permissions = Some(MockPermissionChecker::deny_all());
        self
    }

    /// Configure mock browser actions.
    pub fn with_mock_browser(mut self, browser: MockBrowserActions) -> Self {
        self.mock_browser = Some(Arc::new(browser));
        self
    }

    /// Build the test daemon.
    pub async fn build(self) -> Result<TestDaemon, TestDaemonError> {
        // Create session manager
        let session_manager = if self.config.in_memory {
            SessionManager::in_memory().map_err(|e| TestDaemonError::Setup(e.to_string()))?
        } else {
            return Err(TestDaemonError::Setup(
                "Persistent storage not yet supported in TestDaemonBuilder".to_string(),
            ));
        };

        // Create router
        let router = Router::new();

        // Get or create default mocks
        let mock_llm = self
            .mock_llm
            .unwrap_or_else(|| Arc::new(MockLlmProvider::new()));
        let mock_mcp = self
            .mock_mcp
            .unwrap_or_else(|| Arc::new(MockMcpClient::new()));
        let mock_permissions = self
            .mock_permissions
            .unwrap_or_else(MockPermissionChecker::allow_all);
        let mock_browser = self
            .mock_browser
            .unwrap_or_else(|| Arc::new(MockBrowserActions::new()));

        // Get or create default config
        let config = self.config.agent_config.unwrap_or_default();

        Ok(TestDaemon {
            session_manager,
            router,
            config,
            mock_llm,
            mock_mcp,
            mock_permissions,
            mock_browser,
        })
    }
}

/// A test daemon instance with pre-configured mocks.
pub struct TestDaemon {
    session_manager: SessionManager,
    router: Router,
    config: AgentConfig,
    mock_llm: Arc<MockLlmProvider>,
    mock_mcp: Arc<MockMcpClient>,
    mock_permissions: MockPermissionChecker,
    mock_browser: Arc<MockBrowserActions>,
}

impl TestDaemon {
    /// Get the session manager.
    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    /// Get the router.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Get the proxy registry.
    pub fn proxy_registry(&self) -> &ProxyRegistry {
        self.router.proxy_registry()
    }

    /// Get the request registry.
    pub fn request_registry(&self) -> &RequestRegistry {
        self.router.request_registry()
    }

    /// Get the agent configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the mock LLM provider.
    pub fn mock_llm(&self) -> &MockLlmProvider {
        &self.mock_llm
    }

    /// Get the mock MCP client.
    pub fn mock_mcp(&self) -> &MockMcpClient {
        &self.mock_mcp
    }

    /// Get the mock permission checker.
    pub fn mock_permissions(&self) -> &MockPermissionChecker {
        &self.mock_permissions
    }

    /// Get the mock browser actions.
    pub fn mock_browser(&self) -> &MockBrowserActions {
        &self.mock_browser
    }

    /// Create a new session for testing.
    pub async fn create_test_session(&self) -> Result<String, TestDaemonError> {
        let session = self
            .session_manager
            .create_session(None, None)
            .await
            .map_err(|e| TestDaemonError::Session(e.to_string()))?;
        Ok(session.id)
    }

    /// Register a test proxy.
    pub fn register_proxy(&self, proxy_id: &str, pid: u32) {
        self.router.proxy_registry().register(proxy_id, pid);
    }

    /// Register a test request.
    pub fn register_request(&self, request_id: &str, proxy_id: &str, session_id: &str) {
        self.router
            .request_registry()
            .register(request_id, proxy_id, session_id);
    }

    /// Get the number of registered proxies.
    pub fn proxy_count(&self) -> usize {
        self.router.proxy_registry().active_count()
    }

    /// Get the number of active requests.
    pub fn request_count(&self) -> usize {
        self.router.request_registry().active_count()
    }

    /// Check if a proxy is registered.
    pub fn is_proxy_registered(&self, proxy_id: &str) -> bool {
        self.router.proxy_registry().is_registered(proxy_id)
    }

    /// Check permission for an action.
    pub fn check_permission(&self, resource_type: &str, action: &str, resource: &str) -> bool {
        self.mock_permissions.check(resource_type, action, resource)
    }
}

/// Errors that can occur when building or using a test daemon.
#[derive(Debug, thiserror::Error)]
pub enum TestDaemonError {
    /// Setup error.
    #[error("Setup error: {0}")]
    Setup(String),
    /// Session error.
    #[error("Session error: {0}")]
    Session(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_default() {
        let daemon = TestDaemonBuilder::new().build().await.unwrap();

        assert_eq!(daemon.proxy_count(), 0);
        assert_eq!(daemon.request_count(), 0);
    }

    #[tokio::test]
    async fn test_builder_with_mock_llm() {
        let daemon = TestDaemonBuilder::new()
            .with_mock_llm("Test response")
            .build()
            .await
            .unwrap();

        assert!(daemon.mock_llm().is_configured());
        assert_eq!(
            daemon.mock_llm().response_text(),
            Some("Test response".to_string())
        );
    }

    #[tokio::test]
    async fn test_builder_allow_all_permissions() {
        let daemon = TestDaemonBuilder::new()
            .allow_all_permissions()
            .build()
            .await
            .unwrap();

        assert!(daemon.check_permission("file", "read", "/any/path"));
        assert!(daemon.check_permission("network", "connect", "http://example.com"));
    }

    #[tokio::test]
    async fn test_builder_deny_all_permissions() {
        let daemon = TestDaemonBuilder::new()
            .deny_all_permissions()
            .build()
            .await
            .unwrap();

        assert!(!daemon.check_permission("file", "read", "/any/path"));
        assert!(!daemon.check_permission("network", "connect", "http://example.com"));
    }

    #[tokio::test]
    async fn test_daemon_create_session() {
        let daemon = TestDaemonBuilder::new().build().await.unwrap();

        let session_id = daemon.create_test_session().await.unwrap();
        assert!(!session_id.is_empty());
    }

    #[tokio::test]
    async fn test_daemon_register_proxy() {
        let daemon = TestDaemonBuilder::new().build().await.unwrap();

        daemon.register_proxy("test-proxy", 12345);

        assert_eq!(daemon.proxy_count(), 1);
        assert!(daemon.is_proxy_registered("test-proxy"));
        assert!(!daemon.is_proxy_registered("other-proxy"));
    }

    #[tokio::test]
    async fn test_daemon_register_request() {
        let daemon = TestDaemonBuilder::new().build().await.unwrap();

        daemon.register_proxy("proxy-1", 12345);
        daemon.register_request("req-1", "proxy-1", "session-1");

        assert_eq!(daemon.request_count(), 1);
    }

    #[tokio::test]
    async fn test_builder_with_custom_config() {
        let mut config = AgentConfig::default();
        config.daemon.port_range_start = 20000;

        let daemon = TestDaemonBuilder::new()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert_eq!(daemon.config().daemon.port_range_start, 20000);
    }

    #[tokio::test]
    async fn test_builder_with_mock_mcp() {
        let mcp = MockMcpClient::new().with_connected(true);

        let daemon = TestDaemonBuilder::new()
            .with_mock_mcp(mcp)
            .build()
            .await
            .unwrap();

        assert!(daemon.mock_mcp().is_connected());
    }

    #[tokio::test]
    async fn test_builder_with_mock_browser() {
        let browser = MockBrowserActions::new();

        let daemon = TestDaemonBuilder::new()
            .with_mock_browser(browser)
            .build()
            .await
            .unwrap();

        // Browser mock should be accessible
        let _ = daemon.mock_browser();
    }

    #[tokio::test]
    async fn test_builder_chaining() {
        let daemon = TestDaemonBuilder::new()
            .with_mock_llm("Response 1")
            .allow_all_permissions()
            .with_mock_mcp(MockMcpClient::new().with_connected(true))
            .build()
            .await
            .unwrap();

        assert!(daemon.mock_llm().is_configured());
        assert!(daemon.mock_mcp().is_connected());
        assert!(daemon.check_permission("any", "action", "resource"));
    }
}
