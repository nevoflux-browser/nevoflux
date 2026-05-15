//! MCP Server Configuration persistence.
//!
//! This module provides TOML-based configuration loading and saving
//! for MCP servers from ~/.config/nevoflux/mcp-servers.toml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during MCP configuration operations.
#[derive(Debug, Error)]
pub enum McpConfigError {
    /// Failed to read configuration file.
    #[error("failed to read MCP configuration file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse configuration file.
    #[error("failed to parse MCP configuration file: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Failed to serialize configuration.
    #[error("failed to serialize MCP configuration: {0}")]
    SerializeError(#[from] toml::ser::Error),

    /// No config directory found.
    #[error("could not determine config directory")]
    NoConfigDir,

    /// Server not found.
    #[error("MCP server not found: {0}")]
    ServerNotFound(String),

    /// Server already exists.
    #[error("MCP server already exists: {0}")]
    ServerExists(String),

    /// Invalid server configuration.
    #[error("invalid server configuration: {0}")]
    InvalidConfig(String),
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigFile {
    /// Server name (unique identifier).
    pub name: String,

    /// Server type: "stdio", "http", or "sse".
    #[serde(default = "default_stdio")]
    pub server_type: String,

    /// Command to run the server (required for stdio type).
    pub command: Option<String>,

    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Whether the server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Environment variables for the server process.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Human-readable description.
    pub description: Option<String>,

    /// Working directory for the server process.
    pub work_dir: Option<String>,

    /// URL for HTTP/SSE server types.
    pub url: Option<String>,

    /// Connection timeout in seconds.
    pub timeout: Option<u64>,

    /// HTTP headers for HTTP/SSE connections.
    pub headers: Option<HashMap<String, String>>,

    /// Reconnect interval in seconds for SSE.
    pub reconnect: Option<u64>,

    /// HTTP method override.
    pub method: Option<String>,

    /// API key for authenticated connections.
    pub api_key: Option<String>,
}

impl McpServerConfigFile {
    /// Create a new MCP server config with the given name and command.
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            server_type: "stdio".to_string(),
            command: Some(command.into()),
            args: Vec::new(),
            enabled: true,
            env: HashMap::new(),
            description: None,
            work_dir: None,
            url: None,
            timeout: None,
            headers: None,
            reconnect: None,
            method: None,
            api_key: None,
        }
    }

    /// Set the arguments.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Set enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set environment variables.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), McpConfigError> {
        if self.name.is_empty() {
            return Err(McpConfigError::InvalidConfig("name cannot be empty".into()));
        }
        // Name should be alphanumeric with dashes/underscores
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(McpConfigError::InvalidConfig(
                "name must be alphanumeric with dashes or underscores".into(),
            ));
        }
        match self.server_type.as_str() {
            "http" | "sse" => {
                if self.url.is_none() {
                    return Err(McpConfigError::InvalidConfig(
                        "url is required for http/sse server type".into(),
                    ));
                }
            }
            _ => {
                // stdio (default)
                if self.command.as_ref().map_or(true, |c| c.is_empty()) {
                    return Err(McpConfigError::InvalidConfig(
                        "command cannot be empty for stdio server type".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

fn default_stdio() -> String {
    "stdio".to_string()
}

/// Root configuration structure for MCP servers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServersConfig {
    /// List of configured MCP servers.
    #[serde(default)]
    pub servers: Vec<McpServerConfigFile>,
}

impl McpServersConfig {
    /// Returns the default configuration file path.
    ///
    /// This is typically ~/.config/nevoflux/mcp-servers.toml on Linux/macOS
    /// or %APPDATA%\nevoflux\mcp-servers.toml on Windows.
    pub fn config_path() -> Result<PathBuf, McpConfigError> {
        let config_dir = dirs::config_dir().ok_or(McpConfigError::NoConfigDir)?;
        Ok(config_dir.join("nevoflux").join("mcp-servers.toml"))
    }

    /// Load configuration from the default path.
    ///
    /// Returns empty configuration if the file doesn't exist.
    pub fn load() -> Result<Self, McpConfigError> {
        let path = Self::config_path()?;
        Self::load_from_path(&path)
    }

    /// Load configuration from a specific path.
    ///
    /// Returns empty configuration if the file doesn't exist.
    pub fn load_from_path(path: &PathBuf) -> Result<Self, McpConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let config: McpServersConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to the default path.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save(&self) -> Result<(), McpConfigError> {
        let path = Self::config_path()?;
        self.save_to_path(&path)
    }

    /// Save configuration to a specific path.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save_to_path(&self, path: &PathBuf) -> Result<(), McpConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add a new server configuration.
    ///
    /// Returns error if a server with the same name already exists.
    pub fn add_server(&mut self, server: McpServerConfigFile) -> Result<(), McpConfigError> {
        server.validate()?;

        if self.servers.iter().any(|s| s.name == server.name) {
            return Err(McpConfigError::ServerExists(server.name));
        }

        self.servers.push(server);
        Ok(())
    }

    /// Update an existing server configuration.
    ///
    /// Returns error if the server doesn't exist.
    pub fn update_server(
        &mut self,
        name: &str,
        server: McpServerConfigFile,
    ) -> Result<(), McpConfigError> {
        server.validate()?;

        let index = self
            .servers
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| McpConfigError::ServerNotFound(name.to_string()))?;

        self.servers[index] = server;
        Ok(())
    }

    /// Remove a server configuration.
    ///
    /// Returns true if the server was found and removed.
    pub fn remove_server(&mut self, name: &str) -> bool {
        let len_before = self.servers.len();
        self.servers.retain(|s| s.name != name);
        self.servers.len() < len_before
    }

    /// Get a server configuration by name.
    pub fn get_server(&self, name: &str) -> Option<&McpServerConfigFile> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// Get a mutable reference to a server configuration by name.
    pub fn get_server_mut(&mut self, name: &str) -> Option<&mut McpServerConfigFile> {
        self.servers.iter_mut().find(|s| s.name == name)
    }

    /// Get all enabled servers.
    pub fn enabled_servers(&self) -> Vec<&McpServerConfigFile> {
        self.servers.iter().filter(|s| s.enabled).collect()
    }

    /// Check if a server with the given name exists.
    pub fn has_server(&self, name: &str) -> bool {
        self.servers.iter().any(|s| s.name == name)
    }

    /// Get the number of configured servers.
    pub fn len(&self) -> usize {
        self.servers.len()
    }

    /// Check if the configuration is empty.
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_new() {
        let server = McpServerConfigFile::new("test-server", "npx");

        assert_eq!(server.name, "test-server");
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(server.server_type, "stdio");
        assert!(server.args.is_empty());
        assert!(server.enabled);
        assert!(server.env.is_empty());
    }

    #[test]
    fn test_mcp_server_config_builder() {
        let server = McpServerConfigFile::new("test-server", "npx")
            .with_args(vec!["-y".to_string(), "@test/server".to_string()])
            .with_enabled(false)
            .with_env(HashMap::from([("API_KEY".to_string(), "xxx".to_string())]));

        assert_eq!(server.args, vec!["-y", "@test/server"]);
        assert!(!server.enabled);
        assert_eq!(server.env.get("API_KEY"), Some(&"xxx".to_string()));
    }

    #[test]
    fn test_mcp_server_config_validation() {
        // Valid config
        let valid = McpServerConfigFile::new("test-server", "npx");
        assert!(valid.validate().is_ok());

        // Empty name
        let invalid_name = McpServerConfigFile::new("", "npx");
        assert!(invalid_name.validate().is_err());

        // Empty command
        let invalid_cmd = McpServerConfigFile::new("test", "");
        assert!(invalid_cmd.validate().is_err());

        // Invalid characters in name
        let invalid_chars = McpServerConfigFile::new("test server", "npx");
        assert!(invalid_chars.validate().is_err());
    }

    #[test]
    fn test_mcp_servers_config_add_server() {
        let mut config = McpServersConfig::default();

        let server1 = McpServerConfigFile::new("server1", "cmd1");
        config.add_server(server1).unwrap();

        assert_eq!(config.len(), 1);
        assert!(config.has_server("server1"));

        // Adding duplicate should fail
        let server1_dup = McpServerConfigFile::new("server1", "cmd2");
        assert!(config.add_server(server1_dup).is_err());
    }

    #[test]
    fn test_mcp_servers_config_update_server() {
        let mut config = McpServersConfig::default();

        let server = McpServerConfigFile::new("server1", "cmd1");
        config.add_server(server).unwrap();

        // Update existing
        let updated = McpServerConfigFile::new("server1", "cmd2").with_enabled(false);
        config.update_server("server1", updated).unwrap();

        let server = config.get_server("server1").unwrap();
        assert_eq!(server.command.as_deref(), Some("cmd2"));
        assert!(!server.enabled);

        // Update non-existent should fail
        let new = McpServerConfigFile::new("server2", "cmd");
        assert!(config.update_server("server2", new).is_err());
    }

    #[test]
    fn test_mcp_servers_config_remove_server() {
        let mut config = McpServersConfig::default();

        config
            .add_server(McpServerConfigFile::new("server1", "cmd1"))
            .unwrap();
        config
            .add_server(McpServerConfigFile::new("server2", "cmd2"))
            .unwrap();

        assert!(config.remove_server("server1"));
        assert_eq!(config.len(), 1);
        assert!(!config.has_server("server1"));
        assert!(config.has_server("server2"));

        // Remove non-existent returns false
        assert!(!config.remove_server("server1"));
    }

    #[test]
    fn test_mcp_servers_config_enabled_servers() {
        let mut config = McpServersConfig::default();

        config
            .add_server(McpServerConfigFile::new("server1", "cmd1").with_enabled(true))
            .unwrap();
        config
            .add_server(McpServerConfigFile::new("server2", "cmd2").with_enabled(false))
            .unwrap();
        config
            .add_server(McpServerConfigFile::new("server3", "cmd3").with_enabled(true))
            .unwrap();

        let enabled = config.enabled_servers();
        assert_eq!(enabled.len(), 2);
        assert!(enabled.iter().any(|s| s.name == "server1"));
        assert!(enabled.iter().any(|s| s.name == "server3"));
    }

    #[test]
    fn test_mcp_servers_config_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("mcp-servers.toml");

        let mut config = McpServersConfig::default();
        config
            .add_server(
                McpServerConfigFile::new("filesystem", "npx")
                    .with_args(vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-filesystem".to_string(),
                    ])
                    .with_env(HashMap::from([(
                        "PATH".to_string(),
                        "/usr/bin".to_string(),
                    )])),
            )
            .unwrap();

        config.save_to_path(&config_path).unwrap();

        // Verify file exists and content
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[[servers]]"));
        assert!(content.contains("name = \"filesystem\""));

        // Load it back
        let loaded = McpServersConfig::load_from_path(&config_path).unwrap();
        assert_eq!(loaded.len(), 1);

        let server = loaded.get_server("filesystem").unwrap();
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(server.args.len(), 2);
        assert!(server.enabled);
    }

    #[test]
    fn test_mcp_servers_config_load_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/mcp-servers.toml");
        let config = McpServersConfig::load_from_path(&path).unwrap();
        assert!(config.is_empty());
    }
}
