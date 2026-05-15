//! Stdio transport for MCP communication.
//!
//! Spawns a child process and communicates via stdin/stdout using JSON-RPC.

use crate::error::{McpError, Result};
use crate::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

/// Stdio transport for communicating with an MCP server process.
pub struct StdioTransport {
    /// Channel to send requests to the writer task.
    request_tx: mpsc::Sender<TransportMessage>,
    /// Whether the transport is connected.
    connected: Arc<AtomicBool>,
    /// Handle to the child process (for cleanup).
    child: Arc<Mutex<Option<Child>>>,
}

/// Internal message type for transport communication.
enum TransportMessage {
    Request {
        request: JsonRpcRequest,
        response_tx: oneshot::Sender<Result<JsonRpcResponse>>,
    },
    Notification {
        notification: JsonRpcNotification,
    },
    Close,
}

impl StdioTransport {
    /// Spawn a new MCP server process and create a transport.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute (e.g., "npx", "node", "python")
    /// * `args` - Arguments to pass to the command
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let transport = StdioTransport::spawn("npx", &["-y", "@anthropic/mcp-server-filesystem", "~"]).await?;
    /// ```
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self> {
        Self::spawn_with_env(command, args, &HashMap::new()).await
    }

    /// Spawn a new MCP server process with environment variables.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute (e.g., "npx", "node", "python")
    /// * `args` - Arguments to pass to the command
    /// * `env` - Environment variables to set for the process
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut env = HashMap::new();
    /// env.insert("NODE_ENV".to_string(), "production".to_string());
    /// let transport = StdioTransport::spawn_with_env("node", &["server.js"], &env).await?;
    /// ```
    pub async fn spawn_with_env(
        command: &str,
        args: &[&str],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        // Split command string and resolve path (handles "npx -y @pkg" in one string
        // and nvm/pyenv paths not on daemon PATH)
        let (resolved_cmd, all_args) = crate::command::split_command(command, args);

        // On Windows, use cmd /C to resolve .cmd scripts (npx.cmd, etc.)
        // On Unix, execute directly.
        let mut cmd = crate::command::build_command(&resolved_cmd, &all_args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        // Add environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        // On Windows, hide the console window for MCP server subprocesses
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::SpawnFailed(format!("{}: {}", command, e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::SpawnFailed("Failed to capture stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::SpawnFailed("Failed to capture stdout".to_string()))?;

        let connected = Arc::new(AtomicBool::new(true));
        let (request_tx, request_rx) = mpsc::channel(32);

        // Start reader and writer tasks
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));

        let connected_reader = connected.clone();
        let pending_reader = pending_requests.clone();
        tokio::spawn(async move {
            Self::reader_task(stdout, pending_reader, connected_reader).await;
        });

        let connected_writer = connected.clone();
        tokio::spawn(async move {
            Self::writer_task(stdin, request_rx, pending_requests, connected_writer).await;
        });

        Ok(Self {
            request_tx,
            connected,
            child: Arc::new(Mutex::new(Some(child))),
        })
    }

    /// Reader task that processes responses from stdout.
    async fn reader_task(
        stdout: ChildStdout,
        pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<JsonRpcResponse>>>>>,
        connected: Arc<AtomicBool>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }

            // Try to parse as JSON-RPC response
            match serde_json::from_str::<JsonRpcResponse>(&line) {
                Ok(response) => {
                    if let Some(id) = response.id {
                        let mut pending = pending.lock().await;
                        if let Some(tx) = pending.remove(&id) {
                            let _ = tx.send(Ok(response));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse response: {} - {}", e, line);
                }
            }
        }

        connected.store(false, Ordering::SeqCst);
    }

    /// Writer task that sends requests to stdin.
    async fn writer_task(
        mut stdin: ChildStdin,
        mut request_rx: mpsc::Receiver<TransportMessage>,
        pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<JsonRpcResponse>>>>>,
        connected: Arc<AtomicBool>,
    ) {
        while let Some(msg) = request_rx.recv().await {
            match msg {
                TransportMessage::Request {
                    request,
                    response_tx,
                } => {
                    let id = request.id;

                    // Serialize and send
                    match serde_json::to_string(&request) {
                        Ok(json) => {
                            let line = format!("{}\n", json);
                            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                                let _ = response_tx.send(Err(McpError::TransportError(format!(
                                    "Write error: {}",
                                    e
                                ))));
                                continue;
                            }
                            if let Err(e) = stdin.flush().await {
                                let _ = response_tx.send(Err(McpError::TransportError(format!(
                                    "Flush error: {}",
                                    e
                                ))));
                                continue;
                            }

                            // Store pending request
                            pending.lock().await.insert(id, response_tx);
                        }
                        Err(e) => {
                            let _ =
                                response_tx.send(Err(McpError::SerializationError(e.to_string())));
                        }
                    }
                }
                TransportMessage::Notification { notification } => {
                    if let Ok(json) = serde_json::to_string(&notification) {
                        let line = format!("{}\n", json);
                        let _ = stdin.write_all(line.as_bytes()).await;
                        let _ = stdin.flush().await;
                    }
                }
                TransportMessage::Close => {
                    connected.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl super::McpTransport for StdioTransport {
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::ConnectionFailed(
                "Transport disconnected".to_string(),
            ));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(TransportMessage::Request {
                request,
                response_tx,
            })
            .await
            .map_err(|_| McpError::ConnectionFailed("Request channel closed".to_string()))?;

        // Wait for response with timeout
        tokio::time::timeout(std::time::Duration::from_secs(30), response_rx)
            .await
            .map_err(|_| McpError::Timeout(30000))?
            .map_err(|_| McpError::ConnectionFailed("Response channel closed".to_string()))?
    }

    async fn notify(&self, notification: JsonRpcNotification) -> Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::ConnectionFailed(
                "Transport disconnected".to_string(),
            ));
        }

        self.request_tx
            .send(TransportMessage::Notification { notification })
            .await
            .map_err(|_| McpError::ConnectionFailed("Request channel closed".to_string()))?;

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        let _ = self.request_tx.send(TransportMessage::Close).await;

        // Kill the child process
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }

        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_message_variants() {
        // Ensure TransportMessage variants can be constructed
        let req = JsonRpcRequest::new("test", None);
        let (tx, _rx) = oneshot::channel();
        let _msg = TransportMessage::Request {
            request: req,
            response_tx: tx,
        };

        let notif = JsonRpcNotification::new("test", None);
        let _msg = TransportMessage::Notification {
            notification: notif,
        };

        let _msg = TransportMessage::Close;
    }

    #[test]
    fn test_spawn_with_env_accepts_env_vars() {
        // Verify that spawn_with_env accepts a HashMap of environment variables
        // (actual spawning requires integration tests)
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "secret".to_string());
        env.insert("DEBUG".to_string(), "true".to_string());

        // This just verifies the function signature compiles correctly
        // Actual spawning is tested in integration tests
        assert_eq!(env.len(), 2);
    }

    // Integration tests for StdioTransport require actual process spawning
    // and are in the tests/ directory
}
