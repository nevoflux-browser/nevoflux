/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 3: MCP Server handler (bidirectional)

use anyhow::Result;
use shared_protocol::{McpMessage, McpRequestPayload, McpResponsePayload};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info};

/// Handler for Channel 3 (MCP Server)
pub struct McpChannelHandler {
    outgoing: mpsc::UnboundedSender<String>,
    pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<McpResponsePayload>>>>,
}

impl McpChannelHandler {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                outgoing: sender,
                pending_requests: Arc::new(RwLock::new(HashMap::new())),
            },
            receiver,
        )
    }

    /// Handle incoming MCP message (response from extension)
    pub async fn handle_message(&self, json: &str) -> Result<()> {
        let message: McpMessage = serde_json::from_str(json)?;
        debug!("MCP channel received: {:?}", message);

        match message {
            McpMessage::McpResponse(payload) => {
                info!("MCP response: request={}", payload.request_id);
                let mut pending = self.pending_requests.write().await;
                if let Some(sender) = pending.remove(&payload.request_id) {
                    let _ = sender.send(payload);
                }
            }
            McpMessage::McpRequest(_) => {
                // Requests are sent, not received
                debug!("Unexpected MCP request received");
            }
        }
        Ok(())
    }

    /// Send MCP request to extension
    pub async fn send_request(&self, request: McpRequestPayload) -> Result<McpResponsePayload> {
        let request_id = request.request_id.clone();
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), tx);
        }

        let msg = McpMessage::McpRequest(request);
        let json = serde_json::to_string(&msg)?;
        self.outgoing.send(json)?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => anyhow::bail!("MCP response channel closed"),
            Err(_) => {
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                anyhow::bail!("MCP request timeout: {}", request_id)
            }
        }
    }
}

impl Default for McpChannelHandler {
    fn default() -> Self {
        Self::new().0
    }
}
