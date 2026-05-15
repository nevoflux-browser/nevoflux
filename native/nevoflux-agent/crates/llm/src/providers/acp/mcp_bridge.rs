//! MCP-over-HTTP bridge for native tool calling.
//!
//! Provides the bridge between the MCP HTTP server and NevoFlux tool execution.
//! Used when `AcpProviderConfig::use_mcp_bridge` is true (Claude Code).

use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Tool definition for MCP tools/list.
#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Request sent from MCP tool handler to daemon executor.
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result_tx: oneshot::Sender<Result<String, String>>,
}

/// Artifact data created via MCP tool call, pending delivery to sidebar.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingArtifact {
    pub id: String,
    pub title: String,
    pub content_type: String,
    pub description: Option<String>,
    pub content: String,
    pub files: Option<std::collections::HashMap<String, String>>,
    pub entry: Option<String>,
}

/// Record of a tool call made via MCP, for sidebar display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Permission request sent from ACP permission handler to daemon for sidebar approval.
pub struct PermissionRequest {
    pub tool_name: String,
    pub arguments_summary: String,
    pub result_tx: oneshot::Sender<PermissionResponse>,
}

/// User's response to a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow this one call.
    AllowOnce,
    /// Allow this tool for the rest of the session.
    AllowAlways,
    /// Reject this call.
    Reject,
}

/// Bridge between MCP HTTP server and daemon tool execution.
pub struct McpToolBridge {
    tools: Arc<RwLock<Vec<McpToolDef>>>,
    executor: Arc<Mutex<Option<mpsc::Sender<ToolCallRequest>>>>,
    mcp_server_url: OnceLock<String>,
    server_handle: Mutex<Option<JoinHandle<()>>>,
    /// Artifacts created via MCP tool calls, waiting to be sent to sidebar.
    pending_artifacts: Arc<Mutex<Vec<PendingArtifact>>>,
    /// Log of tool calls made during current request, for sidebar display.
    tool_call_log: Arc<Mutex<Vec<ToolCallRecord>>>,
    /// Channel for forwarding permission requests to sidebar.
    permission_tx: Arc<Mutex<Option<mpsc::Sender<PermissionRequest>>>>,
    /// Tools that user has approved "Always Allow" for this session.
    always_allowed_tools: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl Drop for McpToolBridge {
    fn drop(&mut self) {
        if let Some(handle) = self.server_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}

/// RAII guard that clears the executor slot on drop.
pub struct ToolExecutorGuard {
    bridge: Arc<McpToolBridge>,
}

impl Drop for ToolExecutorGuard {
    fn drop(&mut self) {
        self.bridge.clear_executor_sync();
    }
}

impl McpToolBridge {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(Vec::new())),
            executor: Arc::new(Mutex::new(None)),
            mcp_server_url: OnceLock::new(),
            server_handle: Mutex::new(None),
            pending_artifacts: Arc::new(Mutex::new(Vec::new())),
            tool_call_log: Arc::new(Mutex::new(Vec::new())),
            permission_tx: Arc::new(Mutex::new(None)),
            always_allowed_tools: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    pub fn update_tools(&self, tools: Vec<McpToolDef>) {
        *self.tools.write().unwrap() = tools;
    }

    pub fn get_tools(&self) -> Vec<McpToolDef> {
        self.tools.read().unwrap().clone()
    }

    pub fn set_executor(&self, tx: mpsc::Sender<ToolCallRequest>) {
        *self.executor.lock().unwrap() = Some(tx);
    }

    pub fn clear_executor_sync(&self) {
        *self.executor.lock().unwrap() = None;
    }

    pub fn clone_executor(&self) -> Option<mpsc::Sender<ToolCallRequest>> {
        self.executor.lock().unwrap().clone()
    }

    pub fn executor_guard(self: &Arc<Self>) -> ToolExecutorGuard {
        ToolExecutorGuard {
            bridge: self.clone(),
        }
    }

    /// Set the MCP HTTP server URL (called once on first startup).
    pub fn set_mcp_server_url(&self, url: String) {
        let _ = self.mcp_server_url.set(url);
    }

    /// Get the MCP HTTP server URL, if started.
    pub fn mcp_server_url(&self) -> Option<&str> {
        self.mcp_server_url.get().map(|s| s.as_str())
    }

    /// Store the server task handle for shutdown.
    pub fn set_server_handle(&self, handle: JoinHandle<()>) {
        *self.server_handle.lock().unwrap() = Some(handle);
    }

    /// Add a pending artifact (called by MCP tool executor on create_artifact).
    pub fn push_artifact(&self, artifact: PendingArtifact) {
        self.pending_artifacts.lock().unwrap().push(artifact);
    }

    /// Drain all pending artifacts (called by server after agent response completes).
    pub fn drain_artifacts(&self) -> Vec<PendingArtifact> {
        std::mem::take(&mut *self.pending_artifacts.lock().unwrap())
    }

    /// Record a tool call for sidebar display.
    pub fn log_tool_call(&self, record: ToolCallRecord) {
        self.tool_call_log.lock().unwrap().push(record);
    }

    /// Drain all tool call records (called by server to inject into final response).
    pub fn drain_tool_calls(&self) -> Vec<ToolCallRecord> {
        std::mem::take(&mut *self.tool_call_log.lock().unwrap())
    }

    /// Set the permission handler channel (daemon side connects to sidebar).
    pub fn set_permission_handler(&self, tx: mpsc::Sender<PermissionRequest>) {
        *self.permission_tx.lock().unwrap() = Some(tx);
    }

    /// Check if a tool is in the session-level always-allow list.
    pub fn is_always_allowed(&self, tool_name: &str) -> bool {
        self.always_allowed_tools
            .read()
            .unwrap()
            .contains(tool_name)
    }

    /// Add a tool to the session-level always-allow list.
    pub fn add_always_allowed(&self, tool_name: &str) {
        self.always_allowed_tools
            .write()
            .unwrap()
            .insert(tool_name.to_string());
    }

    /// Check if a tool is low-risk (read-only) and can be auto-approved.
    fn is_low_risk_tool(tool_name: &str) -> bool {
        // Strip MCP prefix if present (e.g. "mcp__nevoflux-tools__browser_get_markdown")
        let name = tool_name.rsplit("__").next().unwrap_or(tool_name);
        // Also handle the raw title format from ACP (e.g. "{\"tab_id\":3}")
        // which means the tool name is in the request title, not parsed
        matches!(
            name,
            // Read-only browser tools
            "browser_get_markdown"
                | "browser_snapshot"
                | "browser_get_tabs"
                | "browser_query_tabs"
                | "browser_get_elements"
                | "browser_get_element"
                | "browser_get_content"
                | "browser_screenshot"
                | "browser_read_artifact"
                | "browser_query_all"
                | "browser_scroll"
                // Wait/utility tools (no side effects)
                | "browser_wait_for"
                | "browser_wait_for_stable"
                | "browser_ask_user"
                // Web fetch (read-only)
                | "web_search"
                | "fetch_page"
                // Memory/knowledge read
                | "memory_search"
                | "memory_view"
                // Agent internal tools
                | "tool_search"
                | "skill_load"
                | "think"
                | "create_plan"
        )
    }

    /// Request permission for a tool call. Returns the user's decision.
    /// Low-risk (read-only) tools are auto-approved.
    /// If the tool is already always-allowed, returns AllowAlways immediately.
    /// Otherwise sends to sidebar via permission_tx channel and waits.
    pub async fn request_permission(
        &self,
        tool_name: &str,
        arguments_summary: &str,
    ) -> PermissionResponse {
        // Auto-approve low-risk read-only tools
        if Self::is_low_risk_tool(tool_name) {
            return PermissionResponse::AllowOnce;
        }

        // Check always-allow list
        if self.is_always_allowed(tool_name) {
            return PermissionResponse::AllowAlways;
        }

        // Try to send to sidebar for user decision
        let tx = self.permission_tx.lock().unwrap().clone();
        let Some(tx) = tx else {
            // No permission handler — reject (no sidebar to ask user)
            tracing::warn!("No permission handler set, rejecting {}", tool_name);
            return PermissionResponse::Reject;
        };

        let (result_tx, result_rx) = oneshot::channel();
        if tx
            .send(PermissionRequest {
                tool_name: tool_name.to_string(),
                arguments_summary: arguments_summary.to_string(),
                result_tx,
            })
            .await
            .is_err()
        {
            tracing::warn!("Permission handler dropped, rejecting {}", tool_name);
            return PermissionResponse::Reject;
        }

        match result_rx.await {
            Ok(response) => {
                if response == PermissionResponse::AllowAlways {
                    self.add_always_allowed(tool_name);
                }
                response
            }
            Err(_) => {
                tracing::warn!(
                    "Permission response channel dropped, rejecting {}",
                    tool_name
                );
                PermissionResponse::Reject
            }
        }
    }
}

/// Convert a tool definition to MCP JSON format.
pub fn tool_def_to_json(def: &McpToolDef) -> serde_json::Value {
    serde_json::json!({
        "name": def.name,
        "description": def.description,
        "inputSchema": def.input_schema,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_and_get_tools() {
        let bridge = McpToolBridge::new();
        assert!(bridge.get_tools().is_empty());

        bridge.update_tools(vec![McpToolDef {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }]);

        let tools = bridge.get_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test_tool");
    }

    #[test]
    fn test_set_and_clear_executor() {
        let bridge = McpToolBridge::new();
        assert!(bridge.clone_executor().is_none());

        let (tx, _rx) = mpsc::channel::<ToolCallRequest>(1);
        bridge.set_executor(tx);
        assert!(bridge.clone_executor().is_some());

        bridge.clear_executor_sync();
        assert!(bridge.clone_executor().is_none());
    }

    #[test]
    fn test_executor_guard_clears_on_drop() {
        let bridge = Arc::new(McpToolBridge::new());
        let (tx, _rx) = mpsc::channel::<ToolCallRequest>(1);
        bridge.set_executor(tx);

        {
            let _guard = bridge.executor_guard();
            assert!(bridge.clone_executor().is_some());
        }
        assert!(bridge.clone_executor().is_none());
    }

    #[tokio::test]
    async fn test_tool_call_through_channel() {
        let bridge = McpToolBridge::new();
        let (tx, mut rx) = mpsc::channel::<ToolCallRequest>(1);
        bridge.set_executor(tx);

        let sender = bridge.clone_executor().unwrap();
        let (result_tx, result_rx) = oneshot::channel();
        sender
            .send(ToolCallRequest {
                name: "test".to_string(),
                arguments: serde_json::json!({"key": "value"}),
                result_tx,
            })
            .await
            .unwrap();

        let req = rx.recv().await.unwrap();
        assert_eq!(req.name, "test");
        let _ = req.result_tx.send(Ok("result".to_string()));

        let result = result_rx.await.unwrap();
        assert_eq!(result, Ok("result".to_string()));
    }

    #[test]
    fn test_mcp_server_url() {
        let bridge = McpToolBridge::new();
        assert!(bridge.mcp_server_url().is_none());

        bridge.set_mcp_server_url("http://127.0.0.1:12345/mcp".to_string());
        assert_eq!(bridge.mcp_server_url(), Some("http://127.0.0.1:12345/mcp"));
    }

    #[test]
    fn test_tool_def_to_json() {
        let def = McpToolDef {
            name: "browser_navigate".to_string(),
            description: "Navigate to URL".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let json = tool_def_to_json(&def);
        assert_eq!(json["name"], "browser_navigate");
        assert_eq!(json["description"], "Navigate to URL");
    }
}
