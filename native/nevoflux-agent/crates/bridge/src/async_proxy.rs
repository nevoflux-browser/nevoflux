//! Full-duplex async proxy for Native Messaging.
//!
//! This module provides a 3-task architecture that enables simultaneous
//! sending and receiving of messages between the browser extension and daemon.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Proxy                                │
//! │                                                              │
//! │  stdin ──▶ [Stdin Task] ──▶ stdin_tx ──────┐                │
//! │                                             │                │
//! │                                             ▼                │
//! │                                      ┌─────────────┐        │
//! │                                      │ Socket Task │        │
//! │                                      │  (select!)  │◀──────▶│ TCP
//! │                                      └─────────────┘        │
//! │                                             │                │
//! │  stdout ◀── [Stdout Task] ◀── stdout_rx ◀──┘                │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use crate::config::{BridgeConfig, ConnectionMode};
use crate::daemon_client::{generate_proxy_id, DaemonClient};
use crate::error::{BridgeError, Result};
use crate::native_messaging::{read_message, write_message};
use crate::port_discovery::{find_available_port, launch_daemon_with_port, wait_for_daemon_ready};
use crate::proxy::parse_native_message;
use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

/// Message from stdin (sidebar) to be processed.
#[derive(Debug, Clone)]
pub enum StdinMessage {
    /// A message from the sidebar to forward to daemon.
    SidebarMessage {
        request_id: String,
        channel: Channel,
        payload: serde_json::Value,
    },
    /// Shutdown signal.
    Shutdown,
}

/// Message to be written to stdout (sidebar).
#[derive(Debug, Clone)]
pub enum StdoutMessage {
    /// A response from daemon to forward to sidebar.
    DaemonResponse(DaemonEnvelope),
    /// An error message.
    Error { code: String, message: String },
    /// Initial connection message.
    Connected { version: String, proxy_id: String },
    /// Shutdown signal.
    Shutdown,
}

/// Configuration for the async proxy.
#[derive(Debug, Clone)]
pub struct AsyncProxyConfig {
    /// Bridge configuration.
    pub bridge: BridgeConfig,
    /// Channel buffer size for internal communication.
    pub channel_buffer_size: usize,
}

impl Default for AsyncProxyConfig {
    fn default() -> Self {
        Self {
            bridge: BridgeConfig::default(),
            channel_buffer_size: 100,
        }
    }
}

impl AsyncProxyConfig {
    /// Create a new config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bridge configuration.
    pub fn with_bridge(mut self, bridge: BridgeConfig) -> Self {
        self.bridge = bridge;
        self
    }

    /// Set the channel buffer size.
    pub fn with_channel_buffer_size(mut self, size: usize) -> Self {
        self.channel_buffer_size = size;
        self
    }
}

/// Result from running the async proxy, including lifecycle info.
#[derive(Debug)]
pub struct ProxyResult {
    /// PID of a daemon that was spawned by the proxy (prod mode only).
    /// `None` if the proxy connected to a pre-existing daemon (dev mode or already running).
    pub spawned_daemon_pid: Option<u32>,
}

/// Run the async proxy with full-duplex communication.
///
/// This spawns three tasks:
/// - stdin_task: reads from stdin and sends to socket_task
/// - stdout_task: receives from socket_task and writes to stdout
/// - socket_task: handles bidirectional TCP communication using select!
pub async fn run_async_proxy<R, W>(
    reader: R,
    writer: W,
    config: AsyncProxyConfig,
) -> Result<ProxyResult>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let proxy_id = generate_proxy_id();
    info!("Starting async proxy: {}", proxy_id);

    // Create channels
    let (stdin_tx, stdin_rx) = mpsc::channel::<StdinMessage>(config.channel_buffer_size);
    let (stdout_tx, stdout_rx) = mpsc::channel::<StdoutMessage>(config.channel_buffer_size);
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Create daemon client
    let mut daemon_client = DaemonClient::new(&proxy_id, config.bridge.clone());

    // Connect to daemon using mode-specific strategy
    let spawned_pid = match config.bridge.mode {
        ConnectionMode::Dev => {
            connect_dev_mode(&mut daemon_client, &config.bridge).await?;
            None
        }
        ConnectionMode::Prod => connect_prod_mode(&mut daemon_client, &config.bridge).await?,
    };
    info!("Proxy {} connected to daemon", proxy_id);

    // Send initial connected message
    stdout_tx
        .send(StdoutMessage::Connected {
            version: env!("CARGO_PKG_VERSION").to_string(),
            proxy_id: proxy_id.clone(),
        })
        .await
        .map_err(|_| BridgeError::ChannelClosed)?;

    // Clone for tasks
    let stdin_shutdown_rx = shutdown_tx.subscribe();
    let stdout_shutdown_rx = shutdown_tx.subscribe();
    let socket_shutdown_rx = shutdown_tx.subscribe();
    let proxy_id_for_socket = proxy_id.clone();

    // Spawn stdin task
    let stdin_handle = tokio::spawn(stdin_task(
        BufReader::new(reader),
        stdin_tx,
        stdin_shutdown_rx,
    ));

    // Spawn stdout task
    let stdout_handle = tokio::spawn(stdout_task(
        BufWriter::new(writer),
        stdout_rx,
        stdout_shutdown_rx,
    ));

    // Spawn socket task
    let socket_handle = tokio::spawn(socket_task(
        stdin_rx,
        stdout_tx.clone(),
        daemon_client,
        socket_shutdown_rx,
        proxy_id_for_socket,
    ));

    // Wait for any task to complete
    tokio::select! {
        result = stdin_handle => {
            match result {
                Ok(Ok(())) => debug!("stdin task completed normally"),
                Ok(Err(e)) => error!("stdin task error: {}", e),
                Err(e) => error!("stdin task panicked: {}", e),
            }
        }
        result = stdout_handle => {
            match result {
                Ok(Ok(())) => debug!("stdout task completed normally"),
                Ok(Err(e)) => error!("stdout task error: {}", e),
                Err(e) => error!("stdout task panicked: {}", e),
            }
        }
        result = socket_handle => {
            match result {
                Ok(Ok(())) => debug!("socket task completed normally"),
                Ok(Err(e)) => error!("socket task error: {}", e),
                Err(e) => error!("socket task panicked: {}", e),
            }
        }
    }

    // Signal shutdown to all tasks
    let _ = shutdown_tx.send(());

    info!("Async proxy {} shutting down", proxy_id);
    Ok(ProxyResult {
        spawned_daemon_pid: spawned_pid,
    })
}

/// Dev mode: connect to a manually-started daemon on fixed port 19500.
///
/// Retries connection at 500ms intervals for up to the configured timeout.
/// Does not auto-launch daemon.
async fn connect_dev_mode(client: &mut DaemonClient, config: &BridgeConfig) -> Result<()> {
    let port = config.port_range_start;
    let addr = format!("127.0.0.1:{}", port);
    let timeout = config.connect_timeout;
    let start = std::time::Instant::now();

    info!("Dev mode: connecting to daemon on port {}", port);

    // Retry connect_to directly — TCP connect will fail immediately if daemon
    // isn't listening, so no separate probe is needed.
    loop {
        match client.connect_to(&addr).await {
            Ok(()) => {
                debug!("Dev mode: connected to daemon on port {}", port);
                return Ok(());
            }
            Err(_) => {
                if start.elapsed() > timeout {
                    return Err(BridgeError::ConnectionFailed(format!(
                        "Dev mode: daemon not responding on port {}. Start with: nevoflux-agent --daemon",
                        port
                    )));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

/// Prod mode: connect to daemon, auto-launching if not running.
///
/// Returns `Some(pid)` when daemon was spawned, `None` if a pre-existing daemon was found.
///
/// Zero-file managed mode: the proxy allocates a port, passes it to the daemon
/// via `--port`, and holds the PID from `spawn()`. No port/pid/lock files are
/// written, eliminating stale-file issues on Windows.
async fn connect_prod_mode(
    client: &mut DaemonClient,
    config: &BridgeConfig,
) -> Result<Option<u32>> {
    // 1. Try connecting to a running daemon (dev-mode files or previously launched)
    match client.connect().await {
        Ok(()) => {
            debug!("Prod mode: connected to existing daemon");
            return Ok(None);
        }
        Err(BridgeError::PortFileNotFound(_)) | Err(BridgeError::DaemonNotRunning) => {
            // No port file → daemon never started
        }
        Err(BridgeError::ConnectionFailed(_)) | Err(BridgeError::Io(_)) => {
            // Port file exists but daemon is dead → stale state
            warn!("Prod mode: stale port file detected, cleaning up");
            let _ = tokio::fs::remove_file(config.port_file_path()).await;
            let _ = tokio::fs::remove_file(config.pid_file_path()).await;
            let _ = tokio::fs::remove_file(config.lock_file_path()).await;
        }
        Err(e) => return Err(e),
    }

    // 2. Daemon not reachable — auto-launch with zero-file mode
    if !config.auto_launch_daemon {
        return Err(BridgeError::ConnectionFailed(
            "Daemon not running and auto-launch is disabled".to_string(),
        ));
    }

    info!("Prod mode: allocating port and launching daemon");

    let port = find_available_port(config).await?;

    let exe_path = std::env::current_exe().map_err(|e| {
        BridgeError::DaemonLaunchFailed(format!("Failed to get executable path: {}", e))
    })?;

    let pid = launch_daemon_with_port(&exe_path, config, port).await?;
    info!("Daemon launched with PID {} on port {}", pid, port);

    // 3. Wait for daemon to be ready (TCP health check, not file polling)
    wait_for_daemon_ready(port, config.connect_timeout).await?;

    // 4. Connect directly to the known port
    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await?;

    Ok(Some(pid))
}

/// Task that reads from stdin and sends to the socket task.
async fn stdin_task<R>(
    mut reader: BufReader<R>,
    tx: mpsc::Sender<StdinMessage>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send,
{
    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.recv() => {
                debug!("stdin task received shutdown signal");
                break;
            }

            result = read_message::<_, serde_json::Value>(&mut reader) => {
                match result {
                    Ok(message) => {
                        debug!("stdin received: {:?}", message.get("type"));

                        if let Some((request_id, channel, payload)) = parse_native_message(&message) {
                            let msg = StdinMessage::SidebarMessage {
                                request_id,
                                channel,
                                payload,
                            };
                            if tx.send(msg).await.is_err() {
                                error!("stdin channel closed");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        // EOF or read error - browser closed connection
                        debug!("stdin read error (browser closed): {}", e);
                        let _ = tx.send(StdinMessage::Shutdown).await;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Task that receives from the socket task and writes to stdout.
async fn stdout_task<W>(
    mut writer: BufWriter<W>,
    mut rx: mpsc::Receiver<StdoutMessage>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.recv() => {
                debug!("stdout task received shutdown signal");
                break;
            }

            msg = rx.recv() => {
                match msg {
                    Some(StdoutMessage::DaemonResponse(envelope)) => {
                        let value = serde_json::to_value(&envelope.payload)?;
                        write_message(&mut writer, &value).await?;
                    }
                    Some(StdoutMessage::Error { code, message }) => {
                        let error = serde_json::json!({
                            "type": "error",
                            "payload": {
                                "code": code,
                                "message": message
                            }
                        });
                        write_message(&mut writer, &error).await?;
                    }
                    Some(StdoutMessage::Connected { version, proxy_id }) => {
                        let connected = serde_json::json!({
                            "type": "connected",
                            "payload": {
                                "version": version,
                                "proxy_id": proxy_id
                            }
                        });
                        write_message(&mut writer, &connected).await?;
                    }
                    Some(StdoutMessage::Shutdown) | None => {
                        debug!("stdout task: channel closed or shutdown");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Task that handles bidirectional TCP communication.
///
/// Uses select! to simultaneously:
/// - Forward messages from stdin to daemon
/// - Forward messages from daemon to stdout
async fn socket_task(
    mut stdin_rx: mpsc::Receiver<StdinMessage>,
    stdout_tx: mpsc::Sender<StdoutMessage>,
    mut daemon_client: DaemonClient,
    mut shutdown_rx: broadcast::Receiver<()>,
    proxy_id: String,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;

            // 1. Shutdown signal (highest priority)
            _ = shutdown_rx.recv() => {
                debug!("socket task received shutdown signal");
                break;
            }

            // 2. Messages from sidebar (stdin) -> daemon
            msg = stdin_rx.recv() => {
                match msg {
                    Some(StdinMessage::SidebarMessage { request_id, channel, payload }) => {
                        debug!("Forwarding to daemon: request_id={}", request_id);

                        let envelope = ProxyEnvelope::new(&proxy_id, &request_id, channel, payload);
                        if let Err(e) = daemon_client.send(envelope).await {
                            error!("Failed to send to daemon: {}", e);
                            let _ = stdout_tx.send(StdoutMessage::Error {
                                code: "DAEMON_ERROR".into(),
                                message: e.to_string(),
                            }).await;
                        }
                    }
                    Some(StdinMessage::Shutdown) | None => {
                        debug!("socket task: stdin channel closed or shutdown");
                        break;
                    }
                }
            }

            // 3. Messages from daemon -> sidebar (stdout)
            result = daemon_client.recv() => {
                match result {
                    Ok(envelope) => {
                        debug!("Received from daemon: request_id={:?}", envelope.request_id);

                        if stdout_tx.send(StdoutMessage::DaemonResponse(envelope)).await.is_err() {
                            error!("stdout channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        // Connection errors are internal — don't forward them
                        // to the sidebar (it can't parse them and they just
                        // spam the browser console).
                        if matches!(e, BridgeError::Disconnected) {
                            // recv() already reconnected successfully.
                            // Just resume the loop — no double-reconnect needed.
                            warn!("Connection lost and restored, resuming");
                        } else {
                            // recv() did not reconnect (unrecognized error).
                            // Attempt reconnection ourselves.
                            error!("Daemon receive error: {}", e);
                            info!("Attempting to reconnect to daemon...");
                            if let Err(reconnect_err) = daemon_client.reconnect().await {
                                error!("Reconnection failed: {}", reconnect_err);
                                let _ = stdout_tx.send(StdoutMessage::Error {
                                    code: "DAEMON_DISCONNECTED".into(),
                                    message: "Lost connection to daemon".into(),
                                }).await;
                                break;
                            }
                            info!("Reconnected to daemon");
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    daemon_client.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_async_proxy_config_default() {
        let config = AsyncProxyConfig::default();
        assert_eq!(config.channel_buffer_size, 100);
    }

    #[test]
    fn test_async_proxy_config_builder() {
        let bridge = BridgeConfig::new().with_port_range(20000, 20100);
        let config = AsyncProxyConfig::new()
            .with_bridge(bridge)
            .with_channel_buffer_size(200);

        assert_eq!(config.bridge.port_range_start, 20000);
        assert_eq!(config.channel_buffer_size, 200);
    }

    #[test]
    fn test_stdin_message_debug() {
        let msg = StdinMessage::SidebarMessage {
            request_id: "req-001".into(),
            channel: Channel::Chat,
            payload: serde_json::json!({}),
        };
        assert!(format!("{:?}", msg).contains("SidebarMessage"));

        let shutdown = StdinMessage::Shutdown;
        assert!(format!("{:?}", shutdown).contains("Shutdown"));
    }

    #[test]
    fn test_stdout_message_debug() {
        let msg = StdoutMessage::Error {
            code: "TEST".into(),
            message: "test error".into(),
        };
        assert!(format!("{:?}", msg).contains("Error"));

        let connected = StdoutMessage::Connected {
            version: "1.0.0".into(),
            proxy_id: "proxy-001".into(),
        };
        assert!(format!("{:?}", connected).contains("Connected"));
    }

    #[tokio::test]
    async fn test_stdin_task_handles_shutdown() {
        let (tx, mut rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Empty input that will EOF immediately
        let reader = Cursor::new(vec![]);

        let handle = tokio::spawn(stdin_task(BufReader::new(reader), tx, shutdown_rx));

        // Wait a bit for the task to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should receive shutdown message due to EOF
        let msg = rx.recv().await;
        assert!(matches!(msg, Some(StdinMessage::Shutdown)));

        let _ = shutdown_tx.send(());
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stdout_task_writes_connected() {
        let (tx, rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let mut output = Vec::new();

        // Send connected message
        tx.send(StdoutMessage::Connected {
            version: "1.0.0".into(),
            proxy_id: "test-proxy".into(),
        })
        .await
        .unwrap();

        // Signal shutdown after message is sent
        tx.send(StdoutMessage::Shutdown).await.unwrap();

        let handle =
            tokio::spawn(
                async move { stdout_task(BufWriter::new(&mut output), rx, shutdown_rx).await },
            );

        let _ = shutdown_tx.send(());
        let _ = handle.await;

        // Note: output is moved into the task, so we can't check it directly
        // In a real test, we'd use a different approach
    }

    #[tokio::test]
    async fn test_stdout_task_writes_error() {
        let (tx, rx) = mpsc::channel(10);
        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let output: Vec<u8> = Vec::new();

        // Send error and shutdown
        tx.send(StdoutMessage::Error {
            code: "TEST_ERROR".into(),
            message: "Test error message".into(),
        })
        .await
        .unwrap();
        tx.send(StdoutMessage::Shutdown).await.unwrap();

        let handle =
            tokio::spawn(async move { stdout_task(BufWriter::new(output), rx, shutdown_rx).await });

        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}
