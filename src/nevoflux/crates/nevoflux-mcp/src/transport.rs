/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! MCP Transport Layer

use anyhow::Result;
use nevoflux_common::config::McpServerConfig;
use tokio::process::{Child, Command};

use crate::protocol::{Request, Response};

/// Transport for communicating with an MCP server
pub struct Transport {
    _process: Child,
    _config: McpServerConfig,
}

impl Transport {
    /// Create a new transport by spawning an MCP server process
    pub async fn new(config: &McpServerConfig) -> Result<Self> {
        let mut command = Command::new(&config.command);
        command.args(&config.args);

        if let Some(env) = &config.env {
            for (key, value) in env {
                command.env(key, value);
            }
        }

        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let process = command.spawn()?;

        Ok(Self {
            _process: process,
            _config: config.clone(),
        })
    }

    /// Send a request and receive a response
    pub async fn send(&mut self, _request: Request) -> Result<Response> {
        // TODO: Implement stdio-based JSON-RPC transport
        anyhow::bail!("Transport send not yet implemented")
    }
}
