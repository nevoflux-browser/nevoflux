/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tool Executor
//!
//! Manages tool execution by sending requests to the extension (via native messaging)
//! and tracking pending tool calls until results are received.

use anyhow::{Context, Result};
use shared_protocol::{AgentState, AgentStatePayload, BrowserTool, ExtensionMessage, ToolCallPayload, ToolResultPayload};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use super::{ToolResultBlock, ToolUse};

/// Tool execution configuration
#[derive(Debug, Clone)]
pub struct ToolExecutorConfig {
    /// Maximum steps in agentic loop
    pub max_steps: u32,
    /// Tool execution timeout
    pub tool_timeout: Duration,
    /// Tools requiring user confirmation
    pub dangerous_tools: Vec<String>,
    /// Whether to require confirmation for dangerous tools
    pub require_confirmation: bool,
}

impl Default for ToolExecutorConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            tool_timeout: Duration::from_secs(30),
            dangerous_tools: vec![
                "navigate".to_string(),
                "evaluate_js".to_string(),
                "fill_form".to_string(),
            ],
            require_confirmation: true,
        }
    }
}

/// Pending tool call awaiting result
struct PendingToolCall {
    call_id: String,
    tool_name: String,
    started_at: Instant,
    result_sender: oneshot::Sender<ToolResultPayload>,
}

/// Tool executor for managing browser automation
pub struct ToolExecutor {
    config: ToolExecutorConfig,
    message_sender: mpsc::UnboundedSender<String>,
    pending_calls: Arc<RwLock<HashMap<String, PendingToolCall>>>,
    step_count: Arc<RwLock<u32>>,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new(
        config: ToolExecutorConfig,
        message_sender: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            config,
            message_sender,
            pending_calls: Arc::new(RwLock::new(HashMap::new())),
            step_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Execute a tool and wait for result
    pub async fn execute_tool(
        &self,
        session_id: &str,
        tool_use: &ToolUse,
    ) -> Result<ToolResultBlock> {
        let call_id = format!("call_{}", uuid::Uuid::new_v4());
        let tool_name = &tool_use.name;

        info!("Executing tool: {} (call_id: {})", tool_name, call_id);

        // Check max steps
        {
            let mut step = self.step_count.write().await;
            *step += 1;
            if *step > self.config.max_steps {
                return Ok(ToolResultBlock::error(
                    &tool_use.id,
                    format!("Maximum steps ({}) reached", self.config.max_steps),
                ));
            }
        }

        // Send state update: Executing tool
        self.send_state_update(session_id, AgentState::ExecutingTool, Some(tool_name.clone())).await?;

        // Check if tool requires confirmation
        if self.config.require_confirmation && self.config.dangerous_tools.contains(tool_name) {
            // For now, we proceed. In a full implementation, we'd wait for user confirmation
            info!("Tool {} would require confirmation in production", tool_name);
        }

        // Create result channel
        let (result_tx, result_rx) = oneshot::channel();

        // Register pending call
        {
            let pending = PendingToolCall {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                started_at: Instant::now(),
                result_sender: result_tx,
            };
            self.pending_calls.write().await.insert(call_id.clone(), pending);
        }

        // Send tool call to extension
        let tool_call = ExtensionMessage::ToolCall(ToolCallPayload {
            call_id: call_id.clone(),
            session_id: session_id.to_string(),
            tool_name: tool_name.clone(),
            parameters: tool_use.input.clone(),
            show_feedback: true,
        });

        let json = serde_json::to_string(&tool_call).context("Failed to serialize tool call")?;
        self.message_sender.send(json).map_err(|_| anyhow::anyhow!("Failed to send tool call"))?;

        // Send state update: Waiting for result
        self.send_state_update(session_id, AgentState::WaitingResult, Some(tool_name.clone())).await?;

        // Wait for result with timeout
        let result = tokio::time::timeout(self.config.tool_timeout, result_rx)
            .await
            .map_err(|_| {
                // Remove from pending on timeout
                let pending = self.pending_calls.clone();
                let cid = call_id.clone();
                tokio::spawn(async move {
                    pending.write().await.remove(&cid);
                });
                anyhow::anyhow!("Tool execution timed out after {:?}", self.config.tool_timeout)
            })?
            .map_err(|_| anyhow::anyhow!("Tool result channel closed"))?;

        // Convert to ToolResultBlock for LLM
        if result.success {
            let content = result.result
                .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()))
                .unwrap_or_else(|| "Success".to_string());
            Ok(ToolResultBlock::success(&tool_use.id, content))
        } else {
            Ok(ToolResultBlock::error(
                &tool_use.id,
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    /// Handle tool result received from extension
    pub async fn handle_tool_result(&self, result: ToolResultPayload) -> Result<()> {
        let call_id = &result.call_id;
        debug!("Received tool result for call_id: {}", call_id);

        let pending = self.pending_calls.write().await.remove(call_id);

        if let Some(pending) = pending {
            let elapsed = pending.started_at.elapsed();
            info!(
                "Tool {} completed in {:?}: success={}",
                pending.tool_name, elapsed, result.success
            );

            // Send result through channel (ignore error if receiver dropped)
            let _ = pending.result_sender.send(result);
        } else {
            warn!("Received result for unknown call_id: {}", call_id);
        }

        Ok(())
    }

    /// Send agent state update to extension
    async fn send_state_update(
        &self,
        session_id: &str,
        state: AgentState,
        current_tool: Option<String>,
    ) -> Result<()> {
        let step_count = *self.step_count.read().await;

        let update = ExtensionMessage::AgentStateUpdate(AgentStatePayload {
            session_id: session_id.to_string(),
            state,
            current_tool,
            step_count,
            max_steps: self.config.max_steps,
            message: None,
        });

        let json = serde_json::to_string(&update).context("Failed to serialize state update")?;
        self.message_sender.send(json).map_err(|_| anyhow::anyhow!("Failed to send state update"))?;

        Ok(())
    }

    /// Send complete state
    pub async fn send_complete(&self, session_id: &str) -> Result<()> {
        self.send_state_update(session_id, AgentState::Complete, None).await
    }

    /// Send error state
    pub async fn send_error(&self, session_id: &str, message: &str) -> Result<()> {
        let step_count = *self.step_count.read().await;

        let update = ExtensionMessage::AgentStateUpdate(AgentStatePayload {
            session_id: session_id.to_string(),
            state: AgentState::Error,
            current_tool: None,
            step_count,
            max_steps: self.config.max_steps,
            message: Some(message.to_string()),
        });

        let json = serde_json::to_string(&update).context("Failed to serialize error state")?;
        self.message_sender.send(json).map_err(|_| anyhow::anyhow!("Failed to send error state"))?;

        Ok(())
    }

    /// Reset step count for new conversation
    pub async fn reset(&self) {
        *self.step_count.write().await = 0;
        self.pending_calls.write().await.clear();
    }

    /// Get current step count
    pub async fn get_step_count(&self) -> u32 {
        *self.step_count.read().await
    }

    /// Check if tool is dangerous
    pub fn is_dangerous_tool(&self, tool_name: &str) -> bool {
        self.config.dangerous_tools.contains(&tool_name.to_string())
    }

    /// Parse tool name to BrowserTool enum
    pub fn parse_tool_name(name: &str) -> Option<BrowserTool> {
        match name {
            "click" => Some(BrowserTool::Click),
            "type" => Some(BrowserTool::Type),
            "scroll" => Some(BrowserTool::Scroll),
            "read_text" => Some(BrowserTool::ReadText),
            "read_page" => Some(BrowserTool::ReadPage),
            "screenshot" => Some(BrowserTool::Screenshot),
            "navigate" => Some(BrowserTool::Navigate),
            "wait" => Some(BrowserTool::Wait),
            "evaluate_js" => Some(BrowserTool::EvaluateJs),
            "fill_form" => Some(BrowserTool::FillForm),
            "get_elements" => Some(BrowserTool::GetElements),
            "highlight" => Some(BrowserTool::Highlight),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_executor_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let executor = ToolExecutor::new(ToolExecutorConfig::default(), tx);

        assert_eq!(executor.get_step_count().await, 0);
    }

    #[test]
    fn test_parse_tool_name() {
        assert_eq!(ToolExecutor::parse_tool_name("click"), Some(BrowserTool::Click));
        assert_eq!(ToolExecutor::parse_tool_name("navigate"), Some(BrowserTool::Navigate));
        assert_eq!(ToolExecutor::parse_tool_name("unknown"), None);
    }

    #[test]
    fn test_dangerous_tools() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let executor = ToolExecutor::new(ToolExecutorConfig::default(), tx);

        assert!(executor.is_dangerous_tool("navigate"));
        assert!(executor.is_dangerous_tool("evaluate_js"));
        assert!(!executor.is_dangerous_tool("click"));
    }
}
