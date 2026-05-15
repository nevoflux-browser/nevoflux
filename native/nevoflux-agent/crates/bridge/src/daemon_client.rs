//! TCP client for communicating with the daemon.
//!
//! Uses a length-prefixed JSON framing protocol (same as native messaging)
//! over a TCP connection. Supports automatic reconnection when the daemon restarts.

use crate::config::BridgeConfig;
use crate::error::{BridgeError, Result};
use crate::native_messaging::{read_message, write_message};
use crate::port_discovery::{discover_daemon, DaemonInfo};
use futures::stream::Stream;
use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Message from daemon.
pub type DaemonMessage = DaemonEnvelope;

/// Connection state for the daemon client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Connected to daemon.
    Connected,
    /// Reconnecting after disconnection.
    Reconnecting,
}

/// Configuration for reconnection behavior.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of reconnection attempts.
    pub max_retries: u32,
    /// Initial delay between retries.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

/// Client for communicating with the daemon over TCP.
/// Uses 4-byte little-endian length-prefixed JSON framing (same protocol as native messaging).
/// Supports automatic reconnection when the daemon restarts.
pub struct DaemonClient {
    /// The proxy ID for this client.
    proxy_id: String,
    /// TCP reader half (wrapped in BufReader).
    reader: Option<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    /// TCP writer half (wrapped in BufWriter).
    writer: Option<BufWriter<tokio::net::tcp::OwnedWriteHalf>>,
    /// Configuration.
    config: Arc<BridgeConfig>,
    /// Connected daemon info.
    daemon_info: Option<DaemonInfo>,
    /// Current connection state.
    state: ConnectionState,
    /// Reconnection configuration.
    reconnect_config: ReconnectConfig,
}

impl DaemonClient {
    /// Create a new daemon client with the given proxy ID.
    pub fn new(proxy_id: impl Into<String>, config: BridgeConfig) -> Self {
        Self {
            proxy_id: proxy_id.into(),
            reader: None,
            writer: None,
            config: Arc::new(config),
            daemon_info: None,
            state: ConnectionState::Disconnected,
            reconnect_config: ReconnectConfig::default(),
        }
    }

    /// Create a new daemon client with custom reconnection config.
    pub fn with_reconnect_config(
        proxy_id: impl Into<String>,
        config: BridgeConfig,
        reconnect_config: ReconnectConfig,
    ) -> Self {
        Self {
            proxy_id: proxy_id.into(),
            reader: None,
            writer: None,
            config: Arc::new(config),
            daemon_info: None,
            state: ConnectionState::Disconnected,
            reconnect_config,
        }
    }

    /// Get the proxy ID.
    pub fn proxy_id(&self) -> &str {
        &self.proxy_id
    }

    /// Get the configuration.
    pub fn config(&self) -> &BridgeConfig {
        &self.config
    }

    /// Get the connected daemon info.
    pub fn daemon_info(&self) -> Option<&DaemonInfo> {
        self.daemon_info.as_ref()
    }

    /// Get the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.state
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Establish TCP connection and send registration frame.
    async fn establish_connection(&mut self, addr: &str) -> Result<()> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| BridgeError::ConnectionFailed(e.to_string()))?;

        stream.set_nodelay(true).map_err(|e| {
            BridgeError::ConnectionFailed(format!("Failed to set TCP_NODELAY: {}", e))
        })?;

        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        // Send registration frame with proxy_id
        let registration = serde_json::json!({
            "type": "register",
            "proxy_id": self.proxy_id,
        });
        write_message(&mut writer, &registration).await?;

        self.reader = Some(reader);
        self.writer = Some(writer);
        Ok(())
    }

    /// Connect to the daemon.
    ///
    /// Discovers the daemon port and establishes a TCP connection.
    pub async fn connect(&mut self) -> Result<()> {
        // Discover daemon
        let info = discover_daemon(&self.config).await?;

        let addr = format!("127.0.0.1:{}", info.port);
        info!("Connecting to daemon at {}", addr);

        self.establish_connection(&addr).await?;

        self.daemon_info = Some(info);
        self.state = ConnectionState::Connected;

        debug!("Connected to daemon");
        Ok(())
    }

    /// Connect to a specific address (for testing).
    pub async fn connect_to(&mut self, addr: &str) -> Result<()> {
        info!("Connecting to {}", addr);

        // addr may be "tcp://host:port" format; strip the prefix
        let tcp_addr = addr.strip_prefix("tcp://").unwrap_or(addr);

        self.establish_connection(tcp_addr).await?;
        self.state = ConnectionState::Connected;
        Ok(())
    }

    /// Reconnect to the daemon with exponential backoff.
    ///
    /// Drops the old connection and attempts to establish a new TCP connection.
    /// Will retry up to `max_retries` times with exponential backoff.
    pub async fn reconnect(&mut self) -> Result<()> {
        if self.state == ConnectionState::Reconnecting {
            return Err(BridgeError::ConnectionFailed(
                "Already reconnecting".to_string(),
            ));
        }

        self.state = ConnectionState::Reconnecting;
        info!(
            "Attempting to reconnect to daemon (proxy_id={})",
            self.proxy_id
        );

        // Drop old connection
        self.reader = None;
        self.writer = None;

        let mut delay = self.reconnect_config.initial_delay;
        let mut attempts = 0;

        loop {
            attempts += 1;

            // Try to discover and connect
            match discover_daemon(&self.config).await {
                Ok(info) => {
                    let addr = format!("127.0.0.1:{}", info.port);
                    debug!("Reconnect attempt {}: connecting to {}", attempts, addr);

                    match self.establish_connection(&addr).await {
                        Ok(()) => {
                            self.daemon_info = Some(info);
                            self.state = ConnectionState::Connected;
                            info!(
                                "Reconnected to daemon after {} attempts (proxy_id={})",
                                attempts, self.proxy_id
                            );
                            return Ok(());
                        }
                        Err(e) => {
                            warn!("Reconnect attempt {} failed: {}", attempts, e);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Reconnect attempt {} - daemon discovery failed: {}",
                        attempts, e
                    );
                }
            }

            if attempts >= self.reconnect_config.max_retries {
                self.state = ConnectionState::Disconnected;
                error!(
                    "Reconnection failed after {} attempts (proxy_id={})",
                    attempts, self.proxy_id
                );
                return Err(BridgeError::ReconnectionFailed(attempts));
            }

            // Wait before next attempt
            debug!("Waiting {:?} before next reconnect attempt", delay);
            tokio::time::sleep(delay).await;

            // Exponential backoff
            delay = Duration::from_secs_f64(
                (delay.as_secs_f64() * self.reconnect_config.backoff_multiplier)
                    .min(self.reconnect_config.max_delay.as_secs_f64()),
            );
        }
    }

    /// Send a message to the daemon with auto-reconnection.
    pub async fn send(&mut self, envelope: ProxyEnvelope) -> Result<()> {
        let data = serde_json::to_vec(&envelope)?;

        match self.try_send(&data).await {
            Ok(()) => {
                debug!("Sent message to daemon: request_id={}", envelope.request_id);
                Ok(())
            }
            Err(e) => {
                warn!(
                    "Send failed due to disconnection, attempting reconnect: {}",
                    e
                );
                self.state = ConnectionState::Disconnected;

                // Attempt to reconnect
                self.reconnect().await?;

                // Retry send after reconnection
                self.try_send(&data).await?;
                debug!(
                    "Sent message to daemon after reconnect: request_id={}",
                    envelope.request_id
                );
                Ok(())
            }
        }
    }

    /// Try to send raw data over the TCP connection.
    async fn try_send(&mut self, data: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().ok_or(BridgeError::Disconnected)?;

        let len = data.len() as u32;
        writer.write_all(&len.to_le_bytes()).await?;
        writer.write_all(data).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Send a chat message to the daemon.
    pub async fn send_chat(
        &mut self,
        request_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let envelope = ProxyEnvelope::new(&self.proxy_id, request_id, Channel::Chat, payload);
        self.send(envelope).await
    }

    /// Send an MCP message to the daemon.
    pub async fn send_mcp(
        &mut self,
        request_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let envelope = ProxyEnvelope::new(&self.proxy_id, request_id, Channel::Mcp, payload);
        self.send(envelope).await
    }

    /// Receive a message from the daemon with auto-reconnection.
    pub async fn recv(&mut self) -> Result<DaemonEnvelope> {
        match self.try_recv().await {
            Ok(envelope) => {
                debug!(
                    "Received message from daemon: request_id={:?}",
                    envelope.request_id
                );
                Ok(envelope)
            }
            Err(e) => {
                warn!(
                    "Recv failed due to disconnection, attempting reconnect: {}",
                    e
                );
                self.state = ConnectionState::Disconnected;

                // Attempt to reconnect
                self.reconnect().await?;

                // Return disconnected error - caller should retry
                // We don't retry recv automatically because the original message is lost
                Err(BridgeError::Disconnected)
            }
        }
    }

    /// Try to receive a message from the TCP connection.
    async fn try_recv(&mut self) -> Result<DaemonEnvelope> {
        let reader = self.reader.as_mut().ok_or(BridgeError::Disconnected)?;

        let envelope: DaemonEnvelope = read_message(reader).await?;
        Ok(envelope)
    }

    /// Close the connection.
    pub async fn close(&mut self) -> Result<()> {
        debug!("Closing daemon client connection");
        // Shut down writer gracefully
        if let Some(ref mut writer) = self.writer {
            let _ = writer.shutdown().await;
        }
        self.reader = None;
        self.writer = None;
        self.state = ConnectionState::Disconnected;
        self.daemon_info = None;
        Ok(())
    }

    /// Mark connection as disconnected (e.g., when external detection notices disconnection).
    pub fn mark_disconnected(&mut self) {
        if self.state == ConnectionState::Connected {
            warn!("Marking connection as disconnected");
            self.state = ConnectionState::Disconnected;
        }
    }
}

/// A stream of messages from the daemon.
pub struct DaemonMessageStream {
    receiver: mpsc::Receiver<Result<DaemonEnvelope>>,
}

impl DaemonMessageStream {
    /// Create a new message stream from a receiver.
    pub fn new(receiver: mpsc::Receiver<Result<DaemonEnvelope>>) -> Self {
        Self { receiver }
    }
}

impl Stream for DaemonMessageStream {
    type Item = Result<DaemonEnvelope>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_recv(cx)
    }
}

/// Create a unique proxy ID.
pub fn generate_proxy_id() -> String {
    let uuid_str = uuid::Uuid::new_v4().to_string();
    format!("proxy-{}", &uuid_str[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_proxy_id() {
        let id1 = generate_proxy_id();
        let id2 = generate_proxy_id();

        assert!(id1.starts_with("proxy-"));
        assert!(id2.starts_with("proxy-"));
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 14); // "proxy-" + 8 chars
    }

    #[test]
    fn test_daemon_client_new() {
        let config = BridgeConfig::new();
        let client = DaemonClient::new("test-proxy", config);

        assert_eq!(client.proxy_id(), "test-proxy");
        assert!(client.daemon_info().is_none());
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
        assert!(!client.is_connected());
    }

    #[test]
    fn test_daemon_client_with_reconnect_config() {
        let config = BridgeConfig::new();
        let reconnect_config = ReconnectConfig {
            max_retries: 10,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 1.5,
        };
        let client = DaemonClient::with_reconnect_config("test-proxy", config, reconnect_config);

        assert_eq!(client.proxy_id(), "test-proxy");
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    }

    #[test]
    fn test_daemon_client_config() {
        let config = BridgeConfig::new().with_port_range(20000, 20100);
        let client = DaemonClient::new("test-proxy", config);

        assert_eq!(client.config().port_range_start, 20000);
    }

    #[test]
    fn test_reconnect_config_default() {
        let config = ReconnectConfig::default();

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(5));
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_connection_state_debug() {
        assert_eq!(
            format!("{:?}", ConnectionState::Disconnected),
            "Disconnected"
        );
        assert_eq!(format!("{:?}", ConnectionState::Connected), "Connected");
        assert_eq!(
            format!("{:?}", ConnectionState::Reconnecting),
            "Reconnecting"
        );
    }

    #[test]
    fn test_mark_disconnected() {
        let config = BridgeConfig::new();
        let mut client = DaemonClient::new("test-proxy", config);

        // Initially disconnected, mark_disconnected should be no-op
        client.mark_disconnected();
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_daemon_client_connect_no_daemon() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = BridgeConfig::new().with_data_dir(temp.path());
        let mut client = DaemonClient::new("test-proxy", config);

        let result = client.connect().await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
        // State should remain Disconnected on failure
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_daemon_client_reconnect_no_daemon() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = BridgeConfig::new().with_data_dir(temp.path());
        let reconnect_config = ReconnectConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_multiplier: 2.0,
        };
        let mut client =
            DaemonClient::with_reconnect_config("test-proxy", config, reconnect_config);

        let result = client.reconnect().await;
        assert!(matches!(result, Err(BridgeError::ReconnectionFailed(2))));
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_daemon_client_close() {
        let config = BridgeConfig::new();
        let mut client = DaemonClient::new("test-proxy", config);

        client.close().await.unwrap();
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
        assert!(client.daemon_info().is_none());
    }

    #[test]
    fn test_proxy_envelope_creation() {
        let envelope = ProxyEnvelope::new(
            "proxy-001",
            "req-001",
            Channel::Chat,
            serde_json::json!({"type": "chat_message"}),
        );

        assert_eq!(envelope.proxy_id, "proxy-001");
        assert_eq!(envelope.request_id, "req-001");
        assert_eq!(envelope.channel, Channel::Chat);
    }

    #[test]
    fn test_proxy_envelope_serialization() {
        let envelope = ProxyEnvelope::new(
            "proxy-001",
            "req-001",
            Channel::Chat,
            serde_json::json!({"type": "chat_message", "payload": {"text": "hello"}}),
        );

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: ProxyEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.proxy_id, "proxy-001");
        assert_eq!(decoded.request_id, "req-001");
    }

    #[test]
    fn test_daemon_envelope_deserialization() {
        let json = r#"{
            "proxy_id": "proxy-001",
            "request_id": "req-001",
            "channel": "chat",
            "payload": {"type": "stream_chunk"},
            "timestamp_ms": 1706000000000
        }"#;

        let envelope: DaemonEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.proxy_id, "proxy-001");
        assert_eq!(envelope.channel, Channel::Chat);
    }

    #[tokio::test]
    async fn test_daemon_message_stream() {
        let (tx, rx) = mpsc::channel(10);
        let mut stream = DaemonMessageStream::new(rx);

        // Send a message
        let envelope = DaemonEnvelope::new("proxy-001", Channel::Chat, serde_json::json!({}));
        tx.send(Ok(envelope.clone())).await.unwrap();
        drop(tx);

        // Receive via stream
        use futures::StreamExt;
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received.proxy_id, "proxy-001");
    }
}
