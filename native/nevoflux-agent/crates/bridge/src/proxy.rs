//! Proxy bridge for Native Messaging.
//!
//! The proxy bridges between the browser extension (via Native Messaging)
//! and the daemon (via TCP).

use crate::config::BridgeConfig;
use crate::daemon_client::{generate_proxy_id, DaemonClient};
use crate::error::{BridgeError, Result};
use crate::native_messaging::{read_message, write_message};
use crate::port_discovery::launch_daemon;
use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use tracing::{debug, error, info, warn};

/// Proxy bridge state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyState {
    /// Initial state.
    Disconnected,
    /// Connecting to daemon.
    Connecting,
    /// Connected and running.
    Connected,
    /// Shutting down.
    ShuttingDown,
}

/// Message to send to sidebar via Native Messaging.
#[derive(Debug)]
pub enum SidebarMessage {
    /// Daemon envelope to forward.
    Daemon(DaemonEnvelope),
    /// Error message.
    Error { code: String, message: String },
}

/// Proxy bridge configuration.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Bridge configuration.
    pub bridge: BridgeConfig,
    /// Maximum pending messages.
    pub max_pending: usize,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bridge: BridgeConfig::default(),
            max_pending: 100,
        }
    }
}

impl ProxyConfig {
    /// Create a new proxy config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bridge configuration.
    pub fn with_bridge(mut self, bridge: BridgeConfig) -> Self {
        self.bridge = bridge;
        self
    }

    /// Set the maximum pending messages.
    pub fn with_max_pending(mut self, max: usize) -> Self {
        self.max_pending = max;
        self
    }
}

/// Proxy bridge for Native Messaging.
pub struct Proxy<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Configuration.
    config: ProxyConfig,
    /// Proxy ID.
    proxy_id: String,
    /// Current state.
    state: ProxyState,
    /// Reader for Native Messaging input.
    reader: BufReader<R>,
    /// Writer for Native Messaging output.
    writer: BufWriter<W>,
    /// Daemon client.
    daemon_client: Option<DaemonClient>,
}

impl<R, W> Proxy<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Create a new proxy.
    pub fn new(reader: R, writer: W, config: ProxyConfig) -> Self {
        Self {
            config,
            proxy_id: generate_proxy_id(),
            state: ProxyState::Disconnected,
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            daemon_client: None,
        }
    }

    /// Create a proxy with a specific ID.
    pub fn with_id(reader: R, writer: W, config: ProxyConfig, proxy_id: impl Into<String>) -> Self {
        Self {
            config,
            proxy_id: proxy_id.into(),
            state: ProxyState::Disconnected,
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            daemon_client: None,
        }
    }

    /// Get the proxy ID.
    pub fn proxy_id(&self) -> &str {
        &self.proxy_id
    }

    /// Get the current state.
    pub fn state(&self) -> ProxyState {
        self.state
    }

    /// Connect to the daemon.
    ///
    /// If the daemon is not running and `auto_launch_daemon` is enabled,
    /// this will attempt to start the daemon first.
    pub async fn connect(&mut self) -> Result<()> {
        if self.state != ProxyState::Disconnected {
            return Ok(());
        }

        self.state = ProxyState::Connecting;
        info!("Proxy {} connecting to daemon", self.proxy_id);

        let mut client = DaemonClient::new(&self.proxy_id, self.config.bridge.clone());

        // First attempt to connect
        match client.connect().await {
            Ok(()) => {
                self.daemon_client = Some(client);
                self.state = ProxyState::Connected;
                info!("Proxy {} connected to daemon", self.proxy_id);
                Ok(())
            }
            Err(e) => {
                debug!("Initial connection failed: {}", e);

                // If auto_launch is enabled, try to start daemon
                if self.config.bridge.auto_launch_daemon {
                    info!("Attempting to auto-launch daemon");

                    // Get current executable path
                    let exe_path = std::env::current_exe().map_err(|e| {
                        BridgeError::DaemonLaunchFailed(format!(
                            "Failed to get executable path: {}",
                            e
                        ))
                    })?;

                    // Launch daemon
                    match launch_daemon(&exe_path, &self.config.bridge).await {
                        Ok(pid) => {
                            info!("Daemon launched with PID {}", pid);

                            // Retry connection
                            let mut retry_client =
                                DaemonClient::new(&self.proxy_id, self.config.bridge.clone());

                            match retry_client.connect().await {
                                Ok(()) => {
                                    self.daemon_client = Some(retry_client);
                                    self.state = ProxyState::Connected;
                                    info!(
                                        "Proxy {} connected to daemon after auto-launch",
                                        self.proxy_id
                                    );
                                    return Ok(());
                                }
                                Err(e) => {
                                    self.state = ProxyState::Disconnected;
                                    error!("Failed to connect after daemon launch: {}", e);
                                    return Err(e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to auto-launch daemon: {}", e);
                            // Fall through to return original error
                        }
                    }
                }

                self.state = ProxyState::Disconnected;
                error!("Proxy {} failed to connect: {}", self.proxy_id, e);
                Err(e)
            }
        }
    }

    /// Read a message from Native Messaging.
    pub async fn read_native_message(&mut self) -> Result<serde_json::Value> {
        read_message(&mut self.reader).await
    }

    /// Write a message to Native Messaging.
    pub async fn write_native_message(&mut self, message: &serde_json::Value) -> Result<()> {
        write_message(&mut self.writer, message).await
    }

    /// Forward a message from sidebar to daemon.
    pub async fn forward_to_daemon(
        &mut self,
        request_id: impl Into<String>,
        channel: Channel,
        payload: serde_json::Value,
    ) -> Result<()> {
        let client = self
            .daemon_client
            .as_mut()
            .ok_or(BridgeError::DaemonNotRunning)?;

        let envelope = ProxyEnvelope::new(&self.proxy_id, request_id, channel, payload);
        client.send(envelope).await
    }

    /// Receive a message from daemon.
    pub async fn receive_from_daemon(&mut self) -> Result<DaemonEnvelope> {
        let client = self
            .daemon_client
            .as_mut()
            .ok_or(BridgeError::DaemonNotRunning)?;

        client.recv().await
    }

    /// Forward a daemon message to sidebar.
    pub async fn forward_to_sidebar(&mut self, envelope: DaemonEnvelope) -> Result<()> {
        // Convert to JSON value for native messaging
        let value = serde_json::to_value(&envelope.payload)?;
        self.write_native_message(&value).await
    }

    /// Send an error to sidebar.
    pub async fn send_error(&mut self, code: &str, message: &str) -> Result<()> {
        let error = serde_json::json!({
            "type": "error",
            "payload": {
                "code": code,
                "message": message
            }
        });
        self.write_native_message(&error).await
    }

    /// Shutdown the proxy.
    pub async fn shutdown(&mut self) -> Result<()> {
        if self.state == ProxyState::ShuttingDown {
            return Ok(());
        }

        self.state = ProxyState::ShuttingDown;
        info!("Proxy {} shutting down", self.proxy_id);

        if let Some(ref mut client) = self.daemon_client {
            client.close().await.ok();
        }

        self.daemon_client = None;
        Ok(())
    }
}

/// Extract the channel and request_id from a native message.
pub fn parse_native_message(
    message: &serde_json::Value,
) -> Option<(String, Channel, serde_json::Value)> {
    let obj = message.as_object()?;

    // Get message type
    let msg_type = obj.get("type")?.as_str()?;

    // Generate request ID if not present
    let request_id = obj
        .get("request_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Determine channel based on message type
    let channel = if msg_type.starts_with("mcp_") {
        Channel::Mcp
    } else {
        Channel::Chat
    };

    Some((request_id, channel, message.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_proxy(input: Vec<u8>) -> Proxy<Cursor<Vec<u8>>, Vec<u8>> {
        let reader = Cursor::new(input);
        let writer = Vec::new();
        let config = ProxyConfig::new();
        Proxy::new(reader, writer, config)
    }

    #[test]
    fn test_proxy_config_default() {
        let config = ProxyConfig::default();
        assert_eq!(config.max_pending, 100);
    }

    #[test]
    fn test_proxy_config_builder() {
        let bridge = BridgeConfig::new().with_port_range(20000, 20100);
        let config = ProxyConfig::new().with_bridge(bridge).with_max_pending(200);

        assert_eq!(config.bridge.port_range_start, 20000);
        assert_eq!(config.max_pending, 200);
    }

    #[test]
    fn test_proxy_new() {
        let proxy = create_test_proxy(vec![]);

        assert!(proxy.proxy_id().starts_with("proxy-"));
        assert_eq!(proxy.state(), ProxyState::Disconnected);
    }

    #[test]
    fn test_proxy_with_id() {
        let reader = Cursor::new(vec![]);
        let writer = Vec::new();
        let config = ProxyConfig::new();
        let proxy = Proxy::with_id(reader, writer, config, "custom-proxy-001");

        assert_eq!(proxy.proxy_id(), "custom-proxy-001");
    }

    #[test]
    fn test_proxy_state_debug() {
        assert_eq!(format!("{:?}", ProxyState::Disconnected), "Disconnected");
        assert_eq!(format!("{:?}", ProxyState::Connecting), "Connecting");
        assert_eq!(format!("{:?}", ProxyState::Connected), "Connected");
        assert_eq!(format!("{:?}", ProxyState::ShuttingDown), "ShuttingDown");
    }

    #[test]
    fn test_parse_native_message_chat() {
        let message = serde_json::json!({
            "type": "chat_message",
            "payload": {
                "session_id": "sess-001",
                "text": "Hello"
            }
        });

        let (request_id, channel, payload) = parse_native_message(&message).unwrap();
        assert!(!request_id.is_empty());
        assert_eq!(channel, Channel::Chat);
        assert_eq!(payload["type"], "chat_message");
    }

    #[test]
    fn test_parse_native_message_mcp() {
        let message = serde_json::json!({
            "type": "mcp_request",
            "request_id": "req-001",
            "payload": {}
        });

        let (request_id, channel, _) = parse_native_message(&message).unwrap();
        assert_eq!(request_id, "req-001");
        assert_eq!(channel, Channel::Mcp);
    }

    #[test]
    fn test_parse_native_message_with_request_id() {
        let message = serde_json::json!({
            "type": "chat_message",
            "request_id": "custom-req-001"
        });

        let (request_id, _, _) = parse_native_message(&message).unwrap();
        assert_eq!(request_id, "custom-req-001");
    }

    #[test]
    fn test_parse_native_message_invalid() {
        // Not an object
        assert!(parse_native_message(&serde_json::json!("string")).is_none());

        // No type field
        assert!(parse_native_message(&serde_json::json!({"payload": {}})).is_none());

        // Type not a string
        assert!(parse_native_message(&serde_json::json!({"type": 123})).is_none());
    }

    #[tokio::test]
    async fn test_proxy_connect_no_daemon() {
        let temp = tempfile::TempDir::new().unwrap();
        let bridge = BridgeConfig::new().with_data_dir(temp.path());
        let config = ProxyConfig::new().with_bridge(bridge);

        let reader = Cursor::new(vec![]);
        let writer = Vec::new();
        let mut proxy = Proxy::new(reader, writer, config);

        let result = proxy.connect().await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
        assert_eq!(proxy.state(), ProxyState::Disconnected);
    }

    #[tokio::test]
    async fn test_proxy_forward_to_daemon_not_connected() {
        let mut proxy = create_test_proxy(vec![]);

        let result = proxy
            .forward_to_daemon("req-001", Channel::Chat, serde_json::json!({}))
            .await;
        assert!(matches!(result, Err(BridgeError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_proxy_receive_from_daemon_not_connected() {
        let mut proxy = create_test_proxy(vec![]);

        let result = proxy.receive_from_daemon().await;
        assert!(matches!(result, Err(BridgeError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_proxy_read_write_native_message() {
        use crate::native_messaging::encode_message;

        // Prepare input message
        let input_msg = serde_json::json!({"type": "test", "value": 42});
        let encoded = encode_message(&input_msg).unwrap();

        let reader = Cursor::new(encoded);
        let writer = Vec::new();
        let config = ProxyConfig::new();
        let mut proxy = Proxy::new(reader, writer, config);

        // Read message
        let received: serde_json::Value = proxy.read_native_message().await.unwrap();
        assert_eq!(received["type"], "test");
        assert_eq!(received["value"], 42);
    }

    #[tokio::test]
    async fn test_proxy_write_native_message() {
        let reader = Cursor::new(vec![]);
        let mut output = Vec::new();

        {
            let config = ProxyConfig::new();
            let mut proxy = Proxy::new(reader, &mut output, config);

            let msg = serde_json::json!({"response": "ok"});
            proxy.write_native_message(&msg).await.unwrap();
        }

        // Verify output
        assert!(!output.is_empty());
        let len = u32::from_le_bytes([output[0], output[1], output[2], output[3]]);
        assert!(len > 0);
    }

    #[tokio::test]
    async fn test_proxy_send_error() {
        let reader = Cursor::new(vec![]);
        let mut output = Vec::new();

        {
            let config = ProxyConfig::new();
            let mut proxy = Proxy::new(reader, &mut output, config);

            proxy
                .send_error("TEST_ERROR", "Test error message")
                .await
                .unwrap();
        }

        // Verify output contains error
        let len = u32::from_le_bytes([output[0], output[1], output[2], output[3]]);
        let json: serde_json::Value = serde_json::from_slice(&output[4..4 + len as usize]).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["payload"]["code"], "TEST_ERROR");
    }

    #[tokio::test]
    async fn test_proxy_shutdown() {
        let mut proxy = create_test_proxy(vec![]);

        proxy.shutdown().await.unwrap();
        assert_eq!(proxy.state(), ProxyState::ShuttingDown);

        // Double shutdown should be ok
        proxy.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_proxy_forward_to_sidebar() {
        let reader = Cursor::new(vec![]);
        let mut output = Vec::new();

        {
            let config = ProxyConfig::new();
            let mut proxy = Proxy::new(reader, &mut output, config);

            let envelope = DaemonEnvelope::new(
                "proxy-001",
                Channel::Chat,
                serde_json::json!({"type": "stream_chunk", "data": "hello"}),
            );

            proxy.forward_to_sidebar(envelope).await.unwrap();
        }

        // Verify output
        assert!(!output.is_empty());
    }
}
