/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Native Messaging Message Handler
//!
//! Uses the shared-protocol crate for type-safe message handling.

use anyhow::{Context, Result};
use shared_protocol::{
    AgentState, ChatMessagePayload, ExtensionMessage, PageContextPayload,
    ToolResultPayload, UiActionPayload,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::action_router::ActionRouter;
use crate::llm_integration::{LlmClient, LlmConfig, Message};
use crate::session::SessionManager;
use crate::stream_manager::StreamManager;
use crate::tools::{ToolExecutor, ToolExecutorConfig};

/// Message handler for processing incoming messages
pub struct MessageHandler {
    session_manager: SessionManager,
    action_router: ActionRouter,
    stream_manager: StreamManager,
    tool_executor: Arc<ToolExecutor>,
    llm_client: Option<LlmClient>,
    /// Pending page context requests (session_id -> oneshot sender)
    pending_page_context: Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<PageContextPayload>>>>,
}

impl MessageHandler {
    /// Create a new message handler
    pub fn new(
        session_manager: SessionManager,
        action_router: ActionRouter,
        stream_manager: StreamManager,
    ) -> Self {
        // Create tool executor with the same message sender as stream manager
        let tool_executor = Arc::new(ToolExecutor::new(
            ToolExecutorConfig::default(),
            stream_manager.get_message_sender(),
        ));

        // Try to create LLM client from environment
        let llm_client = match LlmClient::from_env() {
            Ok(client) => {
                info!("LLM client initialized from environment");
                Some(client)
            }
            Err(e) => {
                warn!("LLM client not available: {}. Chat will use placeholder responses.", e);
                None
            }
        };

        Self {
            session_manager,
            action_router,
            stream_manager,
            tool_executor,
            llm_client,
            pending_page_context: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Handle an incoming message using ExtensionMessage protocol
    pub async fn handle_message(&self, message: &str) -> Result<()> {
        // Debug log to file
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/nevoflux-agent-debug.log")
        {
            use std::io::Write;
            let _ = writeln!(f, "Received message: {}", &message[..message.len().min(200)]);
        }

        debug!("Handling message: {}", message);

        // Parse as ExtensionMessage (shared protocol v3.0)
        let msg: ExtensionMessage =
            serde_json::from_str(message).context("Failed to parse ExtensionMessage")?;

        // Extract session_id if present
        let session_id = msg.session_id().map(|s| s.to_string());

        info!(
            "Received ExtensionMessage, session: {:?}",
            session_id
        );

        // Ensure session exists if we have a session_id
        if let Some(ref sid) = session_id {
            let _session = self.session_manager.get_or_create(sid.clone()).await;
        }

        // Route based on message variant
        self.route_message(msg).await
    }

    /// Route message to appropriate handler based on ExtensionMessage variant
    async fn route_message(&self, message: ExtensionMessage) -> Result<()> {
        match message {
            ExtensionMessage::ChatMessage(payload) => {
                self.handle_chat(payload).await
            }
            ExtensionMessage::StopGeneration { session_id } => {
                self.handle_stop_generation(session_id).await
            }
            ExtensionMessage::UiAction(payload) => {
                self.handle_ui_action(payload).await
            }
            ExtensionMessage::RequestTabContext => {
                // This is typically handled by the background script
                debug!("RequestTabContext received - usually handled by background.js");
                Ok(())
            }
            ExtensionMessage::Ping { timestamp } => {
                // Debug log to file
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nevoflux-agent-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "Ping received, sending pong with timestamp: {}", timestamp);
                }

                info!("Ping received at {}", timestamp);
                // Pong is sent via stream_manager
                let result = self.stream_manager.send_pong(timestamp).await;

                // Debug log result
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nevoflux-agent-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "Pong send result: {:?}", result);
                }

                result
            }
            // Downstream messages (shouldn't arrive at agent, but handle gracefully)
            ExtensionMessage::DisplayContent(_)
            | ExtensionMessage::ClearContent { .. }
            | ExtensionMessage::HighlightElement(_)
            | ExtensionMessage::ClearHighlight { .. }
            | ExtensionMessage::InjectContentSidebar { .. } => {
                warn!("Received downstream message at agent - this should be handled by background.js");
                Ok(())
            }
            // Upstream content sidebar messages (routed through background)
            ExtensionMessage::ContentUrlReport(payload) => {
                debug!("ContentUrlReport: {} - {}", payload.url, payload.title);
                Ok(())
            }
            ExtensionMessage::ContentElementClick(payload) => {
                debug!("ContentElementClick: {}", payload.element_tag);
                Ok(())
            }
            ExtensionMessage::ContentSidebarReady { tab_id } => {
                debug!("ContentSidebarReady for tab {}", tab_id);
                Ok(())
            }
            // =========================================================================
            // Page Context (Computer Use)
            // =========================================================================
            ExtensionMessage::RequestPageContext { session_id } => {
                debug!("RequestPageContext for session {}", session_id);
                // This is typically sent by Chat Sidebar, not received by agent
                Ok(())
            }
            ExtensionMessage::PageContextResponse(payload) => {
                self.handle_page_context_response(payload).await
            }

            // =========================================================================
            // Tool Execution (Computer Use)
            // =========================================================================
            ExtensionMessage::ToolCall(_) => {
                // ToolCall is sent by agent, not received
                warn!("Received ToolCall at agent - this should be sent, not received");
                Ok(())
            }
            ExtensionMessage::ToolResult(payload) => {
                self.handle_tool_result(payload).await
            }
            ExtensionMessage::AgentStateUpdate(_) => {
                // AgentStateUpdate is sent by agent, not received
                warn!("Received AgentStateUpdate at agent - this should be sent, not received");
                Ok(())
            }

            // Response messages (shouldn't arrive at agent)
            ExtensionMessage::StreamChunk(_)
            | ExtensionMessage::StreamEnd { .. }
            | ExtensionMessage::AgentError(_)
            | ExtensionMessage::TabContextUpdate(_)
            | ExtensionMessage::ConnectionStatus(_)
            | ExtensionMessage::ContentSidebarInjected { .. }
            | ExtensionMessage::Pong { .. } => {
                warn!("Received response message at agent - ignoring");
                Ok(())
            }
        }
    }

    /// Handle page context response from Content Sidebar
    async fn handle_page_context_response(&self, payload: PageContextPayload) -> Result<()> {
        debug!("Received page context for session {}", payload.session_id);

        // Check if there's a pending request for this session
        let mut pending = self.pending_page_context.write().await;
        if let Some(sender) = pending.remove(&payload.session_id) {
            let _ = sender.send(payload);
        } else {
            debug!("No pending page context request for session {}", payload.session_id);
        }

        Ok(())
    }

    /// Handle tool result from Content Sidebar or Background Script
    async fn handle_tool_result(&self, payload: ToolResultPayload) -> Result<()> {
        debug!(
            "Tool result for call {}: success={}",
            payload.call_id, payload.success
        );

        // Forward to tool executor
        self.tool_executor.handle_tool_result(payload).await
    }

    /// Handle chat message with LLM and tool support
    async fn handle_chat(&self, payload: ChatMessagePayload) -> Result<()> {
        debug!(
            "Chat message for session {}: {} (attachments: {}, has_context: {})",
            payload.session_id,
            payload.text,
            payload.attachments.len(),
            payload.page_context.is_some()
        );

        // Reset tool executor for new conversation
        self.tool_executor.reset().await;

        // Create a stream for response
        let stream_id = format!("stream_{}", payload.message_id);
        self.stream_manager
            .create_text_stream(payload.session_id.clone(), stream_id.clone())
            .await?;

        // Check if LLM client is available
        let llm_client = match &self.llm_client {
            Some(client) => client,
            None => {
                // Fallback to placeholder response
                self.stream_manager
                    .send_text(&stream_id, "LLM not configured. Set ANTHROPIC_API_KEY environment variable.".to_string())
                    .await?;
                self.stream_manager
                    .send_text(&stream_id, format!("\n\nReceived: {}", payload.text))
                    .await?;
                self.stream_manager.finish_stream(&stream_id).await?;
                return Ok(());
            }
        };

        // Build conversation messages
        let mut messages = vec![Message::user(&payload.text)];

        // Agentic loop: continue until LLM stops or max steps reached
        let max_iterations = 10;
        for iteration in 0..max_iterations {
            debug!("Agentic loop iteration {}", iteration);

            // Create text channel for streaming
            let (text_tx, mut text_rx) = tokio::sync::mpsc::unbounded_channel();

            // Spawn task to forward text to stream
            let stream_manager = self.stream_manager.clone();
            let stream_id_clone = stream_id.clone();
            tokio::spawn(async move {
                while let Some(text) = text_rx.recv().await {
                    let _ = stream_manager.send_text(&stream_id_clone, text).await;
                }
            });

            // Call LLM with tools
            let response = match llm_client
                .chat_stream_with_tools(messages.clone(), payload.page_context.as_ref(), text_tx)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    error!("LLM error: {}", e);
                    self.stream_manager
                        .send_text(&stream_id, format!("\n\nError: {}", e))
                        .await?;
                    self.tool_executor.send_error(&payload.session_id, &e.to_string()).await?;
                    break;
                }
            };

            // Check if LLM wants to use tools
            if LlmClient::needs_tool_execution(&response) {
                let tool_uses = LlmClient::extract_tool_uses(&response);

                if tool_uses.is_empty() {
                    debug!("No tool uses found despite tool_use stop reason");
                    break;
                }

                // Add assistant message with tool use to conversation
                messages.push(Message::assistant_with_tool_use(response.content.clone()));

                // Execute each tool
                let mut tool_results = Vec::new();
                for tool_use in &tool_uses {
                    debug!("Executing tool: {} (id: {})", tool_use.name, tool_use.id);

                    // Send thinking indicator
                    self.stream_manager
                        .send_text(&stream_id, format!("\n\n*Executing {}...*\n", tool_use.name))
                        .await?;

                    let result = self.tool_executor
                        .execute_tool(&payload.session_id, tool_use)
                        .await?;

                    tool_results.push(result);
                }

                // Add tool results to conversation
                LlmClient::add_tool_results(&mut messages, tool_results);

            } else {
                // LLM finished without tool use
                debug!("LLM finished (stop_reason: {:?})", response.stop_reason);
                break;
            }
        }

        // Send completion state
        self.tool_executor.send_complete(&payload.session_id).await?;

        self.stream_manager.finish_stream(&stream_id).await?;

        Ok(())
    }

    /// Handle stop generation request
    async fn handle_stop_generation(&self, session_id: String) -> Result<()> {
        debug!("Stop generation for session {}", session_id);

        // Stop all streams for this session
        let streams = self.stream_manager.list_streams().await;
        for stream_id in streams {
            self.stream_manager.finish_stream(&stream_id).await.ok();
        }

        Ok(())
    }

    /// Handle UI action
    async fn handle_ui_action(&self, payload: UiActionPayload) -> Result<()> {
        debug!(
            "UI action for session {}: action={} component={}",
            payload.session_id, payload.action_id, payload.component_id
        );

        // Convert form_data to HashMap for action router
        let form_data = payload
            .form_data
            .map(|v| {
                v.as_object()
                    .map(|obj| {
                        obj.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Route to action handler
        let result = self
            .action_router
            .handle(payload.session_id, payload.action_id.clone(), form_data)
            .await?;

        debug!("Action result: {:?}", result);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_chat_message() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let session_manager = SessionManager::new();
        let action_router = ActionRouter::new();
        let stream_manager = StreamManager::new(tx);

        let handler = MessageHandler::new(session_manager, action_router, stream_manager);

        // Use ExtensionMessage format (v3.1)
        let message = r#"{
            "type": "chat_message",
            "payload": {
                "session_id": "session-1",
                "message_id": "msg-123",
                "text": "Hello",
                "attachments": [],
                "include_page_context": false
            }
        }"#;

        handler.handle_message(message).await.unwrap();

        // Should receive stream messages (either stream_chunk or agent state)
        let msg1 = rx.recv().await.unwrap();
        // Without LLM configured, it will send a fallback message
        assert!(msg1.contains("stream_chunk") || msg1.contains("agent_state"));
    }

    #[tokio::test]
    async fn test_tool_result_handling() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let session_manager = SessionManager::new();
        let action_router = ActionRouter::new();
        let stream_manager = StreamManager::new(tx);

        let handler = MessageHandler::new(session_manager, action_router, stream_manager);

        // Test tool result message
        let message = r#"{
            "type": "tool_result",
            "payload": {
                "call_id": "call-123",
                "session_id": "session-1",
                "success": true,
                "result": {"clicked": true},
                "error": null
            }
        }"#;

        // Should not error even though there's no pending call
        let result = handler.handle_message(message).await;
        assert!(result.is_ok());
    }
}
