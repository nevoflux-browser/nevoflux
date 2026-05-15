//! HTTP MCP server for ACP bridge mode.
//!
//! Implements MCP Streamable HTTP transport: single POST endpoint serving
//! JSON-RPC requests (initialize, tools/list, tools/call).
//! Claude Agent SDK connects via StreamableHTTPClientTransport.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use nevoflux_llm::providers::acp::mcp_bridge::{McpToolBridge, McpToolDef, ToolCallRequest};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Shared state for the MCP HTTP server.
struct McpState {
    tool_bridge: Arc<McpToolBridge>,
}

/// JSON-RPC request structure.
#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

/// JSON-RPC response structure.
#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Convert a tool definition to MCP JSON format.
fn tool_def_to_json(def: &McpToolDef) -> serde_json::Value {
    serde_json::json!({
        "name": def.name,
        "description": def.description,
        "inputSchema": def.input_schema,
    })
}

fn json_rpc_result(id: serde_json::Value, result: serde_json::Value) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    })
    .into_response()
}

fn json_rpc_error(id: serde_json::Value, code: i32, message: &str) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    })
    .into_response()
}

/// Handle MCP JSON-RPC requests over Streamable HTTP.
async fn handle_mcp_request(
    State(state): State<Arc<McpState>>,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    // Notifications (no id) — accept silently
    let Some(id) = request.id else {
        return StatusCode::ACCEPTED.into_response();
    };

    match request.method.as_str() {
        "initialize" => {
            let client_version = request
                .params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2025-03-26");
            json_rpc_result(
                id,
                serde_json::json!({
                    "protocolVersion": client_version,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "nevoflux-tools",
                        "version": "1.0.0"
                    }
                }),
            )
        }
        "tools/list" => {
            let tools: Vec<serde_json::Value> = state
                .tool_bridge
                .get_tools()
                .iter()
                .map(tool_def_to_json)
                .collect();
            json_rpc_result(id, serde_json::json!({ "tools": tools }))
        }
        "tools/call" => {
            let params = match request.params {
                Some(p) => p,
                None => {
                    return json_rpc_error(id, -32602, "Missing params");
                }
            };
            let name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => {
                    return json_rpc_error(id, -32602, "Missing tool name");
                }
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            tracing::info!(tool = %name, "MCP tools/call received from LLM");

            let tx = match state.tool_bridge.clone_executor() {
                Some(tx) => tx,
                None => {
                    return json_rpc_error(id, -32603, "No active tool executor");
                }
            };

            let (result_tx, result_rx) = oneshot::channel();
            if tx
                .send(ToolCallRequest {
                    name: name.clone(),
                    arguments,
                    result_tx,
                })
                .await
                .is_err()
            {
                return json_rpc_error(id, -32603, "Tool executor unavailable");
            }

            match result_rx.await {
                Ok(Ok(text)) => json_rpc_result(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }),
                ),
                Ok(Err(e)) => json_rpc_result(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": e }],
                        "isError": true
                    }),
                ),
                Err(_) => json_rpc_error(id, -32603, "Tool executor dropped"),
            }
        }
        _ => json_rpc_error(id, -32601, "Method not found"),
    }
}

/// Start the MCP HTTP server on a random available port.
///
/// Returns `(port, join_handle)`. The server runs until the handle is aborted.
pub async fn start_mcp_http_server(
    tool_bridge: Arc<McpToolBridge>,
) -> std::result::Result<(u16, JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(McpState { tool_bridge });
    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "MCP HTTP server error");
        }
    });

    tracing::info!(port, "MCP HTTP server started");
    Ok((port, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a reqwest client that bypasses proxy env vars for localhost tests.
    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[test]
    fn test_tool_def_to_json() {
        let def = McpToolDef {
            name: "browser_navigate".to_string(),
            description: "Navigate to URL".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "url": { "type": "string" } }
            }),
        };
        let json = tool_def_to_json(&def);
        assert_eq!(json["name"], "browser_navigate");
        assert_eq!(json["description"], "Navigate to URL");
        assert!(json["inputSchema"]["properties"]["url"].is_object());
    }

    #[tokio::test]
    async fn test_mcp_server_initialize() {
        let bridge = Arc::new(McpToolBridge::new());
        let (port, handle) = start_mcp_http_server(bridge).await.unwrap();

        let client = test_client();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-03-26", "capabilities": {} }
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(body["result"]["serverInfo"]["name"], "nevoflux-tools");

        handle.abort();
    }

    #[tokio::test]
    async fn test_mcp_server_tools_list() {
        let bridge = Arc::new(McpToolBridge::new());
        bridge.update_tools(vec![McpToolDef {
            name: "test_tool".to_string(),
            description: "A test".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }]);
        let (port, handle) = start_mcp_http_server(bridge).await.unwrap();

        let client = test_client();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list"
            }))
            .send()
            .await
            .unwrap();

        let body: serde_json::Value = resp.json().await.unwrap();
        let tools = body["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");

        handle.abort();
    }

    #[tokio::test]
    async fn test_mcp_server_tools_call() {
        let bridge = Arc::new(McpToolBridge::new());
        let (tool_tx, mut tool_rx) = tokio::sync::mpsc::channel::<ToolCallRequest>(1);
        bridge.set_executor(tool_tx);

        let (port, handle) = start_mcp_http_server(bridge).await.unwrap();

        // Spawn a task to handle the tool call
        tokio::spawn(async move {
            if let Some(req) = tool_rx.recv().await {
                assert_eq!(req.name, "browser_navigate");
                let _ = req.result_tx.send(Ok("navigated".to_string()));
            }
        });

        let client = test_client();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "browser_navigate",
                    "arguments": { "url": "https://example.com" }
                }
            }))
            .send()
            .await
            .unwrap();

        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(body["result"]["content"][0]["text"], "navigated");

        handle.abort();
    }

    #[tokio::test]
    async fn test_mcp_server_notification_returns_202() {
        let bridge = Arc::new(McpToolBridge::new());
        let (port, handle) = start_mcp_http_server(bridge).await.unwrap();

        let client = test_client();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 202);

        handle.abort();
    }

    #[tokio::test]
    async fn test_mcp_server_no_executor() {
        let bridge = Arc::new(McpToolBridge::new());
        // No executor set
        let (port, handle) = start_mcp_http_server(bridge).await.unwrap();

        let client = test_client();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/mcp"))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": { "name": "test", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();

        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No active tool executor"));

        handle.abort();
    }
}
