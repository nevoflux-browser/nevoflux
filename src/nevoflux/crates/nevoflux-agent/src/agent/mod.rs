/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Agent Core Logic
//!
//! Orchestrates AI capabilities including LLM calls, MCP services,
//! WASM plugins, and browser control.

use anyhow::{Context, Result};
use serde_json::Value;
use tracing::{debug, info};

use nevoflux_browser::BrowserController;
use nevoflux_common::config::Config;
use nevoflux_llm::LlmClient;
use nevoflux_mcp::McpClient;
// use nevoflux_wasm::WasmRuntime;  // Temporarily disabled - requires Rust 1.82+

/// Main agent that handles all operations
pub struct Agent {
    config: Config,
    llm_client: LlmClient,
    mcp_client: Option<McpClient>,
    // wasm_runtime: WasmRuntime,  // Temporarily disabled
    browser: BrowserController,
}

impl Agent {
    /// Create a new agent instance
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing agent");

        // Initialize LLM client
        let llm_client = LlmClient::new(&config.llm).context("Failed to create LLM client")?;

        // Initialize MCP client if configured
        let mcp_client = if config.mcp.enabled {
            Some(McpClient::new(&config.mcp).await?)
        } else {
            None
        };

        // WASM runtime temporarily disabled - requires Rust 1.82+
        // let wasm_runtime = WasmRuntime::new(&config.wasm)?;

        // Initialize browser controller
        let browser = BrowserController::new();

        Ok(Self {
            config,
            llm_client,
            mcp_client,
            // wasm_runtime,
            browser,
        })
    }

    /// Handle a message from the browser extension
    pub async fn handle_message(&mut self, message: Value) -> Result<Value> {
        debug!("Handling message: {:?}", message);

        // Extract message type and data
        let msg_type = message
            .get("type")
            .and_then(|v| v.as_str())
            .context("Missing message type")?;

        let data = message.get("data").context("Missing message data")?;

        match msg_type {
            "agent_request" => self.handle_agent_request(data).await,
            "browser_action" => self.handle_browser_action(data).await,
            "mcp_call" => self.handle_mcp_call(data).await,
            "wasm_invoke" => self.handle_wasm_invoke(data).await,
            _ => {
                anyhow::bail!("Unknown message type: {}", msg_type)
            }
        }
    }

    /// Handle an agent request (natural language input)
    async fn handle_agent_request(&mut self, data: &Value) -> Result<Value> {
        let message = data
            .get("message")
            .and_then(|v| v.as_str())
            .context("Missing message")?;

        info!("Processing agent request: {}", message);

        // TODO: Implement full agent logic with planning and execution
        // For now, just echo through LLM
        let response = self.llm_client.chat(message).await?;

        Ok(serde_json::json!({
            "type": "agent_response",
            "content": response
        }))
    }

    /// Handle a browser action request
    async fn handle_browser_action(&mut self, data: &Value) -> Result<Value> {
        debug!("Handling browser action: {:?}", data);

        let result = self.browser.execute(data).await?;

        Ok(serde_json::json!({
            "type": "browser_action_result",
            "result": result
        }))
    }

    /// Handle an MCP service call
    async fn handle_mcp_call(&mut self, data: &Value) -> Result<Value> {
        debug!("Handling MCP call: {:?}", data);

        let mcp_client = self
            .mcp_client
            .as_mut()
            .context("MCP client not initialized")?;

        let result = mcp_client.call(data).await?;

        Ok(serde_json::json!({
            "type": "mcp_result",
            "result": result
        }))
    }

    /// Handle a WASM plugin invocation
    async fn handle_wasm_invoke(&mut self, data: &Value) -> Result<Value> {
        debug!("Handling WASM invoke: {:?}", data);

        // WASM runtime temporarily disabled - requires Rust 1.82+
        anyhow::bail!("WASM runtime temporarily disabled")
    }
}
