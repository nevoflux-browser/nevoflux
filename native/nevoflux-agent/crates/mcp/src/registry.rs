//! MCP server registry for managing multiple MCP connections.
//!
//! The registry allows managing multiple MCP server connections,
//! aggregating their tools and resources.

use crate::backend::McpClientBackend;
use crate::error::{McpError, Result};
use crate::types::{Resource, ToolDefinition, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(not(feature = "legacy-backend"))]
use crate::rmcp_adapter::RmcpClient;

#[cfg(feature = "legacy-backend")]
use crate::client::McpClient;

/// Transport type for connecting to an MCP server.
#[derive(Debug, Clone, Default)]
pub enum TransportType {
    /// Stdio transport (spawn a child process).
    #[default]
    Stdio,
    /// HTTP/SSE (Streamable HTTP) transport.
    Http,
}

/// Configuration for an MCP server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server name (unique identifier).
    pub name: String,
    /// Command to execute (for stdio) or URL (for http/sse).
    pub command: String,
    /// Arguments to pass to the command (stdio only).
    pub args: Vec<String>,
    /// Whether the server is enabled.
    pub enabled: bool,
    /// Environment variables to set (stdio only).
    pub env: HashMap<String, String>,
    /// Transport type.
    pub transport: TransportType,
}

impl ServerConfig {
    /// Create a new server configuration (stdio transport).
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            enabled: true,
            env: HashMap::new(),
            transport: TransportType::Stdio,
        }
    }

    /// Create an HTTP/SSE server configuration.
    pub fn new_http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: url.into(),
            args: Vec::new(),
            enabled: true,
            env: HashMap::new(),
            transport: TransportType::Http,
        }
    }

    /// Add arguments.
    pub fn with_args(mut self, args: Vec<impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Set enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Add environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// A tool with its server origin.
#[derive(Debug, Clone)]
pub struct ServerTool {
    /// The server this tool belongs to.
    pub server_name: String,
    /// The tool definition.
    pub tool: ToolDefinition,
}

/// A resource with its server origin.
#[derive(Debug, Clone)]
pub struct ServerResource {
    /// The server this resource belongs to.
    pub server_name: String,
    /// The resource definition.
    pub resource: Resource,
}

/// Registry for managing multiple MCP server connections.
pub struct McpRegistry {
    /// Connected clients by server name.
    clients: RwLock<HashMap<String, Arc<dyn McpClientBackend>>>,
    /// Server configurations.
    configs: RwLock<HashMap<String, ServerConfig>>,
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl McpRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            configs: RwLock::new(HashMap::new()),
        }
    }

    /// Add a server configuration.
    pub async fn add_config(&self, config: ServerConfig) {
        self.configs
            .write()
            .await
            .insert(config.name.clone(), config);
    }

    /// Remove a server configuration.
    pub async fn remove_config(&self, name: &str) -> Option<ServerConfig> {
        // Also disconnect if connected
        self.disconnect(name).await.ok();
        self.configs.write().await.remove(name)
    }

    /// Get all configured server names.
    pub async fn configured_servers(&self) -> Vec<String> {
        self.configs.read().await.keys().cloned().collect()
    }

    /// Get all connected server names.
    pub async fn connected_servers(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }

    /// Connect to a configured server by name.
    ///
    /// Uses the official rmcp SDK when the `rmcp-backend` feature is enabled,
    /// otherwise uses the custom McpClient implementation.
    pub async fn connect(&self, name: &str) -> Result<()> {
        let config = self
            .configs
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| {
                McpError::ConnectionFailed(format!("Server not configured: {}", name))
            })?;

        if !config.enabled {
            return Err(McpError::ConnectionFailed(format!(
                "Server disabled: {}",
                name
            )));
        }

        let args: Vec<&str> = config.args.iter().map(|s| s.as_str()).collect();

        // Connect using the appropriate transport
        #[cfg(not(feature = "legacy-backend"))]
        let client: Arc<dyn McpClientBackend> = match config.transport {
            TransportType::Http => Arc::new(RmcpClient::connect_http(&config.command).await?),
            TransportType::Stdio => Arc::new(
                RmcpClient::connect_stdio_with_env(&config.command, &args, &config.env).await?,
            ),
        };

        #[cfg(feature = "legacy-backend")]
        let client: Arc<dyn McpClientBackend> =
            Arc::new(McpClient::connect_stdio_with_env(&config.command, &args, &config.env).await?);

        self.clients.write().await.insert(name.to_string(), client);

        Ok(())
    }

    /// Connect to all enabled servers concurrently.
    pub async fn connect_all(&self) -> Vec<(String, Result<()>)> {
        let configs: Vec<ServerConfig> = self
            .configs
            .read()
            .await
            .values()
            .filter(|c| c.enabled)
            .cloned()
            .collect();

        let futures: Vec<_> = configs
            .into_iter()
            .map(|config| {
                let name = config.name.clone();
                async move {
                    let result = self.connect(&name).await;
                    (name, result)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// Disconnect from a server.
    pub async fn disconnect(&self, name: &str) -> Result<()> {
        if let Some(client) = self.clients.write().await.remove(name) {
            client.close().await?;
        }
        Ok(())
    }

    /// Disconnect from all servers.
    pub async fn disconnect_all(&self) -> Result<()> {
        let clients: Vec<Arc<dyn McpClientBackend>> =
            self.clients.write().await.drain().map(|(_, c)| c).collect();

        for client in clients {
            let _ = client.close().await;
        }
        Ok(())
    }

    /// Get a specific client by name.
    pub async fn get_client(&self, name: &str) -> Option<Arc<dyn McpClientBackend>> {
        self.clients.read().await.get(name).cloned()
    }

    /// List all tools from all connected servers.
    pub async fn list_all_tools(&self) -> Result<Vec<ServerTool>> {
        let clients = self.clients.read().await;
        let mut all_tools = Vec::new();

        for (name, client) in clients.iter() {
            match client.list_tools().await {
                Ok(tools) => {
                    for tool in tools {
                        all_tools.push(ServerTool {
                            server_name: name.clone(),
                            tool,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to list tools from {}: {}", name, e);
                }
            }
        }

        Ok(all_tools)
    }

    /// List all resources from all connected servers.
    pub async fn list_all_resources(&self) -> Result<Vec<ServerResource>> {
        let clients = self.clients.read().await;
        let mut all_resources = Vec::new();

        for (name, client) in clients.iter() {
            match client.list_resources().await {
                Ok(resources) => {
                    for resource in resources {
                        all_resources.push(ServerResource {
                            server_name: name.clone(),
                            resource,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to list resources from {}: {}", name, e);
                }
            }
        }

        Ok(all_resources)
    }

    /// Call a tool on a specific server.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult> {
        let client = self
            .clients
            .read()
            .await
            .get(server)
            .cloned()
            .ok_or_else(|| {
                McpError::ConnectionFailed(format!("Server not connected: {}", server))
            })?;

        client.call_tool(tool, arguments).await
    }

    /// Find which server provides a tool.
    pub async fn find_tool(&self, tool_name: &str) -> Option<String> {
        let clients = self.clients.read().await;

        for (server_name, client) in clients.iter() {
            if let Ok(tools) = client.list_tools().await {
                if tools.iter().any(|t| t.name == tool_name) {
                    return Some(server_name.clone());
                }
            }
        }

        None
    }

    /// Check health of all connected servers.
    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let clients = self.clients.read().await;
        let mut results = HashMap::new();

        for (name, client) in clients.iter() {
            let healthy = client.health_check().await.unwrap_or(false);
            results.insert(name.clone(), healthy);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_new() {
        let config = ServerConfig::new("test-server", "node");

        assert_eq!(config.name, "test-server");
        assert_eq!(config.command, "node");
        assert!(config.args.is_empty());
        assert!(config.enabled);
    }

    #[test]
    fn test_server_config_builder() {
        let config = ServerConfig::new("test", "npx")
            .with_args(vec!["-y", "@anthropic/mcp-server"])
            .with_enabled(false)
            .with_env("NODE_ENV", "production");

        assert_eq!(config.args, vec!["-y", "@anthropic/mcp-server"]);
        assert!(!config.enabled);
        assert_eq!(config.env.get("NODE_ENV"), Some(&"production".to_string()));
    }

    #[tokio::test]
    async fn test_registry_new() {
        let registry = McpRegistry::new();

        assert!(registry.configured_servers().await.is_empty());
        assert!(registry.connected_servers().await.is_empty());
    }

    #[tokio::test]
    async fn test_registry_add_config() {
        let registry = McpRegistry::new();

        registry.add_config(ServerConfig::new("test", "echo")).await;

        let servers = registry.configured_servers().await;
        assert_eq!(servers.len(), 1);
        assert!(servers.contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_registry_remove_config() {
        let registry = McpRegistry::new();

        registry.add_config(ServerConfig::new("test", "echo")).await;
        registry.remove_config("test").await;

        assert!(registry.configured_servers().await.is_empty());
    }

    #[tokio::test]
    async fn test_registry_connect_not_configured() {
        let registry = McpRegistry::new();

        let result = registry.connect("nonexistent").await;
        assert!(matches!(result, Err(McpError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_registry_connect_disabled() {
        let registry = McpRegistry::new();

        registry
            .add_config(ServerConfig::new("test", "echo").with_enabled(false))
            .await;

        let result = registry.connect("test").await;
        assert!(matches!(result, Err(McpError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_registry_disconnect_not_connected() {
        let registry = McpRegistry::new();

        // Should not error
        let result = registry.disconnect("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_health_check_empty() {
        let registry = McpRegistry::new();

        let results = registry.health_check_all().await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_server_config_with_multiple_env_vars() {
        let config = ServerConfig::new("test", "node")
            .with_env("API_KEY", "secret123")
            .with_env("DEBUG", "true")
            .with_env("NODE_ENV", "production");

        assert_eq!(config.env.len(), 3);
        assert_eq!(config.env.get("API_KEY"), Some(&"secret123".to_string()));
        assert_eq!(config.env.get("DEBUG"), Some(&"true".to_string()));
        assert_eq!(config.env.get("NODE_ENV"), Some(&"production".to_string()));
    }
}
