//! MCP bridge for stdio communication.
//!
//! This bridge handles `nevoflux-agent --mcp` mode, providing an MCP server
//! over stdio that forwards to the daemon.

use crate::config::BridgeConfig;
use crate::daemon_client::{generate_proxy_id, DaemonClient};
use crate::error::{BridgeError, Result};
use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tracing::{debug, error, info};

/// MCP bridge state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpBridgeState {
    /// Initial state.
    Disconnected,
    /// Connecting to daemon.
    Connecting,
    /// Connected and running.
    Connected,
    /// Shutting down.
    ShuttingDown,
}

/// MCP bridge configuration.
#[derive(Debug, Clone)]
pub struct McpBridgeConfig {
    /// Bridge configuration.
    pub bridge: BridgeConfig,
    /// Agent name for MCP source.
    pub agent_name: String,
}

impl Default for McpBridgeConfig {
    fn default() -> Self {
        Self {
            bridge: BridgeConfig::default(),
            agent_name: "mcp-client".to_string(),
        }
    }
}

impl McpBridgeConfig {
    /// Create a new MCP bridge config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bridge configuration.
    pub fn with_bridge(mut self, bridge: BridgeConfig) -> Self {
        self.bridge = bridge;
        self
    }

    /// Set the agent name.
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = name.into();
        self
    }
}

/// MCP bridge for stdio-based MCP communication.
///
/// This bridges MCP JSON-RPC messages from stdin to the daemon
/// and responses back to stdout.
pub struct McpBridge<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Configuration.
    config: McpBridgeConfig,
    /// Bridge ID (acts as proxy_id for daemon).
    bridge_id: String,
    /// Current state.
    state: McpBridgeState,
    /// Reader for stdio input.
    reader: BufReader<R>,
    /// Writer for stdio output.
    writer: BufWriter<W>,
    /// Daemon client.
    daemon_client: Option<DaemonClient>,
    /// Request ID counter.
    request_counter: u64,
}

impl<R, W> McpBridge<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Create a new MCP bridge.
    pub fn new(reader: R, writer: W, config: McpBridgeConfig) -> Self {
        Self {
            config,
            bridge_id: generate_proxy_id(),
            state: McpBridgeState::Disconnected,
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            daemon_client: None,
            request_counter: 0,
        }
    }

    /// Create an MCP bridge with a specific ID.
    pub fn with_id(
        reader: R,
        writer: W,
        config: McpBridgeConfig,
        bridge_id: impl Into<String>,
    ) -> Self {
        Self {
            config,
            bridge_id: bridge_id.into(),
            state: McpBridgeState::Disconnected,
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            daemon_client: None,
            request_counter: 0,
        }
    }

    /// Get the bridge ID.
    pub fn bridge_id(&self) -> &str {
        &self.bridge_id
    }

    /// Get the current state.
    pub fn state(&self) -> McpBridgeState {
        self.state
    }

    /// Connect to the daemon.
    pub async fn connect(&mut self) -> Result<()> {
        if self.state != McpBridgeState::Disconnected {
            return Ok(());
        }

        self.state = McpBridgeState::Connecting;
        info!("MCP bridge {} connecting to daemon", self.bridge_id);

        let mut client = DaemonClient::new(&self.bridge_id, self.config.bridge.clone());

        match client.connect().await {
            Ok(()) => {
                self.daemon_client = Some(client);
                self.state = McpBridgeState::Connected;
                info!("MCP bridge {} connected to daemon", self.bridge_id);
                Ok(())
            }
            Err(e) => {
                self.state = McpBridgeState::Disconnected;
                error!("MCP bridge {} failed to connect: {}", self.bridge_id, e);
                Err(e)
            }
        }
    }

    /// Read a JSON-RPC message from stdin.
    ///
    /// MCP uses newline-delimited JSON (NDJSON).
    pub async fn read_message(&mut self) -> Result<serde_json::Value> {
        loop {
            let mut line = String::new();
            let bytes_read = self.reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                return Err(BridgeError::ChannelClosed);
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Skip empty lines
                continue;
            }

            return serde_json::from_str(trimmed).map_err(BridgeError::from);
        }
    }

    /// Write a JSON-RPC message to stdout.
    pub async fn write_message(&mut self, message: &serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(message)?;
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Generate a unique request ID for internal tracking.
    fn next_request_id(&mut self) -> String {
        self.request_counter += 1;
        format!("mcp-{}-{}", self.bridge_id, self.request_counter)
    }

    /// Forward an MCP request to the daemon.
    pub async fn forward_to_daemon(&mut self, message: serde_json::Value) -> Result<String> {
        if self.daemon_client.is_none() {
            return Err(BridgeError::DaemonNotRunning);
        }

        let request_id = self.next_request_id();
        let agent_name = self.config.agent_name.clone();
        let bridge_id = self.bridge_id.clone();

        // Wrap in MCP message format
        let mcp_payload = serde_json::json!({
            "type": "mcp_request",
            "payload": {
                "request_id": &request_id,
                "source": {
                    "agent": &agent_name,
                    "session_id": null
                },
                "payload": message
            }
        });

        let envelope = ProxyEnvelope::new(&bridge_id, &request_id, Channel::Mcp, mcp_payload);

        let client = self.daemon_client.as_mut().unwrap();
        client.send(envelope).await?;

        debug!("Forwarded MCP request to daemon: {}", request_id);
        Ok(request_id)
    }

    /// Receive a message from daemon.
    pub async fn receive_from_daemon(&mut self) -> Result<DaemonEnvelope> {
        let client = self
            .daemon_client
            .as_mut()
            .ok_or(BridgeError::DaemonNotRunning)?;

        client.recv().await
    }

    /// Extract the JSON-RPC response from a daemon envelope.
    pub fn extract_mcp_response(envelope: &DaemonEnvelope) -> Option<serde_json::Value> {
        let payload = envelope.payload.as_object()?;
        let msg_type = payload.get("type")?.as_str()?;

        if msg_type == "mcp_response" {
            payload.get("payload")?.as_object()?.get("payload").cloned()
        } else {
            None
        }
    }

    /// Send a JSON-RPC error response.
    pub async fn send_error(
        &mut self,
        id: Option<serde_json::Value>,
        code: i32,
        message: &str,
    ) -> Result<()> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message
            }
        });
        self.write_message(&response).await
    }

    /// Shutdown the bridge.
    pub async fn shutdown(&mut self) -> Result<()> {
        if self.state == McpBridgeState::ShuttingDown {
            return Ok(());
        }

        self.state = McpBridgeState::ShuttingDown;
        info!("MCP bridge {} shutting down", self.bridge_id);

        if let Some(ref mut client) = self.daemon_client {
            client.close().await.ok();
        }

        self.daemon_client = None;
        Ok(())
    }
}

/// JSON-RPC error codes.
pub mod error_codes {
    /// Parse error.
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid request.
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error (reserved range -32000 to -32099).
    pub const SERVER_ERROR: i32 = -32000;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_bridge(input: &str) -> McpBridge<Cursor<Vec<u8>>, Vec<u8>> {
        let reader = Cursor::new(input.as_bytes().to_vec());
        let writer = Vec::new();
        let config = McpBridgeConfig::new();
        McpBridge::new(reader, writer, config)
    }

    #[test]
    fn test_mcp_bridge_config_default() {
        let config = McpBridgeConfig::default();
        assert_eq!(config.agent_name, "mcp-client");
    }

    #[test]
    fn test_mcp_bridge_config_builder() {
        let bridge = BridgeConfig::new().with_port_range(20000, 20100);
        let config = McpBridgeConfig::new()
            .with_bridge(bridge)
            .with_agent_name("claude-code");

        assert_eq!(config.bridge.port_range_start, 20000);
        assert_eq!(config.agent_name, "claude-code");
    }

    #[test]
    fn test_mcp_bridge_new() {
        let bridge = create_test_bridge("");

        assert!(bridge.bridge_id().starts_with("proxy-"));
        assert_eq!(bridge.state(), McpBridgeState::Disconnected);
    }

    #[test]
    fn test_mcp_bridge_with_id() {
        let reader = Cursor::new(vec![]);
        let writer = Vec::new();
        let config = McpBridgeConfig::new();
        let bridge = McpBridge::with_id(reader, writer, config, "mcp-bridge-001");

        assert_eq!(bridge.bridge_id(), "mcp-bridge-001");
    }

    #[test]
    fn test_mcp_bridge_state_debug() {
        assert_eq!(
            format!("{:?}", McpBridgeState::Disconnected),
            "Disconnected"
        );
        assert_eq!(format!("{:?}", McpBridgeState::Connecting), "Connecting");
        assert_eq!(format!("{:?}", McpBridgeState::Connected), "Connected");
        assert_eq!(
            format!("{:?}", McpBridgeState::ShuttingDown),
            "ShuttingDown"
        );
    }

    #[tokio::test]
    async fn test_mcp_bridge_read_message() {
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}
"#;
        let mut bridge = create_test_bridge(input);

        let message = bridge.read_message().await.unwrap();
        assert_eq!(message["jsonrpc"], "2.0");
        assert_eq!(message["id"], 1);
        assert_eq!(message["method"], "initialize");
    }

    #[tokio::test]
    async fn test_mcp_bridge_read_message_skip_empty_lines() {
        let input = r#"

{"jsonrpc":"2.0","id":1,"method":"test"}
"#;
        let mut bridge = create_test_bridge(input);

        let message = bridge.read_message().await.unwrap();
        assert_eq!(message["method"], "test");
    }

    #[tokio::test]
    async fn test_mcp_bridge_read_message_eof() {
        let mut bridge = create_test_bridge("");

        let result = bridge.read_message().await;
        assert!(matches!(result, Err(BridgeError::ChannelClosed)));
    }

    #[tokio::test]
    async fn test_mcp_bridge_read_message_invalid_json() {
        let input = "not valid json\n";
        let mut bridge = create_test_bridge(input);

        let result = bridge.read_message().await;
        assert!(matches!(result, Err(BridgeError::Json(_))));
    }

    #[tokio::test]
    async fn test_mcp_bridge_write_message() {
        let reader = Cursor::new(vec![]);
        let mut output = Vec::new();

        {
            let config = McpBridgeConfig::new();
            let mut bridge = McpBridge::new(reader, &mut output, config);

            let message = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {"success": true}
            });
            bridge.write_message(&message).await.unwrap();
        }

        // Verify output is NDJSON
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.ends_with("\n"));
        let parsed: serde_json::Value = serde_json::from_str(output_str.trim()).unwrap();
        assert_eq!(parsed["result"]["success"], true);
    }

    #[tokio::test]
    async fn test_mcp_bridge_send_error() {
        let reader = Cursor::new(vec![]);
        let mut output = Vec::new();

        {
            let config = McpBridgeConfig::new();
            let mut bridge = McpBridge::new(reader, &mut output, config);

            bridge
                .send_error(
                    Some(serde_json::json!(1)),
                    error_codes::METHOD_NOT_FOUND,
                    "Method not found",
                )
                .await
                .unwrap();
        }

        let output_str = String::from_utf8(output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(output_str.trim()).unwrap();
        assert_eq!(parsed["error"]["code"], error_codes::METHOD_NOT_FOUND);
        assert_eq!(parsed["error"]["message"], "Method not found");
    }

    #[test]
    fn test_extract_mcp_response() {
        let envelope = DaemonEnvelope::new(
            "mcp-bridge-001",
            Channel::Mcp,
            serde_json::json!({
                "type": "mcp_response",
                "payload": {
                    "request_id": "req-001",
                    "payload": {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {"tools": []}
                    }
                }
            }),
        );

        let response =
            McpBridge::<Cursor<Vec<u8>>, Vec<u8>>::extract_mcp_response(&envelope).unwrap();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["result"]["tools"], serde_json::json!([]));
    }

    #[test]
    fn test_extract_mcp_response_not_mcp() {
        let envelope = DaemonEnvelope::new(
            "proxy-001",
            Channel::Chat,
            serde_json::json!({"type": "stream_chunk"}),
        );

        let response = McpBridge::<Cursor<Vec<u8>>, Vec<u8>>::extract_mcp_response(&envelope);
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_mcp_bridge_connect_no_daemon() {
        let temp = tempfile::TempDir::new().unwrap();
        let bridge_config = BridgeConfig::new().with_data_dir(temp.path());
        let config = McpBridgeConfig::new().with_bridge(bridge_config);

        let reader = Cursor::new(vec![]);
        let writer = Vec::new();
        let mut bridge = McpBridge::new(reader, writer, config);

        let result = bridge.connect().await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
        assert_eq!(bridge.state(), McpBridgeState::Disconnected);
    }

    #[tokio::test]
    async fn test_mcp_bridge_forward_not_connected() {
        let mut bridge = create_test_bridge("");

        let result = bridge.forward_to_daemon(serde_json::json!({})).await;
        assert!(matches!(result, Err(BridgeError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_mcp_bridge_receive_not_connected() {
        let mut bridge = create_test_bridge("");

        let result = bridge.receive_from_daemon().await;
        assert!(matches!(result, Err(BridgeError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_mcp_bridge_shutdown() {
        let mut bridge = create_test_bridge("");

        bridge.shutdown().await.unwrap();
        assert_eq!(bridge.state(), McpBridgeState::ShuttingDown);

        // Double shutdown should be ok
        bridge.shutdown().await.unwrap();
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(error_codes::PARSE_ERROR, -32700);
        assert_eq!(error_codes::INVALID_REQUEST, -32600);
        assert_eq!(error_codes::METHOD_NOT_FOUND, -32601);
        assert_eq!(error_codes::INVALID_PARAMS, -32602);
        assert_eq!(error_codes::INTERNAL_ERROR, -32603);
        assert_eq!(error_codes::SERVER_ERROR, -32000);
    }

    #[tokio::test]
    async fn test_mcp_bridge_multiple_messages() {
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
"#;
        let mut bridge = create_test_bridge(input);

        let msg1 = bridge.read_message().await.unwrap();
        assert_eq!(msg1["id"], 1);

        let msg2 = bridge.read_message().await.unwrap();
        assert_eq!(msg2["id"], 2);
    }
}
