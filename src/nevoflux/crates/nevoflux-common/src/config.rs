/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Configuration management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub mcp: McpConfig,
    pub wasm: WasmConfig,
}

impl Config {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref()).context("Failed to read config file")?;
        toml::from_str(&contents).context("Failed to parse config file")
    }

    /// Get default config file path
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Failed to get config directory")?;
        Ok(config_dir.join("nevoflux").join("config.toml"))
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub api_key: Option<String>,
    pub model: String,
    pub api_base: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Anthropic,
            api_key: None,
            model: "claude-3-5-sonnet-20241022".to_string(),
            api_base: None,
        }
    }
}

/// LLM provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Custom,
}

/// MCP configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub servers: Vec<McpServerConfig>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// WASM runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    pub enabled: bool,
    pub plugin_dir: Option<PathBuf>,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plugin_dir: None,
        }
    }
}
