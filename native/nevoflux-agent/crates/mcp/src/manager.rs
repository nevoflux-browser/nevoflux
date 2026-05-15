//! MCP Manager for external server lifecycle management.
//!
//! Provides higher-level management of MCP servers including:
//! - Automatic reconnection on failure
//! - Periodic health monitoring
//! - Server supervision with configurable policies
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_mcp::{McpManager, ManagerConfig, ServerConfig};
//! use std::time::Duration;
//!
//! let manager = McpManager::new(ManagerConfig {
//!     health_check_interval: Duration::from_secs(30),
//!     auto_reconnect: true,
//!     max_reconnect_attempts: 3,
//!     reconnect_delay: Duration::from_secs(5),
//! });
//!
//! // Add and connect servers
//! manager.add_server(ServerConfig::new("filesystem", "npx")
//!     .with_args(vec!["-y", "@modelcontextprotocol/server-filesystem", "/home"])
//! ).await?;
//!
//! // Start the manager (begins health monitoring)
//! manager.start().await;
//!
//! // Call tools across any server
//! let result = manager.call_tool_any("read_file", json!({"path": "/test.txt"})).await?;
//! ```

use crate::error::{McpError, Result};
use crate::registry::{McpRegistry, ServerConfig, ServerTool};
use crate::types::ToolResult;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;

/// Configuration for the MCP manager.
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Interval between health checks.
    pub health_check_interval: Duration,
    /// Whether to automatically reconnect failed servers.
    pub auto_reconnect: bool,
    /// Maximum number of reconnection attempts.
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts.
    pub reconnect_delay: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(60),
            auto_reconnect: true,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(5),
        }
    }
}

/// Server status information.
#[derive(Debug, Clone)]
pub struct ServerStatus {
    /// Server name.
    pub name: String,
    /// Whether the server is connected.
    pub connected: bool,
    /// Whether the server is healthy (passed last health check).
    pub healthy: bool,
    /// Number of reconnection attempts.
    pub reconnect_attempts: u32,
    /// Last error message if any.
    pub last_error: Option<String>,
}

/// MCP Manager for lifecycle management of external MCP servers.
pub struct McpManager {
    /// Underlying registry.
    registry: Arc<McpRegistry>,
    /// Manager configuration.
    config: ManagerConfig,
    /// Whether the manager is running.
    running: Arc<AtomicBool>,
    /// Reconnection attempts per server.
    reconnect_attempts: Arc<RwLock<HashMap<String, AtomicU32>>>,
    /// Last errors per server.
    last_errors: Arc<RwLock<HashMap<String, String>>>,
    /// Health status per server.
    health_status: Arc<RwLock<HashMap<String, bool>>>,
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new(ManagerConfig::default())
    }
}

impl McpManager {
    /// Create a new MCP manager with the given configuration.
    pub fn new(config: ManagerConfig) -> Self {
        Self {
            registry: Arc::new(McpRegistry::new()),
            config,
            running: Arc::new(AtomicBool::new(false)),
            reconnect_attempts: Arc::new(RwLock::new(HashMap::new())),
            last_errors: Arc::new(RwLock::new(HashMap::new())),
            health_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a manager with an existing registry.
    pub fn with_registry(registry: Arc<McpRegistry>, config: ManagerConfig) -> Self {
        Self {
            registry,
            config,
            running: Arc::new(AtomicBool::new(false)),
            reconnect_attempts: Arc::new(RwLock::new(HashMap::new())),
            last_errors: Arc::new(RwLock::new(HashMap::new())),
            health_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a server configuration and optionally connect immediately.
    pub async fn add_server(&self, config: ServerConfig) -> Result<()> {
        let name = config.name.clone();
        self.registry.add_config(config).await;

        // Initialize tracking state
        self.reconnect_attempts
            .write()
            .await
            .insert(name.clone(), AtomicU32::new(0));
        self.health_status.write().await.insert(name, true);

        Ok(())
    }

    /// Add a server and connect immediately.
    pub async fn add_and_connect(&self, config: ServerConfig) -> Result<()> {
        let name = config.name.clone();
        self.add_server(config).await?;
        self.connect(&name).await
    }

    /// Remove a server configuration.
    pub async fn remove_server(&self, name: &str) -> Result<()> {
        self.registry.remove_config(name).await;
        self.reconnect_attempts.write().await.remove(name);
        self.last_errors.write().await.remove(name);
        self.health_status.write().await.remove(name);
        Ok(())
    }

    /// Connect to a specific server.
    pub async fn connect(&self, name: &str) -> Result<()> {
        match self.registry.connect(name).await {
            Ok(()) => {
                // Reset reconnect attempts on successful connect
                if let Some(attempts) = self.reconnect_attempts.read().await.get(name) {
                    attempts.store(0, Ordering::SeqCst);
                }
                self.last_errors.write().await.remove(name);
                self.health_status
                    .write()
                    .await
                    .insert(name.to_string(), true);
                tracing::info!(server = %name, "Connected to MCP server");
                Ok(())
            }
            Err(e) => {
                self.last_errors
                    .write()
                    .await
                    .insert(name.to_string(), e.to_string());
                self.health_status
                    .write()
                    .await
                    .insert(name.to_string(), false);
                tracing::warn!(server = %name, error = %e, "Failed to connect to MCP server");
                Err(e)
            }
        }
    }

    /// Connect to all configured servers.
    pub async fn connect_all(&self) -> Vec<(String, Result<()>)> {
        let results = self.registry.connect_all().await;

        for (name, result) in &results {
            if result.is_ok() {
                if let Some(attempts) = self.reconnect_attempts.read().await.get(name) {
                    attempts.store(0, Ordering::SeqCst);
                }
                self.last_errors.write().await.remove(name);
                self.health_status.write().await.insert(name.clone(), true);
            } else if let Err(e) = result {
                self.last_errors
                    .write()
                    .await
                    .insert(name.clone(), e.to_string());
                self.health_status.write().await.insert(name.clone(), false);
            }
        }

        results
    }

    /// Disconnect from a specific server.
    pub async fn disconnect(&self, name: &str) -> Result<()> {
        self.registry.disconnect(name).await?;
        self.health_status
            .write()
            .await
            .insert(name.to_string(), false);
        tracing::info!(server = %name, "Disconnected from MCP server");
        Ok(())
    }

    /// Disconnect from all servers.
    pub async fn disconnect_all(&self) -> Result<()> {
        self.registry.disconnect_all().await?;
        self.health_status.write().await.clear();
        tracing::info!("Disconnected from all MCP servers");
        Ok(())
    }

    /// Start the manager (begins background health monitoring).
    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let registry = self.registry.clone();
        let config = self.config.clone();
        let reconnect_attempts = self.reconnect_attempts.clone();
        let last_errors = self.last_errors.clone();
        let health_status = self.health_status.clone();

        tokio::spawn(async move {
            let mut check_interval = interval(config.health_check_interval);

            while running.load(Ordering::SeqCst) {
                check_interval.tick().await;

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                // Perform health checks
                let health_results = registry.health_check_all().await;

                for (name, healthy) in health_results {
                    health_status.write().await.insert(name.clone(), healthy);

                    if !healthy && config.auto_reconnect {
                        // Attempt reconnection
                        let attempts_map = reconnect_attempts.read().await;
                        if let Some(attempts) = attempts_map.get(&name) {
                            let current_attempts = attempts.fetch_add(1, Ordering::SeqCst);

                            if current_attempts < config.max_reconnect_attempts {
                                tracing::info!(
                                    server = %name,
                                    attempt = current_attempts + 1,
                                    max = config.max_reconnect_attempts,
                                    "Attempting to reconnect MCP server"
                                );

                                drop(attempts_map);
                                tokio::time::sleep(config.reconnect_delay).await;

                                match registry.connect(&name).await {
                                    Ok(()) => {
                                        if let Some(attempts) =
                                            reconnect_attempts.read().await.get(&name)
                                        {
                                            attempts.store(0, Ordering::SeqCst);
                                        }
                                        last_errors.write().await.remove(&name);
                                        health_status.write().await.insert(name.clone(), true);
                                        tracing::info!(
                                            server = %name,
                                            "Successfully reconnected MCP server"
                                        );
                                    }
                                    Err(e) => {
                                        last_errors
                                            .write()
                                            .await
                                            .insert(name.clone(), e.to_string());
                                        tracing::warn!(
                                            server = %name,
                                            error = %e,
                                            "Failed to reconnect MCP server"
                                        );
                                    }
                                }
                            } else {
                                tracing::error!(
                                    server = %name,
                                    "Max reconnection attempts reached for MCP server"
                                );
                            }
                        }
                    }
                }
            }
        })
    }

    /// Stop the manager (stops background health monitoring).
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("MCP manager stopped");
    }

    /// Check if the manager is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get status of all servers.
    pub async fn get_all_status(&self) -> Vec<ServerStatus> {
        let configured = self.registry.configured_servers().await;
        let connected = self.registry.connected_servers().await;
        let health = self.health_status.read().await;
        let errors = self.last_errors.read().await;
        let attempts = self.reconnect_attempts.read().await;

        configured
            .into_iter()
            .map(|name| {
                let is_connected = connected.contains(&name);
                let is_healthy = health.get(&name).copied().unwrap_or(false);
                let reconnect_count = attempts
                    .get(&name)
                    .map(|a| a.load(Ordering::SeqCst))
                    .unwrap_or(0);
                let last_error = errors.get(&name).cloned();

                ServerStatus {
                    name,
                    connected: is_connected,
                    healthy: is_healthy,
                    reconnect_attempts: reconnect_count,
                    last_error,
                }
            })
            .collect()
    }

    /// Get status of a specific server.
    pub async fn get_status(&self, name: &str) -> Option<ServerStatus> {
        let all_status = self.get_all_status().await;
        all_status.into_iter().find(|s| s.name == name)
    }

    /// List all available tools from all connected servers.
    pub async fn list_all_tools(&self) -> Result<Vec<ServerTool>> {
        self.registry.list_all_tools().await
    }

    /// Call a tool on a specific server.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult> {
        self.registry.call_tool(server, tool, arguments).await
    }

    /// Call a tool on any server that provides it.
    pub async fn call_tool_any(
        &self,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult> {
        let server = self.registry.find_tool(tool).await.ok_or_else(|| {
            McpError::ConnectionFailed(format!("No server provides tool: {}", tool))
        })?;

        self.registry.call_tool(&server, tool, arguments).await
    }

    /// Get the underlying registry.
    pub fn registry(&self) -> &McpRegistry {
        &self.registry
    }

    /// Reset reconnection attempts for a server.
    pub async fn reset_reconnect_attempts(&self, name: &str) {
        if let Some(attempts) = self.reconnect_attempts.read().await.get(name) {
            attempts.store(0, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_config_default() {
        let config = ManagerConfig::default();

        assert_eq!(config.health_check_interval, Duration::from_secs(60));
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 3);
        assert_eq!(config.reconnect_delay, Duration::from_secs(5));
    }

    #[test]
    fn test_manager_config_custom() {
        let config = ManagerConfig {
            health_check_interval: Duration::from_secs(30),
            auto_reconnect: false,
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(10),
        };

        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert!(!config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 5);
    }

    #[tokio::test]
    async fn test_manager_new() {
        let manager = McpManager::new(ManagerConfig::default());

        assert!(!manager.is_running());
        assert!(manager.get_all_status().await.is_empty());
    }

    #[tokio::test]
    async fn test_manager_add_server() {
        let manager = McpManager::new(ManagerConfig::default());

        manager
            .add_server(ServerConfig::new("test", "echo"))
            .await
            .unwrap();

        let status = manager.get_all_status().await;
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].name, "test");
        assert!(!status[0].connected);
    }

    #[tokio::test]
    async fn test_manager_remove_server() {
        let manager = McpManager::new(ManagerConfig::default());

        manager
            .add_server(ServerConfig::new("test", "echo"))
            .await
            .unwrap();
        manager.remove_server("test").await.unwrap();

        assert!(manager.get_all_status().await.is_empty());
    }

    #[tokio::test]
    async fn test_manager_get_status_not_found() {
        let manager = McpManager::new(ManagerConfig::default());

        let status = manager.get_status("nonexistent").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_manager_start_stop() {
        let manager = McpManager::new(ManagerConfig {
            health_check_interval: Duration::from_millis(100),
            ..Default::default()
        });

        assert!(!manager.is_running());

        let handle = manager.start();
        assert!(manager.is_running());

        manager.stop();
        assert!(!manager.is_running());

        // Give the task time to notice the stop
        tokio::time::sleep(Duration::from_millis(150)).await;
        handle.abort();
    }

    #[tokio::test]
    async fn test_manager_with_registry() {
        let registry = Arc::new(McpRegistry::new());
        registry.add_config(ServerConfig::new("test", "echo")).await;

        let manager = McpManager::with_registry(registry.clone(), ManagerConfig::default());

        // Manager should see the server from the registry
        let configured = manager.registry().configured_servers().await;
        assert!(configured.contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_manager_call_tool_any_not_found() {
        let manager = McpManager::new(ManagerConfig::default());

        let result = manager
            .call_tool_any("nonexistent_tool", serde_json::json!({}))
            .await;

        assert!(matches!(result, Err(McpError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_manager_reset_reconnect_attempts() {
        let manager = McpManager::new(ManagerConfig::default());

        manager
            .add_server(ServerConfig::new("test", "echo"))
            .await
            .unwrap();

        // Simulate some reconnect attempts
        {
            let attempts_map = manager.reconnect_attempts.read().await;
            if let Some(attempts) = attempts_map.get("test") {
                attempts.store(5, Ordering::SeqCst);
            }
        }

        manager.reset_reconnect_attempts("test").await;

        {
            let attempts_map = manager.reconnect_attempts.read().await;
            if let Some(attempts) = attempts_map.get("test") {
                assert_eq!(attempts.load(Ordering::SeqCst), 0);
            }
        }
    }

    #[test]
    fn test_server_status_debug() {
        let status = ServerStatus {
            name: "test".to_string(),
            connected: true,
            healthy: true,
            reconnect_attempts: 0,
            last_error: None,
        };

        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("connected"));
    }

    #[tokio::test]
    async fn test_manager_connect_not_configured() {
        let manager = McpManager::new(ManagerConfig::default());

        let result = manager.connect("nonexistent").await;
        assert!(matches!(result, Err(McpError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_manager_disconnect_not_connected() {
        let manager = McpManager::new(ManagerConfig::default());

        // Should not error when disconnecting a non-connected server
        let result = manager.disconnect("nonexistent").await;
        assert!(result.is_ok());
    }
}
