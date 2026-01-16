/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! MCP Client

use anyhow::Result;
use nevoflux_common::config::McpConfig;
use serde_json::Value;

use crate::transport::Transport;

/// MCP client for communicating with MCP servers
pub struct McpClient {
    transports: Vec<Transport>,
}

impl McpClient {
    /// Create a new MCP client
    pub async fn new(config: &McpConfig) -> Result<Self> {
        let mut transports = Vec::new();

        for server_config in &config.servers {
            let transport = Transport::new(server_config).await?;
            transports.push(transport);
        }

        Ok(Self { transports })
    }

    /// Call an MCP tool
    pub async fn call(&mut self, request: &Value) -> Result<Value> {
        // TODO: Implement MCP protocol call
        // This is a placeholder that returns an error
        anyhow::bail!("MCP call not yet implemented")
    }

    /// List available tools from all servers
    pub async fn list_tools(&mut self) -> Result<Vec<String>> {
        // TODO: Implement tool listing
        Ok(vec![])
    }
}
