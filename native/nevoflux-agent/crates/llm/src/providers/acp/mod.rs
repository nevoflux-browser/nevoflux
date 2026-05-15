//! ACP (Agent Communication Protocol) provider.
//!
//! Communicates with CLI agents (claude-code, gemini-cli) over stdio using the
//! sacp protocol. A background tokio task owns the `ClientToAgent` connection;
//! the public `AcpProvider` sends requests through an `mpsc` channel.

pub mod claude;
pub mod context;
pub mod gemini;
pub mod mcp_bridge;
pub mod openclaw;
pub mod tools;

// Re-export key schema types so downstream crates (e.g. nevoflux-daemon)
// can construct ContentBlock values without a direct sacp dependency.
pub use sacp::schema::{ContentBlock, TextContent};

use sacp::schema::{
    ContentChunk, InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
    PromptRequest, ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionId, SessionNotification, SessionUpdate,
    SetSessionModeRequest, StopReason,
};
use sacp::{ClientToAgent, JrConnectionCx, JrMessage};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from ACP provider operations.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("{0}")]
    Internal(String),
}

impl From<String> for AcpError {
    fn from(s: String) -> Self {
        AcpError::Internal(s)
    }
}

type Result<T> = std::result::Result<T, AcpError>;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for spawning an ACP agent process.
#[derive(Clone, Debug)]
pub struct AcpProviderConfig {
    /// Path to the CLI executable (e.g. `claude` or `gemini`).
    pub command: PathBuf,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Extra environment variables to set.
    pub env: Vec<(String, String)>,
    /// Environment variables to remove before spawning.
    pub env_remove: Vec<String>,
    /// Working directory for the agent session.
    pub work_dir: PathBuf,
    /// Session mode to request (e.g. "plan", "code").
    pub session_mode: String,
    /// When true, use MCP server for native tool calling.
    /// When false, use <tool_call> XML extraction.
    pub use_mcp_bridge: bool,
    /// Whether to inject MCP URL into NewSessionRequest.
    /// false for OpenClaw (registers via gateway config instead).
    pub inject_mcp_url: bool,
}

/// Internal request sent from `AcpProvider` to the background client loop.
pub(crate) enum ClientRequest {
    NewSession {
        response_tx: oneshot::Sender<Result<NewSessionResponse>>,
    },
    Prompt {
        session_id: SessionId,
        content: Vec<ContentBlock>,
        response_tx: mpsc::Sender<AcpUpdate>,
    },
    Shutdown,
}

/// Updates streamed back from the ACP agent during a prompt.
#[derive(Debug)]
pub enum AcpUpdate {
    /// Incremental text from the agent's response.
    Text(String),
    /// Incremental thought/reasoning from the agent.
    Thought(String),
    /// The agent has finished responding.
    Complete(StopReason),
    /// A protocol-level error occurred.
    Error(String),
}

/// ACP-based LLM provider that communicates with a CLI agent over stdio.
///
/// Use [`AcpProvider::new`] to create an instance, then [`AcpProvider::connect`]
/// to spawn the process and establish the protocol handshake.
pub struct AcpProvider {
    config: AcpProviderConfig,
    tx: Option<mpsc::Sender<ClientRequest>>,
    /// MCP tool bridge, only present when use_mcp_bridge is true.
    tool_bridge: Option<Arc<mcp_bridge::McpToolBridge>>,
}

impl AcpProvider {
    /// Create a new (disconnected) provider with the given config.
    pub fn new(config: AcpProviderConfig) -> Self {
        let tool_bridge = if config.use_mcp_bridge {
            Some(Arc::new(mcp_bridge::McpToolBridge::new()))
        } else {
            None
        };
        Self {
            config,
            tx: None,
            tool_bridge,
        }
    }

    /// Return the MCP tool bridge, if present.
    pub fn tool_bridge(&self) -> Option<&Arc<mcp_bridge::McpToolBridge>> {
        self.tool_bridge.as_ref()
    }

    /// Spawn the agent process and complete the ACP handshake.
    pub async fn connect(&mut self) -> Result<()> {
        let child = spawn_acp_process(&self.config).await?;
        let (tx, rx) = mpsc::channel(32);
        let (init_tx, init_rx) = oneshot::channel();
        let config = self.config.clone();

        let tool_bridge = self.tool_bridge.clone();
        tokio::spawn(async move {
            if let Err(e) = run_client_loop_direct(config, child, rx, init_tx, tool_bridge).await {
                tracing::error!(error = %e, "ACP client loop error");
            }
        });

        let init_result = init_rx
            .await
            .map_err(|_| AcpError::Internal("ACP client initialization cancelled".into()))?;
        let _init_response = init_result?;

        self.tx = Some(tx);
        Ok(())
    }

    /// Whether the background client loop is still running.
    pub fn is_alive(&self) -> bool {
        self.tx.as_ref().is_some_and(|tx| !tx.is_closed())
    }

    /// Reconnect if the background client loop has died.
    pub async fn ensure_connected(&mut self) -> Result<()> {
        if !self.is_alive() {
            tracing::info!("ACP connection lost, reconnecting...");
            self.connect().await?;
        }
        Ok(())
    }

    /// Create a new session on the connected agent.
    pub async fn new_session(&self) -> Result<SessionId> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| AcpError::Internal("ACP provider is not connected".into()))?;

        let (response_tx, response_rx) = oneshot::channel();
        tx.send(ClientRequest::NewSession { response_tx })
            .await
            .map_err(|_| AcpError::Internal("ACP client is unavailable".into()))?;

        let session = response_rx
            .await
            .map_err(|_| AcpError::Internal("ACP session/new cancelled".into()))??;
        Ok(session.session_id)
    }

    /// Send a prompt to the agent and receive streaming updates.
    pub async fn prompt(
        &self,
        session_id: SessionId,
        content: Vec<ContentBlock>,
    ) -> Result<mpsc::Receiver<AcpUpdate>> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| AcpError::Internal("ACP provider is not connected".into()))?;

        let (response_tx, response_rx) = mpsc::channel(64);
        tx.send(ClientRequest::Prompt {
            session_id,
            content,
            response_tx,
        })
        .await
        .map_err(|_| AcpError::Internal("ACP client is unavailable".into()))?;

        Ok(response_rx)
    }

    /// Shut down the agent process gracefully.
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(ClientRequest::Shutdown).await;
        }
    }
}

impl Drop for AcpProvider {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            tokio::spawn(async move {
                let _ = tx.send(ClientRequest::Shutdown).await;
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Process spawning
// ---------------------------------------------------------------------------

async fn spawn_acp_process(config: &AcpProviderConfig) -> Result<Child> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    // Ensure the child process has an extended PATH that includes common
    // locations for npm-installed CLIs (e.g. /usr/local/bin on macOS).
    // This is critical because shebang scripts like `gemini` use
    // `#!/usr/bin/env node` which needs `node` in the child's PATH.
    if let Some(extended_path) = crate::util::build_search_path() {
        cmd.env("PATH", extended_path);
    }

    for key in &config.env_remove {
        cmd.env_remove(key);
    }
    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    tracing::info!(
        command = %config.command.display(),
        args = ?config.args,
        work_dir = %config.work_dir.display(),
        "ACP: spawning process"
    );

    cmd.spawn()
        .map_err(|e| AcpError::Internal(format!("failed to spawn ACP process: {e}")))
}

// ---------------------------------------------------------------------------
// Background client loop
// ---------------------------------------------------------------------------

async fn run_client_loop_direct(
    config: AcpProviderConfig,
    mut child: Child,
    mut rx: mpsc::Receiver<ClientRequest>,
    init_tx: oneshot::Sender<Result<InitializeResponse>>,
    tool_bridge: Option<Arc<mcp_bridge::McpToolBridge>>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdin = child.stdin.take().ok_or("no stdin on ACP child process")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("no stdout on ACP child process")?;

    let transport = sacp::ByteStreams::new(stdin.compat_write(), stdout.compat());

    let prompt_response_tx: Arc<Mutex<Option<mpsc::Sender<AcpUpdate>>>> =
        Arc::new(Mutex::new(None));

    let error_notify_tx = prompt_response_tx.clone();

    let result = ClientToAgent::builder()
        .on_receive_notification(
            {
                let prompt_response_tx = prompt_response_tx.clone();
                // Use UntypedMessage to catch ALL notifications including unknown ones
                // like `usage_update` which would crash the loop if we used SessionNotification.
                // sacp 10.x / 11.x fails to deserialize `usage_update` variant, causing
                // `Some(Err(parse_error))` which terminates the client loop.
                async move |message: sacp::UntypedMessage, _cx| {
                    // Try to parse as SessionNotification — ignore parse failures silently
                    if let Some(Ok(notification)) =
                        SessionNotification::parse_message(&message.method, &message.params)
                    {
                        if let Some(tx) = prompt_response_tx
                            .lock()
                            .ok()
                            .as_ref()
                            .and_then(|g| g.as_ref())
                        {
                            match notification.update {
                                SessionUpdate::AgentMessageChunk(ContentChunk {
                                    content: ContentBlock::Text(TextContent { text, .. }),
                                    ..
                                }) => {
                                    let _ = tx.try_send(AcpUpdate::Text(text));
                                }
                                SessionUpdate::AgentThoughtChunk(ContentChunk {
                                    content: ContentBlock::Text(TextContent { text, .. }),
                                    ..
                                }) => {
                                    let _ = tx.try_send(AcpUpdate::Thought(text));
                                }
                                _ => {}
                            }
                        }
                    }
                    // Unknown notifications (usage_update, etc.) are silently ignored
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let tool_bridge = tool_bridge.clone();
                // Permission handler: forwards to sidebar via McpToolBridge for user approval.
                // Session-level "Always Allow" decisions are cached to avoid repeated prompts.
                async move |request: RequestPermissionRequest, request_cx, _connection_cx| {
                    use mcp_bridge::PermissionResponse;
                    use sacp::schema::SelectedPermissionOutcome;

                    // Extract tool name from toolCallId or title.
                    // Gemini CLI uses toolCallId format: "mcp_nevoflux-tools_<tool_name>-<timestamp>"
                    // Claude Code uses title: "mcp__nevoflux-tools__<tool_name>"
                    let tool_call_id = request.tool_call.tool_call_id.0.to_string();
                    let tool_name = extract_tool_name_from_id(&tool_call_id)
                        .or_else(|| request.tool_call.fields.title.clone())
                        .unwrap_or_default();
                    let args_summary = request
                        .tool_call
                        .fields
                        .raw_input
                        .as_ref()
                        .and_then(|v| serde_json::to_string(v).ok())
                        .unwrap_or_default();

                    // Check with McpToolBridge — may ask sidebar or return cached decision
                    let decision = if let Some(ref bridge) = tool_bridge {
                        bridge.request_permission(&tool_name, &args_summary).await
                    } else {
                        // No bridge (non-MCP mode) — auto-approve
                        PermissionResponse::AllowOnce
                    };

                    // Map decision to ACP option ID
                    let option_id = match decision {
                        PermissionResponse::AllowAlways => {
                            // Pick the per-tool allow_always option (e.g. "proceed_always_tool"),
                            // NOT the per-server option ("proceed_always_server") which would
                            // bypass permission for ALL tools. Prefer option whose name contains
                            // the tool name (Gemini uses "Always Allow <tool_name>").
                            request
                                .options
                                .iter()
                                .find(|o| {
                                    matches!(
                                        o.kind,
                                        sacp::schema::PermissionOptionKind::AllowAlways
                                    ) && o.option_id.0.as_ref().contains("tool")
                                })
                                .or_else(|| {
                                    // Fallback: any allow_always
                                    request.options.iter().find(|o| {
                                        matches!(
                                            o.kind,
                                            sacp::schema::PermissionOptionKind::AllowAlways
                                        )
                                    })
                                })
                                .or_else(|| {
                                    request.options.iter().find(|o| {
                                        matches!(
                                            o.kind,
                                            sacp::schema::PermissionOptionKind::AllowOnce
                                        )
                                    })
                                })
                                .map(|o| o.option_id.0.to_string())
                                .unwrap_or_else(|| "allow".to_string())
                        }
                        PermissionResponse::AllowOnce => {
                            // Pick the allow_once option
                            request
                                .options
                                .iter()
                                .find(|o| {
                                    matches!(o.kind, sacp::schema::PermissionOptionKind::AllowOnce)
                                })
                                .map(|o| o.option_id.0.to_string())
                                .unwrap_or_else(|| "allow".to_string())
                        }
                        PermissionResponse::Reject => {
                            // Pick the reject option, or cancel
                            request
                                .options
                                .iter()
                                .find(|o| {
                                    matches!(o.kind, sacp::schema::PermissionOptionKind::RejectOnce)
                                })
                                .map(|o| o.option_id.0.to_string())
                                .unwrap_or_else(|| "cancel".to_string())
                        }
                    };

                    let response =
                        RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
                            SelectedPermissionOutcome::new(option_id),
                        ));
                    request_cx.respond(response)
                }
            },
            sacp::on_receive_request!(),
        )
        .connect_to(transport)?
        .run_until(|cx: JrConnectionCx<ClientToAgent>| {
            handle_requests(
                config,
                cx,
                &mut rx,
                prompt_response_tx,
                init_tx,
                tool_bridge,
            )
        })
        .await;

    // If the client loop exits (e.g. due to a parse error from an unknown
    // notification like "usage_update"), notify any active prompt receiver
    // so it doesn't hang forever waiting for AcpUpdate::Complete.
    if let Some(tx) = error_notify_tx.lock().ok().and_then(|mut g| g.take()) {
        let err_msg = match &result {
            Ok(()) => "ACP client loop exited".to_string(),
            Err(e) => format!("ACP client loop error: {e}"),
        };
        // Treat unexpected exit as completion — the agent likely already
        // finished its response (text chunks were delivered via notifications).
        // sacp 10.x doesn't know about "usage_update" which the agent sends
        // AFTER the response is complete, causing a parse error.
        tracing::warn!("{}, sending synthetic completion", err_msg);
        let _ = tx.try_send(AcpUpdate::Complete(StopReason::EndTurn));
    }

    result.map_err(|e| e.into())
}

/// Extract tool name from ACP toolCallId.
/// Gemini CLI: "mcp_nevoflux-tools_browser_get_markdown-1774240394151" → "browser_get_markdown"
/// Claude Code: "toolu_01BKyw4Ubz7YNgaL5vNouGCo" → None (use title instead)
fn extract_tool_name_from_id(tool_call_id: &str) -> Option<String> {
    // Gemini format: mcp_<server>_<tool_name>-<timestamp>
    if tool_call_id.starts_with("mcp_nevoflux-tools_") {
        let rest = &tool_call_id["mcp_nevoflux-tools_".len()..];
        // Remove trailing -<timestamp>
        if let Some(dash_pos) = rest.rfind('-') {
            let name = &rest[..dash_pos];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    // Claude Code format with double underscore
    if tool_call_id.contains("__nevoflux-tools__") {
        let parts: Vec<&str> = tool_call_id.split("__").collect();
        if let Some(name) = parts.last() {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

async fn handle_requests(
    config: AcpProviderConfig,
    cx: JrConnectionCx<ClientToAgent>,
    rx: &mut mpsc::Receiver<ClientRequest>,
    prompt_response_tx: Arc<Mutex<Option<mpsc::Sender<AcpUpdate>>>>,
    init_tx: oneshot::Sender<Result<InitializeResponse>>,
    tool_bridge: Option<Arc<mcp_bridge::McpToolBridge>>,
) -> std::result::Result<(), sacp::Error> {
    let mut init_tx = Some(init_tx);

    let init_response = cx
        .send_request(InitializeRequest::new(ProtocolVersion::LATEST))
        .block_task()
        .await
        .map_err(|err| {
            let message = format!("ACP initialize failed: {err}");
            if let Some(tx) = init_tx.take() {
                let _ = tx.send(Err(AcpError::Internal(message.clone())));
            }
            sacp::Error::internal_error().data(message)
        })?;

    if let Some(tx) = init_tx.take() {
        let _ = tx.send(Ok(init_response));
    }

    while let Some(request) = rx.recv().await {
        match request {
            ClientRequest::NewSession { response_tx } => {
                tracing::info!(
                    cwd = %config.work_dir.display(),
                    "ACP: sending NewSessionRequest"
                );
                let mut request = NewSessionRequest::new(config.work_dir.clone());
                if config.inject_mcp_url {
                    if let Some(ref bridge) = tool_bridge {
                        if let Some(url) = bridge.mcp_server_url() {
                            use sacp::schema::McpServerHttp;
                            request.mcp_servers.push(sacp::schema::McpServer::Http(
                                McpServerHttp::new("nevoflux-tools", url),
                            ));
                            tracing::info!("ACP: injecting MCP server URL into session: {}", url);
                        }
                    }
                }
                let session = cx.send_request(request).block_task().await;

                let result = match session {
                    Ok(session) => {
                        let modes_str = session
                            .modes
                            .as_ref()
                            .map(|m| {
                                let available: Vec<&str> = m
                                    .available_modes
                                    .iter()
                                    .map(|mode| mode.id.0.as_ref())
                                    .collect();
                                format!(
                                    "current={}, available=[{}]",
                                    m.current_mode_id.0,
                                    available.join(", ")
                                )
                            })
                            .unwrap_or_else(|| "None".to_string());
                        tracing::info!(
                            session_id = %session.session_id.0,
                            modes = %modes_str,
                            "ACP: session created"
                        );
                        apply_session_mode(&config, &cx, session).await
                    }
                    Err(err) => Err(AcpError::Internal(format!("ACP session/new failed: {err}"))),
                };

                let _ = response_tx.send(result);
            }

            ClientRequest::Prompt {
                session_id,
                content,
                response_tx,
            } => {
                let content_len: usize = content
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text(t) => t.text.len(),
                        _ => 0,
                    })
                    .sum();
                tracing::info!(
                    session_id = %session_id.0,
                    content_blocks = content.len(),
                    content_bytes = content_len,
                    "ACP: sending PromptRequest"
                );
                *prompt_response_tx.lock().unwrap() = Some(response_tx.clone());

                let response = cx
                    .send_request(PromptRequest::new(session_id, content))
                    .block_task()
                    .await;

                match response {
                    Ok(r) => {
                        tracing::info!(stop_reason = ?r.stop_reason, "ACP: prompt completed");
                        let _ = response_tx.try_send(AcpUpdate::Complete(r.stop_reason));
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "ACP: prompt failed");
                        let _ = response_tx.try_send(AcpUpdate::Error(e.to_string()));
                    }
                }

                *prompt_response_tx.lock().unwrap() = None;
            }

            ClientRequest::Shutdown => break,
        }
    }

    Ok(())
}

async fn apply_session_mode(
    config: &AcpProviderConfig,
    cx: &JrConnectionCx<ClientToAgent>,
    session: NewSessionResponse,
) -> Result<NewSessionResponse> {
    if let Some(modes) = session.modes.as_ref() {
        if modes.current_mode_id.0.as_ref() != config.session_mode.as_str() {
            let available: Vec<String> = modes
                .available_modes
                .iter()
                .map(|mode| mode.id.0.to_string())
                .collect();

            if !available.iter().any(|id| id == &config.session_mode) {
                return Err(AcpError::Internal(format!(
                    "Requested mode '{}' not offered by agent. Available modes: {}",
                    config.session_mode,
                    available.join(", ")
                )));
            }

            cx.send_request(SetSessionModeRequest::new(
                session.session_id.clone(),
                config.session_mode.clone(),
            ))
            .block_task()
            .await
            .map_err(|err| {
                AcpError::Internal(format!("ACP agent rejected session/set_mode: {err}"))
            })?;
        }
    }

    Ok(session)
}
