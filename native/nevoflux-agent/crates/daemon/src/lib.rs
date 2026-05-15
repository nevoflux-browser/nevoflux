//! NevoFlux Daemon - Core daemon for NevoFlux Agent
//!
//! The daemon is the central processing unit that handles:
//! - Message routing between proxies and the agent core
//! - Session management and persistence
//! - Context building for LLM requests
//! - MCP server management
//! - Permission management
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Daemon Internals                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ProxyRegistry          RequestRegistry      SessionManager  │
//! │  ┌─────────────┐       ┌───────────────┐    ┌───────────┐   │
//! │  │ proxy_id →  │       │ request_id →  │    │ session   │   │
//! │  │ ProxyInfo   │       │ ActiveRequest │    │ (SQLite)  │   │
//! │  └─────────────┘       └───────────────┘    └───────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod agent;
pub mod agent_host;
pub mod asset_server;
pub mod canvas_persist;
pub mod canvas_tools;
pub mod canvas_video;
pub mod config;
pub mod config_watcher;
pub mod context;
pub mod error;
pub mod event_bus;
pub mod file_picker;
pub mod health;
pub mod learning;
pub mod loops;
pub mod mcp_config;
pub mod openclaw_setup;
pub mod permission;
pub mod registry;
pub mod retry;
pub mod router;
pub mod secrets;
pub mod server;
pub mod session;
pub mod share;
pub mod skills;
pub mod trace;
pub mod tts;
pub mod validation;
pub mod wasm;

pub use config::{
    AgentConfig, AuthConfig, ConfigError, ContextConfig, DaemonConfig, LlmConfig, LoggingConfig,
    SessionConfig, StorageConfig,
};
pub use config_watcher::{create_config_watcher, ConfigReceiver, ConfigWatcher, WatcherError};
pub use error::{DaemonError, Result};
pub use mcp_config::{McpConfigError, McpServerConfigFile, McpServersConfig};
pub use permission::{Action, PermissionEnforcer, PermissionResult, ResourceType};
pub use registry::{ActiveRequest, ProxyInfo, ProxyRegistry, RequestRegistry};
pub use router::Router;
pub use server::{find_available_port, start_server, Server, ServerConfig};
pub use session::SessionManager;
pub use wasm::{
    create_linker, HostState, LlmChatRequest, LlmChatResponse, LlmMessage, LlmUsage, WasmConfig,
    WasmInstance, WasmRuntime,
};

pub use agent::{
    create_mock_computer, create_stream_channel, register_computer_tools, AgentContent, AgentInput,
    AgentMode, AgentOutput, AgentProcessInput, AgentProcessOutput, AgentRunner, AgentRunnerConfig,
    GetDisplaysTool, GetMousePositionTool, HistoryEntry, MouseClickTool, MouseDragTool,
    MouseMoveTool, MouseScrollTool, PressKeyTool, ScreenshotTool, StreamEvent, StreamHandle,
    StreamSendError, ToolExecutor, ToolRegistry, TypeTextTool, DEFAULT_STREAM_BUFFER_SIZE,
};

// Platform-specific computer creation
#[cfg(target_os = "linux")]
pub use agent::computer_tools::create_computer;
#[cfg(target_os = "macos")]
pub use agent::computer_tools::create_computer;
#[cfg(target_os = "windows")]
pub use agent::computer_tools::create_computer;
pub use health::{HealthMonitor, HealthStatus};
pub use retry::{with_retry, RetryConfig, Retryable};
pub use secrets::{ApiKey, ApiKeyManager, KeySource, SecretError};
pub use skills::SkillsManager;
pub use trace::{FullTraceSpan, SpanType, TraceSpan};
pub use validation::{
    validate_extension_id, validate_length, validate_path, validate_port, validate_session_id,
    ValidationError,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_registry_registers_proxy() {
        // RED: ProxyRegistry should track connected proxies
        let registry = ProxyRegistry::new();
        registry.register("proxy-001", 12345);

        assert!(registry.is_registered("proxy-001"));
        assert!(!registry.is_registered("proxy-002"));
    }

    #[test]
    fn test_request_registry_tracks_active_requests() {
        // RED: RequestRegistry should track active requests
        let registry = RequestRegistry::new();
        registry.register("req-001", "proxy-001", "session-001");

        let request = registry.get("req-001");
        assert!(request.is_some());
        assert_eq!(request.unwrap().proxy_id, "proxy-001");
    }

    #[test]
    fn test_router_routes_to_correct_proxy() {
        // RED: Router should route messages to the correct proxy
        let router = Router::new();

        // Register a proxy and request
        router.proxy_registry().register("proxy-001", 12345);
        router
            .request_registry()
            .register("req-001", "proxy-001", "session-001");

        // Should find the proxy for a response
        let proxy_id = router.find_proxy_for_request("req-001");
        assert_eq!(proxy_id, Some("proxy-001".to_string()));
    }

    #[tokio::test]
    async fn test_session_manager_creates_session() {
        // RED: SessionManager should create and retrieve sessions
        let manager = SessionManager::in_memory().unwrap();

        let session = manager.create_session(None, None).await.unwrap();
        assert!(!session.id.is_empty());

        let retrieved = manager.get_session(&session.id).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_daemon_config_defaults() {
        // RED: DaemonConfig should have sensible defaults
        let config = DaemonConfig::default();

        assert!(config.idle_timeout_secs > 0);
        assert!(config.heartbeat_timeout_secs > 0);
    }
}
