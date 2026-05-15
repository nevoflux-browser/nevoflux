//! TCP server for the daemon.

use crate::agent_host::{DaemonHostFunctions, SidebarStreamChunk};
use crate::config::AgentConfig;
use crate::error::{DaemonError, Result};
use crate::router::{RouteDecision, Router};
use crate::session::SessionManager;
use crate::trace::collector::TraceCollector;
use crate::trace::file_writer::TraceFileWriter;
use crate::wasm::{BrowserRequest, BrowserResponse, HostServices};
use nevoflux_builtin_wasm::{Agent, AgentInput, AgentMode, Attachment, Message as WasmMessage};
use nevoflux_protocol::{
    AgentMessage, Artifact, ArtifactComplete, ArtifactDelta, ArtifactStart, Channel,
    DaemonEnvelope, PlanProposal, PlanResponse, ProxyEnvelope, ToolAuthResponse,
};
use nevoflux_skills::{check_tool_availability, format_missing_tools_message, ToolCheckResult};
use nevoflux_storage::{ContentType, ListSessionsParams, Message as StorageMessage, MessageRole};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, error, info, warn};

/// Mirror a ContentStore write for a `canvas:{id}` key into the `artifacts`
/// table so downstream readers (canvas.share in particular) see the user's
/// latest edits.
///
/// Background: canvas artifacts live in two places — the `artifacts` SQL
/// table (populated by `save_artifact` at create time) and the ContentStore
/// key-value config table under key `canvas:{id}` (rewritten on every
/// in-browser edit). Without this mirror the two diverge: the table freezes
/// at creation-time state while ContentStore tracks latest, so `canvas.share`
/// (which reads the table via `load_artifact`) uploads a stale snapshot.
///
/// Best-effort: failures are logged, never propagated — the ContentStore
/// write itself has already succeeded and the caller must not be penalized.
fn mirror_canvas_to_artifacts_table(
    session_manager: &SessionManager,
    key: &str,
    value: &serde_json::Value,
) {
    let id = match key.strip_prefix("canvas:") {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return,
    };
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            warn!(
                "ContentStore canvas value for {} is not an object, skipping artifacts mirror",
                key
            );
            return;
        }
    };

    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled")
        .to_string();
    let content_type = obj
        .get("content_type")
        .or_else(|| obj.get("contentType"))
        .and_then(|v| v.as_str())
        .unwrap_or("text/html")
        .to_string();
    let content = obj
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // Diagnostic: log a fingerprint of the incoming write so we can tell
    // whether ContentStore is sending fresh edits or stale state.
    {
        let files_obj = obj.get("files").and_then(|v| v.as_object());
        let files_summary = files_obj
            .map(|m| {
                m.iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_str().map(|s| s.len()).unwrap_or(0)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_else(|| "<none>".into());
        let probe_brand = files_obj
            .and_then(|m| m.get("index.html"))
            .and_then(|v| v.as_str())
            .map(|s| s.contains("全新 GPT 体验"))
            .unwrap_or(false);
        let probe_orange = files_obj
            .and_then(|m| m.get("DESIGN.md"))
            .and_then(|v| v.as_str())
            .map(|s| s.contains("#ff6600"))
            .unwrap_or(false);
        info!(
            "mirror_canvas: id={}, content_len={}, files=[{}], idx_has_new_brand={}, design_has_ff6600={}",
            id, content.len(), files_summary, probe_brand, probe_orange
        );
    }
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let files = obj.get("files").and_then(|v| v.as_object()).map(|m| {
        m.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect::<HashMap<String, String>>()
    });
    let entry = obj
        .get("entry")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Resolve the row state: existing rows take the UPDATE path (preserves
    // session_id, works even when it's NULL), missing rows take the INSERT
    // path which requires a session_id from the value or we skip.
    let existing_row = match session_manager.get_artifact(&id) {
        Ok(opt) => opt,
        Err(e) => {
            warn!("get_artifact({}) failed during mirror: {:#}", id, e);
            return;
        }
    };

    if existing_row.is_some() {
        // Existing row: prefer update_files which only touches files+content
        // +updated_at. This is essential for persistent artifacts whose
        // session_id has been SET NULL (canvas_create_composition via the
        // MCP path also creates with NULL session_id because the LLM tool
        // args don't include one). Going through save_artifact's
        // INSERT-ON-CONFLICT path would require a session_id and silently
        // skip those rows, orphaning every Canvas Editor / browser_edit
        // edit in the config table.
        //
        // Migration 016 moved binary assets into the dedicated
        // `composition_assets` table, so `artifacts.files` is now
        // text-only (DESIGN.md, index.html, composition.meta.json). The
        // historical defensive merge for `assets/*` entries is no longer
        // needed — the editable surface and the asset surface are now
        // separate sources of truth, written by separate paths, and
        // never overlap.
        let files_for_update = files.unwrap_or_default();
        match session_manager.update_artifact_files(&id, &files_for_update, &content) {
            Ok(true) => {}
            Ok(false) => {
                warn!(
                    "update_artifact_files mirror for canvas {} returned 0 rows (vanished?)",
                    id
                );
            }
            Err(e) => {
                warn!(
                    "update_artifact_files mirror for canvas {} failed: {:#}",
                    id, e
                );
            }
        }
        return;
    }

    // No existing row: need to INSERT. INSERT requires a session_id from the
    // value; otherwise we'd create an orphan that violates the FK.
    let session_id_from_value = obj
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let session_id = match session_id_from_value {
        Some(s) => s,
        None => {
            debug!(
                "ContentStore canvas {} has no session_id and no existing row; skipping artifacts mirror",
                id
            );
            return;
        }
    };

    let mut params =
        nevoflux_storage::CreateArtifactParams::new(&id, &session_id, &title, &content_type)
            .with_content(&content);
    if let Some(d) = description {
        params = params.with_description(&d);
    }
    if let Some(f) = files {
        params = params.with_files(f);
    }
    if let Some(e) = entry {
        params = params.with_entry(&e);
    }

    if let Err(e) = session_manager.save_artifact(params) {
        warn!("save_artifact mirror for canvas {} failed: {:#}", id, e);
    }
}

/// Registry for pending browser tool requests.
/// Maps request_id to (created_at, response_sender).
/// Entries are cleaned up periodically to prevent unbounded growth.
type BrowserRequestRegistry =
    Arc<Mutex<HashMap<String, (std::time::Instant, oneshot::Sender<BrowserResponse>)>>>;

/// Registry for pending plan proposals.
/// Maps session_id to the response sender.
type PlanRequestRegistry = Arc<Mutex<HashMap<String, oneshot::Sender<PlanResponse>>>>;

/// Registry for active streaming sessions that can be cancelled.
/// Maps session_id to the cancellation token.
type CancellationRegistry = Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>;

/// Registry for active agent interrupt flags.
/// Maps session_id to the interrupt flag so stop_generation can signal the agent to stop.
type InterruptRegistry = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

/// Registry for pending tool authorization requests.
/// Maps tool_id to the response sender.
type ToolAuthRegistry = Arc<Mutex<HashMap<String, oneshot::Sender<ToolAuthResponse>>>>;

/// Registry for session-level memory extractors.
/// Maps session_id to a shared SessionMemoryExtractor so message counts accumulate across turns.
type ExtractionRegistry = Arc<
    Mutex<
        HashMap<
            String,
            (
                std::time::Instant,
                Arc<crate::learning::session_extractor::SessionMemoryExtractor>,
            ),
        >,
    >,
>;

/// Tracks EventBus subscription-to-proxy mappings for delivery routing.
#[allow(dead_code)]
struct SubscriptionEntry {
    proxy_id: String,
    identity: Vec<u8>,
    cancel_token: tokio_util::sync::CancellationToken,
}

type SubscriptionRouter = Arc<Mutex<HashMap<String, SubscriptionEntry>>>;

/// Shared mutable agent config that can be updated at runtime (e.g. when changing active LLM provider).
type SharedAgentConfig = Arc<RwLock<Arc<AgentConfig>>>;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Port range start.
    pub port_start: u16,
    /// Port range end.
    pub port_end: u16,
    /// Bind address (default: 127.0.0.1).
    pub bind_address: String,
    /// Whether trace collection is enabled.
    pub trace_enabled: bool,
    /// Whether this daemon is managed by a proxy (self-terminates on idle).
    pub managed: bool,
    /// Idle timeout before self-termination (only used when `managed` is true).
    pub idle_timeout: std::time::Duration,
    /// Data directory for writing port/pid files early during startup.
    /// When set, port and pid files are written immediately after the port
    /// is found, before MCP/embedding initialization completes.
    pub data_dir: Option<PathBuf>,
    /// Explicit port to bind to (set by proxy in managed mode).
    /// When set, skips port scanning and port/pid file writes.
    pub explicit_port: Option<u16>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port_start: 19500,
            port_end: 19600,
            bind_address: "127.0.0.1".into(),
            trace_enabled: false,
            managed: false,
            idle_timeout: std::time::Duration::from_secs(30),
            data_dir: None,
            explicit_port: None,
        }
    }
}

/// The TCP server handle.
pub struct Server {
    /// The bound port.
    port: u16,
    /// Shutdown signal sender.
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl Server {
    /// Get the bound port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Signal the server to shutdown.
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}

/// Parse an agent mode string into `AgentMode`.
///
/// `"code"` is deprecated and silently maps to `AgentMode::Agent`.
/// Unknown strings default to `AgentMode::Chat`.
fn parse_agent_mode(mode_str: &str) -> AgentMode {
    match mode_str {
        "browser" => AgentMode::Browser,
        "agent" => AgentMode::Agent,
        "code" => AgentMode::Agent, // Code mode deprecated, maps to Agent
        _ => AgentMode::Chat,
    }
}

/// Find an available port in the range.
pub async fn find_available_port(config: &ServerConfig) -> Result<u16> {
    use std::net::TcpListener;

    for port in config.port_start..=config.port_end {
        if TcpListener::bind((&*config.bind_address, port)).is_ok() {
            return Ok(port);
        }
    }

    Err(DaemonError::PortExhausted)
}

/// Maximum message size for length-prefixed TCP framing (1 MB).
const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Handle a single proxy TCP connection.
///
/// Reads the registration frame to learn the proxy_id, then loops reading
/// length-prefixed JSON frames and forwarding them to the message processing loop.
async fn handle_proxy_connection(
    stream: tokio::net::TcpStream,
    msg_tx: mpsc::Sender<(Vec<u8>, ProxyEnvelope)>,
    writers: Arc<Mutex<HashMap<String, BufWriter<tokio::net::tcp::OwnedWriteHalf>>>>,
    last_message_time: Arc<Mutex<std::time::Instant>>,
) {
    let (read_half, write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Read registration frame: { "type": "register", "proxy_id": "proxy-xxx" }
    let proxy_id = match read_length_prefixed_message(&mut reader).await {
        Ok(data) => match serde_json::from_slice::<serde_json::Value>(&data) {
            Ok(val) => {
                if val.get("type").and_then(|t| t.as_str()) == Some("register") {
                    if let Some(id) = val.get("proxy_id").and_then(|v| v.as_str()) {
                        id.to_string()
                    } else {
                        error!("Registration frame missing proxy_id");
                        return;
                    }
                } else {
                    error!("First frame is not a registration frame");
                    return;
                }
            }
            Err(e) => {
                error!("Failed to parse registration frame: {}", e);
                return;
            }
        },
        Err(e) => {
            // Early EOF is normal for health-check / probe connections
            debug!("Connection closed before registration: {}", e);
            return;
        }
    };

    info!("Proxy registered: {}", proxy_id);

    // Register writer
    {
        let writer = BufWriter::new(write_half);
        writers.lock().await.insert(proxy_id.clone(), writer);
    }

    // Identity bytes (proxy_id encoded as UTF-8) for compatibility with existing pipeline
    let identity = proxy_id.as_bytes().to_vec();

    // Read loop: read length-prefixed JSON frames
    loop {
        match read_length_prefixed_message(&mut reader).await {
            Ok(data) => match serde_json::from_slice::<ProxyEnvelope>(&data) {
                Ok(envelope) => {
                    let msg_type = envelope
                        .payload
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    info!(
                        "Socket received: type={}, proxy_id={}",
                        msg_type, envelope.proxy_id
                    );
                    *last_message_time.lock().await = std::time::Instant::now();
                    if msg_tx.send((identity.clone(), envelope)).await.is_err() {
                        debug!("Message channel closed, stopping reader for {}", proxy_id);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to parse ProxyEnvelope from {}: {}", proxy_id, e);
                }
            },
            Err(e) => {
                info!("Proxy {} disconnected: {}", proxy_id, e);
                break;
            }
        }
    }

    // Clean up writer
    writers.lock().await.remove(&proxy_id);

    // Notify the message loop about the disconnect so EventBus subscriptions
    // belonging to this proxy can be cleaned up.
    let disconnect_payload = serde_json::json!({
        "type": "_proxy_disconnected",
        "proxy_id": proxy_id,
    });
    let disconnect_envelope = ProxyEnvelope::new(&proxy_id, "", Channel::Chat, disconnect_payload);
    let _ = msg_tx.send((identity.clone(), disconnect_envelope)).await;

    info!("Proxy {} cleaned up", proxy_id);
}

/// Read a single length-prefixed message from a TCP stream.
///
/// Format: 4-byte little-endian length + JSON payload.
async fn read_length_prefixed_message(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> std::result::Result<Vec<u8>, std::io::Error> {
    use tokio::io::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf);

    if len > MAX_MESSAGE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            ),
        ));
    }

    if len == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Empty message",
        ));
    }

    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Start the TCP server.
pub async fn start_server(
    config: ServerConfig,
    router: Arc<Router>,
    session_manager: Arc<SessionManager>,
) -> Result<Server> {
    let port = if let Some(p) = config.explicit_port {
        info!("Using explicit port {}", p);
        p
    } else {
        find_available_port(&config).await?
    };
    let bind_addr = format!("{}:{}", config.bind_address, port);

    // Bind TCP listener immediately so the port is actually open before we
    // advertise it via the port file. This prevents the bridge from getting
    // "connection refused" while we do slow initialization.
    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| DaemonError::InternalError(format!("Failed to bind: {}", e)))?;
    info!("TCP listener bound on {}", bind_addr);

    // Write port/pid files only when needed (dev mode or managed without
    // explicit port). In managed+explicit_port mode the proxy already knows
    // port and PID, so no files are written — zero disk artifacts.
    let skip_files = config.managed && config.explicit_port.is_some();
    if !skip_files {
        if let Some(ref data_dir) = config.data_dir {
            let (port_name, pid_name) = if config.managed {
                ("daemon-managed.port", "daemon-managed.pid")
            } else {
                ("daemon.port", "daemon.pid")
            };
            if let Err(e) = std::fs::write(data_dir.join(port_name), port.to_string()) {
                error!("Failed to write port file: {}", e);
            }
            if let Err(e) = std::fs::write(data_dir.join(pid_name), std::process::id().to_string())
            {
                error!("Failed to write pid file: {}", e);
            }
            info!(
                "Port file written early: {}/{}",
                data_dir.display(),
                port_name
            );
        }
    }

    info!("Starting daemon server on {}", bind_addr);

    // Load agent config for LLM settings
    let agent_config = match AgentConfig::load() {
        Ok(cfg) => {
            info!(
                "Loaded agent config: llm.provider={:?}",
                cfg.llm.active_provider()
            );
            cfg
        }
        Err(e) => {
            error!("Failed to load agent config: {}, using defaults", e);
            AgentConfig::default()
        }
    };
    let agent_config: SharedAgentConfig = Arc::new(RwLock::new(Arc::new(agent_config)));

    // Create host services with database from session manager
    let db = session_manager.storage().database().clone();

    // Create browser request channel and registry
    let (browser_tx, mut browser_rx) =
        mpsc::channel::<(BrowserRequest, oneshot::Sender<BrowserResponse>)>(100);
    let browser_registry: BrowserRequestRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Create cancellation registry for active streaming sessions
    let cancellation_registry: CancellationRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Create interrupt registry for signalling agents to stop
    let interrupt_registry: InterruptRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Create plan request registry for pending plan proposals
    let plan_registry: PlanRequestRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Create tool auth registry for pending tool authorization requests
    let tool_auth_registry: ToolAuthRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Create extraction registry for session-level memory extractors
    let extraction_registry: ExtractionRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Initialize EventBus with persistence
    let event_bus = {
        use crate::event_bus::{EventBus, PersistentCleaner, PersistentWriter};
        let storage_arc = session_manager.shared_storage();
        let (writer_handle, writer) = PersistentWriter::new(storage_arc.clone());
        tokio::spawn(writer.run());
        let cleaner = PersistentCleaner::new(storage_arc);
        tokio::spawn(cleaner.run());
        Arc::new(EventBus::with_persistence(writer_handle))
    };
    let subscription_router: SubscriptionRouter = Arc::new(Mutex::new(HashMap::new()));

    // Initialize MCP manager (empty) and tool search index.
    // Actual connections happen in a background task so the daemon starts fast.
    let mcp_manager = {
        use nevoflux_mcp::{ManagerConfig, McpManager};
        Arc::new(McpManager::new(ManagerConfig::default()))
    };
    let tool_search_index = Arc::new(tokio::sync::RwLock::new(
        nevoflux_mcp::ToolSearchIndex::new(),
    ));

    // Spawn background task: load MCP configs, connect servers, index tools.
    {
        use nevoflux_mcp::ServerConfig as McpServerConfig;

        let bg_manager = Arc::clone(&mcp_manager);
        let bg_tool_search = Arc::clone(&tool_search_index);
        tokio::spawn(async move {
            let mcp_config = match crate::mcp_config::McpServersConfig::load() {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to load MCP config: {}", e);
                    return;
                }
            };

            // Register all server configs first (non-blocking)
            let mut server_names = Vec::new();
            for server in mcp_config.enabled_servers() {
                let sc = if server.server_type == "http" || server.server_type == "sse" {
                    // HTTP/SSE transport: use URL from config
                    let Some(ref url) = server.url else {
                        warn!(
                            "MCP server {} has no URL configured for HTTP/SSE, skipping",
                            server.name
                        );
                        continue;
                    };
                    McpServerConfig::new_http(&server.name, url.as_str())
                } else {
                    // Stdio transport: use command + args
                    let Some(ref command) = server.command else {
                        warn!(
                            "MCP server {} has no command configured, skipping",
                            server.name
                        );
                        continue;
                    };
                    let mut sc = McpServerConfig::new(&server.name, command)
                        .with_args(server.args.iter().map(|s| s.as_str()).collect());
                    for (k, v) in &server.env {
                        sc = sc.with_env(k, v);
                    }
                    sc
                };
                if let Err(e) = bg_manager.add_server(sc).await {
                    warn!("Failed to add MCP server config {}: {}", server.name, e);
                } else {
                    server_names.push(server.name.clone());
                }
            }

            if server_names.is_empty() {
                return;
            }

            // Connect all servers concurrently
            info!(
                "Connecting {} MCP servers in background: {:?}",
                server_names.len(),
                server_names
            );
            let results = bg_manager.connect_all().await;
            let mut connected = 0u32;
            for (name, result) in &results {
                match result {
                    Ok(()) => {
                        connected += 1;
                        info!("Connected MCP server: {}", name);
                    }
                    Err(e) => warn!("Failed to connect MCP server {}: {}", name, e),
                }
            }

            // Index tools from successfully connected servers into the shared search index
            if connected > 0 {
                match bg_manager.list_all_tools().await {
                    Ok(server_tools) => {
                        let tool_defs: Vec<_> =
                            server_tools.iter().map(|st| st.tool.clone()).collect();
                        if !tool_defs.is_empty() {
                            bg_tool_search.write().await.index(&tool_defs);
                            info!("Indexed {} MCP tools for tool_search", tool_defs.len());
                        }
                    }
                    Err(e) => warn!("Failed to list MCP tools for indexing: {}", e),
                }
            }

            info!(
                "MCP background init complete: {}/{} servers connected",
                connected,
                results.len()
            );
        });
    }

    // Shared embedding provider slot — initially empty, populated by the
    // background init task below once the ONNX model finishes loading.
    use crate::wasm::services::SharedEmbedding;
    let shared_embedding: SharedEmbedding = Arc::new(std::sync::RwLock::new(None));

    // Build vector index (starts empty; populated by background task after
    // the embedding provider is ready).
    let vector_index = Arc::new(std::sync::RwLock::new(
        nevoflux_storage::SimpleVectorIndex::new(),
    ));

    // Spawn background embedding init task — loads ONNX model, populates the
    // shared embedding slot, loads existing memory vectors, and starts backfill.
    // This avoids blocking daemon startup (~8-9s for model loading).
    {
        let embedding_slot = Arc::clone(&shared_embedding);
        let vi = Arc::clone(&vector_index);
        let db_arc = session_manager.storage().database().clone();
        let backfill_storage = session_manager.shared_storage();

        #[cfg(feature = "embedding")]
        {
            let embedding_config = agent_config.read().unwrap().embedding.clone();
            if embedding_config.enabled {
                tokio::spawn(async move {
                    use nevoflux_llm::{
                        EmbeddingConfig as LlmEmbeddingConfig, EmbeddingModel, FastEmbedProvider,
                    };

                    let model = match embedding_config.model.as_str() {
                        "multilingual-e5-small" => EmbeddingModel::MultilingualE5Small,
                        other => EmbeddingModel::Custom(other.to_string()),
                    };
                    let llm_config = LlmEmbeddingConfig {
                        model,
                        show_download_progress: true,
                    };

                    // Run model loading on a blocking thread with a 30s timeout.
                    let provider: Option<Arc<dyn nevoflux_llm::EmbeddingProvider>> =
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(30),
                            tokio::task::spawn_blocking(move || FastEmbedProvider::new(llm_config)),
                        )
                        .await
                        {
                            Ok(Ok(Ok(p))) => {
                                info!(
                                    model = embedding_config.model.as_str(),
                                    "Embedding provider initialized"
                                );
                                Some(Arc::new(p) as _)
                            }
                            Ok(Ok(Err(e))) => {
                                warn!(
                                    "Embedding provider unavailable: {e}, semantic search disabled"
                                );
                                None
                            }
                            Ok(Err(e)) => {
                                warn!("Embedding task panicked: {e}, semantic search disabled");
                                None
                            }
                            Err(_) => {
                                warn!("Embedding init timed out (30s), semantic search disabled");
                                None
                            }
                        };

                    if let Some(ref provider) = provider {
                        // Publish to the shared slot so all consumers see it
                        if let Ok(mut slot) = embedding_slot.write() {
                            *slot = Some(Arc::clone(provider));
                        }

                        // Load existing memory embeddings into vector index
                        match db_arc.memory().list_with_embeddings(10_000) {
                            Ok(chunks) => {
                                if let Ok(mut idx) = vi.write() {
                                    for chunk in &chunks {
                                        if let Some(ref emb) = chunk.embedding {
                                            idx.add(&chunk.id, emb.clone());
                                        }
                                    }
                                    info!(
                                        count = chunks.len(),
                                        "Loaded memory embeddings into vector index"
                                    );
                                }
                            }
                            Err(e) => warn!("Failed to load memory embeddings: {e}"),
                        }

                        // Backfill entries that lack embeddings
                        backfill_embeddings(Arc::clone(provider), backfill_storage, vi).await;
                    }
                });
            } else {
                info!("Embedding provider disabled in config");
            }
        }
        #[cfg(not(feature = "embedding"))]
        {
            let _ = (embedding_slot, vi, db_arc, backfill_storage);
            info!("Embedding support not compiled in, semantic search disabled");
        }
    }

    // Load soul documents for system prompt injection
    let knowledge_retriever = {
        use crate::learning::retriever::KnowledgeRetriever;
        use crate::learning::soul::manager::SoulManager;

        let soul_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nevoflux");
        match SoulManager::init(&soul_dir).await {
            Ok(manager) => {
                let cache = manager.cache();
                info!(
                    "Loaded soul documents: IDENTITY={}B, SOUL={}B, USER={}B, TOOLS={}B, AGENTS={}B",
                    cache.identity_raw.len(),
                    cache.soul_raw.len(),
                    cache.user_raw.len(),
                    cache.tools_raw.len(),
                    cache.agents_raw.len(),
                );
                let cache = Arc::new(cache.clone());
                let storage = session_manager.shared_storage();
                let retriever = KnowledgeRetriever::new(cache, storage)
                    .with_embedding(Arc::clone(&shared_embedding));
                Some(Arc::new(retriever))
            }
            Err(e) => {
                warn!("Failed to load soul documents: {}, skipping", e);
                None
            }
        }
    };

    // Start soul document file watcher for detecting external edits
    {
        use crate::learning::soul::watcher::SoulWatcher;

        let soul_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nevoflux");
        match SoulWatcher::start(&soul_dir) {
            Ok(mut watcher) => {
                let retriever_for_watcher = knowledge_retriever.clone();
                tokio::spawn(async move {
                    while let Some(changed_path) = watcher.next_change().await {
                        let filename = changed_path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("unknown");
                        info!("Soul document changed externally: {}", filename);

                        // Reload the soul manager and update the retriever's cache
                        if let Some(ref retriever) = retriever_for_watcher {
                            match crate::learning::soul::manager::SoulManager::load(
                                watcher.soul_dir(),
                            )
                            .await
                            {
                                Ok(mut manager) => {
                                    retriever.update_soul_cache(manager.cache().clone());

                                    // Mark all sections in the changed file as manually
                                    // edited so that system promotions don't overwrite
                                    // user changes.
                                    manager.mark_file_manual(filename).await;

                                    info!(
                                        "Soul cache reloaded after external edit to {}",
                                        filename
                                    );
                                }
                                Err(e) => {
                                    warn!("Failed to reload soul documents after edit: {}", e);
                                }
                            }
                        }
                    }
                });
                info!("Soul file watcher started");
            }
            Err(e) => {
                warn!("Failed to start soul file watcher: {}", e);
            }
        }
    }

    // Initialize learning pipeline if enabled
    let learning_config = agent_config.read().unwrap().learning.clone();
    if learning_config.enabled {
        use crate::learning::buffer::MemoryBuffer;
        use crate::learning::collector::LearningCollector;
        use crate::learning::pipeline::{
            CategoryPromotionThresholds, LearningPipeline, PromotionThresholds,
            ValidationThresholds,
        };
        use crate::learning::sources::{
            MemoryChunkPreferenceSource, SiteAdaptationSource, ToolTraceLearningSource,
        };

        let shared_storage = session_manager.shared_storage();
        let buffer = Arc::new(MemoryBuffer::new(
            learning_config.flush_threshold,
            std::time::Duration::from_secs(learning_config.flush_interval_secs),
        ));
        let pipeline = Arc::new(LearningPipeline::new(
            Arc::clone(&buffer),
            Arc::clone(&shared_storage),
            Arc::clone(&shared_embedding),
        ));

        // Create collector with ToolTraceLearningSource and link to pipeline
        let mut collector = LearningCollector::new();
        collector.set_enabled(pipeline.enabled_flag());
        collector.set_rate_limit(learning_config.rate_limit_per_hour);
        collector.register_source(Box::new(ToolTraceLearningSource::new(Arc::clone(
            &shared_storage,
        ))));
        collector.register_source(Box::new(SiteAdaptationSource::new(Arc::clone(
            &shared_storage,
        ))));
        collector.register_source(Box::new(MemoryChunkPreferenceSource::new(Arc::clone(
            &shared_storage,
        ))));
        let collector = Arc::new(std::sync::Mutex::new(collector));

        let validation_thresholds = ValidationThresholds {
            min_occurrences: learning_config.validation.min_occurrences,
            min_confidence: learning_config.validation.min_confidence,
            min_alive_hours: learning_config.validation.min_alive_hours,
        };
        let promotion_thresholds = PromotionThresholds {
            batch_size: 50,
            min_alive_days: learning_config.promotion.min_alive_days,
            site_interaction: CategoryPromotionThresholds {
                min_hits: learning_config.promotion.site_interaction_min_hits,
                min_effectiveness: learning_config.promotion.site_interaction_min_effectiveness,
            },
            tool_optimization: CategoryPromotionThresholds {
                min_hits: learning_config.promotion.tool_optimization_min_hits,
                min_effectiveness: learning_config
                    .promotion
                    .tool_optimization_min_effectiveness,
            },
            user_preference: CategoryPromotionThresholds {
                min_hits: learning_config.promotion.user_preference_min_hits,
                min_effectiveness: 0.5,
            },
            hot_limit_site_interaction: 15,
            hot_limit_tool_optimization: 10,
            hot_limit_user_preference: 10,
        };

        let _soul_dir = learning_config
            .soul_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("nevoflux")
            });

        let flush_interval = learning_config.flush_interval_secs;

        // Spawn periodic collect → flush → validate → promote background task
        let pipeline_clone = Arc::clone(&pipeline);
        let buffer_clone = Arc::clone(&buffer);
        let collector_clone = Arc::clone(&collector);
        let shared_storage_clone = Arc::clone(&shared_storage);
        let agent_config_clone = agent_config.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(flush_interval));
            loop {
                interval.tick().await;

                if !pipeline_clone.is_enabled() {
                    continue;
                }

                // Collect entries from registered sources → buffer
                {
                    let entries = collector_clone.lock().unwrap().collect_all();
                    for entry in entries {
                        buffer_clone.insert(entry);
                    }
                }

                // Flush buffer to SQLite
                match pipeline_clone.flush() {
                    Ok(n) if n > 0 => info!("Learning pipeline flushed {} entries", n),
                    Err(e) => warn!("Learning pipeline flush error: {}", e),
                    _ => {}
                }

                // Validate pending entries
                match pipeline_clone.validate(&validation_thresholds) {
                    Ok(n) if n > 0 => info!("Learning pipeline validated {} entries", n),
                    Err(e) => warn!("Learning pipeline validate error: {}", e),
                    _ => {}
                }

                // Promote validated entries (less frequently - every 10th cycle)
                static PROMOTE_COUNTER: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let count = PROMOTE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count.is_multiple_of(10) {
                    match pipeline_clone.promote(&promotion_thresholds).await {
                        Ok(result) if result.promoted > 0 => {
                            info!(
                                "Learning pipeline promoted {} entries (skipped: threshold={})",
                                result.promoted, result.skipped_threshold
                            );
                        }
                        Err(e) => warn!("Learning pipeline promote error: {}", e),
                        _ => {}
                    }

                    // Check if any category needs consolidation (Auto-Dream)
                    let consolidator =
                        crate::learning::consolidator::KnowledgeConsolidator::new(0.8);
                    let hot_limits = vec![
                        (
                            "user_preference".to_string(),
                            promotion_thresholds.hot_limit_user_preference,
                        ),
                        (
                            "tool_optimization".to_string(),
                            promotion_thresholds.hot_limit_tool_optimization,
                        ),
                        (
                            "site_interaction".to_string(),
                            promotion_thresholds.hot_limit_site_interaction,
                        ),
                    ];

                    if let Some((category, limit)) = consolidator.category_needing_consolidation(
                        shared_storage_clone.database(),
                        &hot_limits,
                    ) {
                        let target =
                            crate::learning::consolidator::KnowledgeConsolidator::target_count(
                                limit,
                            );
                        let cons_config = agent_config_clone.read().unwrap().clone();
                        let cons_db = std::sync::Arc::new(shared_storage_clone.database().clone());
                        let cons_category = category.clone();
                        tokio::spawn(async move {
                            match crate::learning::consolidator::consolidate_category(
                                cons_config,
                                cons_db,
                                &cons_category,
                                target,
                            )
                            .await
                            {
                                Ok(r) => {
                                    info!(
                                        "Consolidated '{}': {} → {} entries",
                                        r.category, r.original_count, r.consolidated_count
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "Knowledge consolidation failed for '{}': {}",
                                        cons_category, e
                                    );
                                }
                            }
                        });
                    }
                }
            }
        });

        info!(
            "Learning pipeline started (flush_interval={}s, flush_threshold={})",
            flush_interval, learning_config.flush_threshold
        );
    } else {
        info!("Learning system disabled by config");
    }

    // Create agent role registry
    let role_registry = {
        let user_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
            .join("nevoflux")
            .join("agents");
        let builtin_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../builtin-wasm/prompts/agents");
        let mut registry = crate::agent::roles::AgentRoleRegistry::new(user_dir, builtin_dir);
        if let Err(e) = registry.scan() {
            tracing::warn!("Failed to scan agent roles: {}", e);
        } else {
            tracing::info!("Loaded {} agent roles", registry.list().len());
        }
        Arc::new(registry)
    };

    // Canvas Tool Whitelist registry
    let canvas_tool_registry = {
        use crate::canvas_tools::ToolWhitelistRegistry;
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nevoflux");
        let builtin_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("canvas-tools");
        let user_dir = config_dir.join("canvas-tools");
        let reg = Arc::new(ToolWhitelistRegistry::with_dirs(builtin_dir, user_dir));
        // Load tools from disk in a background task
        let bg_reg = Arc::clone(&reg);
        tokio::spawn(async move {
            bg_reg.load_from_disk().await;
            info!(
                "Canvas tool registry loaded: {} tools ({} enabled)",
                bg_reg.list_all().len(),
                bg_reg.list_enabled().len(),
            );
        });
        reg
    };

    // Canvas Share Service
    let canvas_share_service = {
        use crate::share::{CanvasShareService, ShareHttpClient};
        let storage_arc = session_manager.shared_storage();
        let http = ShareHttpClient::with_default_url().unwrap_or_else(|_| {
            // Fallback: use a dummy URL if construction fails
            ShareHttpClient::new("https://share.nevoflux.app").expect("valid fallback URL")
        });
        // Master key for local credential encryption — derived from config or
        // random fallback. For now, use a stable placeholder; production
        // should derive from user config.
        let master_key: [u8; 32] = {
            let mut k = [0u8; 32];
            k.copy_from_slice(&[0x42u8; 32]); // TODO: derive from config
            k
        };
        Arc::new(CanvasShareService::new(storage_arc, http, master_key))
    };

    // Canvas Persist Service (My Canvas)
    let canvas_persist_service = {
        let storage_arc = session_manager.shared_storage();
        Arc::new(crate::canvas_persist::CanvasPersistService::new(
            storage_arc,
        ))
    };

    // Shared skill registry — created once here and handed to both
    // CanvasVideoService (so T6/T7 can read templates) and HostServices
    // (which previously built its own internal copy).
    let shared_skills = {
        let mut registry = nevoflux_skills::SkillRegistry::new();
        if let Err(e) = registry.load() {
            tracing::warn!("canvas_video: failed to load skills: {}", e);
        }
        Arc::new(tokio::sync::RwLock::new(registry))
    };

    // Canvas Video Service (video render pipeline).
    // Wire in the EventBus so emit_progress / emit_succeeded / emit_failed
    // actually publish on jobs.render.{job_id}. Without this, subscribers
    // (sidebar, PoC gate test) never see terminal events even though the
    // render loop finishes and writes the MP4.
    let canvas_video_service = Arc::new(
        crate::canvas_video::CanvasVideoService::new()
            .with_event_bus(event_bus.clone())
            .with_storage(session_manager.shared_storage())
            .with_skills(shared_skills.clone()),
    );

    // Build HostServices first (without loop_manager), spin up LoopManager
    // with a `services` clone so its IterationExecutor can spawn a real
    // production agent (Phase 9c), then snap loop_manager back onto the
    // canonical services. The two have a chicken-and-egg dependency:
    // services needs loop_manager for the loop.* MCP dispatch path, and
    // loop_manager needs services for `Agent::run`. HostServices is `Clone`
    // and Arc-backed, so the round-trip is cheap.
    //
    // We also stash `agent_config` + the runtime Handle on services so
    // `IterationExecutor` can build a `DaemonHostFunctions` without
    // depending on the chat-session boot path (server.rs::handle_chat_send).
    // Shared session→proxy tracker so /loop iterations (which have no
    // inbound proxy of their own) can borrow the session's active sidebar
    // to fulfill browser_* tool calls.
    let session_proxy_tracker = Arc::new(crate::registry::SessionProxyTracker::new());

    let mut services = HostServices::with_skills(Arc::new(db.clone()), shared_skills)
        .with_browser_sender(browser_tx)
        .with_mcp_manager(mcp_manager)
        .with_shared_tool_search(tool_search_index)
        .with_vector_index(vector_index)
        .with_role_registry(role_registry)
        .with_embedding(Arc::clone(&shared_embedding))
        .with_canvas_video_service(canvas_video_service.clone())
        .with_tts_config(agent_config.read().unwrap().tts.clone())
        .with_agent_config(agent_config.read().unwrap().clone())
        .with_runtime_handle(tokio::runtime::Handle::current())
        .with_session_proxy_tracker(session_proxy_tracker.clone());

    // Construct the /loop skill's LoopManager and inject into HostServices
    // so the loop.* tool dispatcher (mcp_tool_executor + future direct-API
    // path) can resolve `services.loop_manager`. Spec §4 architecture.
    // Pass the (loop_manager-less) services clone so the IterationExecutor
    // gets a real `Agent::run` invocation path.
    let loop_manager = std::sync::Arc::new(crate::loops::LoopManager::start_with_bus(
        db.clone(),
        Some(event_bus.clone()),
        Some(services.clone()),
    ));
    // Publish process-global handle so IterationExecutor can back-fill
    // services.loop_manager when claude-code (ACP) calls loop.* via MCP.
    let _ = crate::loops::CURRENT_LOOP_MANAGER.set(loop_manager.clone());
    services = services.with_loop_manager(loop_manager.clone());
    if let Some(retriever) = knowledge_retriever {
        services = services.with_knowledge_retriever(retriever);
    }
    if let Some(computer) = crate::agent::computer_tools::create_computer() {
        services = services.with_computer_controller(Arc::new(computer));
        info!("Computer controller initialized");
    } else {
        warn!("Computer controller not available on this platform");
    }

    // Set LLM config on services so subagents can make LLM calls.
    {
        let config = agent_config.read().unwrap();
        if let (Some(provider_str), Some(api_key), Some(model)) = (
            config.llm.active_provider(),
            config.llm.active_api_key(),
            config.llm.active_model(),
        ) {
            if let Ok(provider) = provider_str.parse::<nevoflux_llm::ProviderType>() {
                let mut llm_config =
                    crate::wasm::services::LlmConfig::new(provider, api_key, model);
                if let Some(base_url) = config.llm.active_base_url() {
                    llm_config.base_url = Some(base_url.to_string());
                }
                services = services.with_llm(llm_config);
                info!(
                    "LLM config set on services: provider={:?}, model={}",
                    provider, model
                );
            }
        }
    }

    // Initialize subagent executor for MCP bridge mode (subagent_spawn tool).
    // Must pass services clone so subagents have access to LLM, browser, etc.
    let subagent_config = crate::config::SubagentConfig::default();
    let subagent_executor = Arc::new(
        crate::wasm::subagent::SubagentExecutor::new(
            subagent_config,
            tokio::runtime::Handle::current(),
        )
        .with_services(services.clone())
        .with_agent_config(agent_config.read().unwrap().clone()),
    );
    services = services.with_subagent_executor(subagent_executor);
    info!("Subagent executor initialized");

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (msg_tx, mut msg_rx) = mpsc::channel::<(Vec<u8>, ProxyEnvelope)>(100);
    let (response_tx, mut response_rx) = mpsc::channel::<(Vec<u8>, DaemonEnvelope)>(100);

    // Wire the response channel into HostServices so
    // `mcp_tool_executor::execute_canvas_video_tool` can emit the
    // canvas_video_open_render_tab broadcast on the MCP/ACP path. The
    // in-scope TCP-proxy canvas_video_render_start handler already has its
    // own direct broadcast; this one covers the LLM-driven tool call path.
    services = services.with_broadcast_tx(response_tx.clone());

    // Boot the Asset & Stream Plane HTTP server. Reuses the same port
    // range as the bridge — bridge takes its slot first, AssetServer
    // takes the next free one (per design D4 / §5.2). On bind failure
    // the daemon keeps running with `asset_server = None`; tools fall
    // back to NM-only (matches old-extension behavior).
    {
        use crate::asset_server::{AssetServer, AssetServerConfig};
        let asset_config = AssetServerConfig {
            port_range: config.port_start..(config.port_end.saturating_add(1)),
            // Phase 2 needs storage for /v1/composition/:id and the asset
            // GET handler. Phase 1's screenshot upload doesn't read this,
            // so passing it unconditionally is safe.
            storage: Some(services.database.clone()),
            // Phase 3 asset upload reuses CanvasVideoService::attach_asset
            // for resize + MIME sniff + dual-write — pass through the
            // already-constructed service handle.
            canvas_video_service: Some(canvas_video_service.clone()),
            ..Default::default()
        };
        match AssetServer::start(asset_config).await {
            Ok(server) => {
                info!(
                    bound_port = server.bound_port(),
                    "asset_server: Asset & Stream Plane online"
                );
                // Late-bind the AssetServer onto CanvasVideoService so
                // `load_composition` rewrites `assets/X` to /v1/asset/...
                // URLs (Phase 2) instead of inlining data URIs. The
                // service was constructed before this point and is
                // already Arc-wrapped, so set_asset_server uses interior
                // mutability (OnceLock).
                canvas_video_service.set_asset_server(server.clone());
                services = services.with_asset_server(server);
            }
            Err(e) => {
                warn!(
                    "asset_server: failed to start ({e}); tools requiring HTTP transport will fall back to NM-only"
                );
            }
        }
    }

    // Writer registry: maps proxy_id → writer half for routing responses
    type WriterMap = Arc<Mutex<HashMap<String, BufWriter<tokio::net::tcp::OwnedWriteHalf>>>>;
    let writers: WriterMap = Arc::new(Mutex::new(HashMap::new()));

    // Response writer task: receives (identity, DaemonEnvelope) and writes to correct proxy
    let writer_map = writers.clone();
    tokio::spawn(async move {
        while let Some((identity, response)) = response_rx.recv().await {
            let proxy_id = String::from_utf8_lossy(&identity).to_string();
            let msg_type = response
                .payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let Ok(data) = serde_json::to_vec(&response) else {
                error!(
                    "serde_json::to_vec failed for response to proxy {}: type={}",
                    proxy_id, msg_type
                );
                continue;
            };
            let len = data.len() as u32;

            // Broadcast: identity "*" fans the frame out to every currently
            // connected proxy. Used for daemon-initiated pushes that are
            // not tied to a specific requester (e.g. canvas_video_open_render_tab).
            if proxy_id == "*" {
                let mut map = writer_map.lock().await;
                let ids: Vec<String> = map.keys().cloned().collect();
                let mut dead: Vec<String> = Vec::new();
                for id in &ids {
                    if let Some(writer) = map.get_mut(id) {
                        let result = async {
                            writer.write_all(&len.to_le_bytes()).await?;
                            writer.write_all(&data).await?;
                            writer.flush().await?;
                            Ok::<(), std::io::Error>(())
                        }
                        .await;
                        match result {
                            Ok(()) => info!("Broadcast to proxy {}: type={}", id, msg_type),
                            Err(e) => {
                                error!("Broadcast to proxy {} failed: {}", id, e);
                                dead.push(id.clone());
                            }
                        }
                    }
                }
                for id in dead {
                    map.remove(&id);
                }
                continue;
            }

            let mut map = writer_map.lock().await;
            if let Some(writer) = map.get_mut(&proxy_id) {
                let result = async {
                    writer.write_all(&len.to_le_bytes()).await?;
                    writer.write_all(&data).await?;
                    writer.flush().await?;
                    Ok::<(), std::io::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        info!("Sent to proxy {}: type={}", proxy_id, msg_type);
                    }
                    Err(e) => {
                        error!("Failed to send to proxy {}: {}", proxy_id, e);
                        // Remove disconnected writer
                        map.remove(&proxy_id);
                    }
                }
            } else {
                warn!("No writer for proxy {}, dropping message", proxy_id);
            }
        }
    });

    // TCP accept loop: accepts connections and spawns per-connection reader tasks
    let accept_writers = writers.clone();
    let config_managed = config.managed;
    let config_idle_timeout = config.idle_timeout;
    let accept_shutdown_tx = shutdown_tx.clone();
    let shutdown_loop_manager = loop_manager.clone();
    tokio::spawn(async move {
        let last_message_time = Arc::new(Mutex::new(std::time::Instant::now()));

        // Idle check task for managed daemon
        if config_managed {
            let idle_last_message = last_message_time.clone();
            let idle_shutdown = accept_shutdown_tx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    let elapsed = idle_last_message.lock().await.elapsed();
                    if elapsed > config_idle_timeout {
                        info!(
                            "Managed daemon: idle for {:?}, self-terminating",
                            config_idle_timeout
                        );
                        let _ = idle_shutdown.send(()).await;
                        break;
                    }
                }
            });
        }

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            debug!("New TCP connection from {}", peer_addr);
                            if let Err(e) = stream.set_nodelay(true) {
                                warn!("Failed to set TCP_NODELAY: {}", e);
                            }

                            let msg_tx = msg_tx.clone();
                            let conn_writers = accept_writers.clone();
                            let last_msg = last_message_time.clone();

                            tokio::spawn(async move {
                                handle_proxy_connection(stream, msg_tx, conn_writers, last_msg).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept TCP connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Server shutdown signal received");
                    info!("Tearing down /loop skill subscriptions and pending iterations…");
                    shutdown_loop_manager.shutdown().await;
                    break;
                }
            }
        }
    });

    // Spawn browser request handler task
    // This task receives browser tool requests from the agent and sends them to the sidebar
    let browser_response_tx = response_tx.clone();
    let browser_registry_clone = browser_registry.clone();
    tokio::spawn(async move {
        while let Some((request, response_sender)) = browser_rx.recv().await {
            let request_id = request.request_id.clone();
            info!(
                "Browser request sending to sidebar: id={}, action={:?}, proxy_id={}, identity_len={}",
                request_id, request.action, request.proxy_id, request.client_identity.len()
            );

            // Store the response sender in the registry with timestamp
            {
                let mut registry = browser_registry_clone.lock().await;
                registry.insert(
                    request_id.clone(),
                    (std::time::Instant::now(), response_sender),
                );
                info!(
                    "Browser request registered: id={}, registry_size={}",
                    request_id,
                    registry.len()
                );
            }

            // Create BrowserToolRequest message to send to sidebar
            let browser_request = nevoflux_protocol::BrowserToolRequest {
                request_id: request.request_id,
                session_id: request.session_id,
                tab_id: request.tab_id,
                action: request.action,
                params: request.params,
                timeout_ms: request.timeout_ms,
            };

            // Wrap in AgentMessage and send
            let agent_message =
                nevoflux_protocol::AgentMessage::BrowserToolRequest(browser_request);
            let response_payload = serde_json::to_value(&agent_message).unwrap_or_default();

            // Send to the sidebar using the client identity from the request
            let response = DaemonEnvelope::new(&request.proxy_id, Channel::Chat, response_payload);
            if let Err(e) = browser_response_tx
                .send((request.client_identity, response))
                .await
            {
                error!("Failed to send browser request: {}", e);
            } else {
                info!("Browser request sent to response queue: id={}", request_id);
            }
        }
    });

    // Spawn periodic cleanup task for stale browser request registry entries.
    // Removes entries where the receiver has been dropped (agent cancelled) or
    // entries older than 10 minutes (response lost / sidebar disconnected).
    let cleanup_browser_registry = browser_registry.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut registry = cleanup_browser_registry.lock().await;
            let before = registry.len();
            registry.retain(|_id, (created_at, sender)| {
                !sender.is_closed() && created_at.elapsed() < std::time::Duration::from_secs(600)
            });
            let removed = before - registry.len();
            if removed > 0 {
                info!(
                    "Browser registry cleanup: removed {} stale entries, {} remaining",
                    removed,
                    registry.len()
                );
            }
        }
    });

    // Periodic cleanup for extraction registry.
    // Remove extractors for sessions idle longer than 1 hour.
    let cleanup_extraction_registry = extraction_registry.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        loop {
            interval.tick().await;
            let mut registry = cleanup_extraction_registry.lock().await;
            let before = registry.len();
            registry.retain(|_id, (last_access, _)| {
                last_access.elapsed() < std::time::Duration::from_secs(3600)
            });
            let removed = before - registry.len();
            if removed > 0 {
                info!(
                    "Extraction registry cleanup: removed {} stale entries, {} remaining",
                    removed,
                    registry.len()
                );
            }
        }
    });

    // --- P3: forward canvas_video lint requests to proxies ---------------
    // The CanvasVideoService publishes jobs:lint:request:{correlator} on the
    // EventBus when lint_composition runs.  We subscribe here and forward each
    // event as a canvas_video_lint_request TCP broadcast so the extension's
    // background handler (Task 22) can run the linter and reply with
    // canvas_video_lint_result.
    {
        let lint_bus = event_bus.clone();
        let lint_tx = response_tx.clone();
        tokio::spawn(async move {
            use crate::event_bus::types::TopicPattern;
            use crate::event_bus::{BackpressurePolicy, SubscriberIdentity};
            let pattern = TopicPattern::wildcard("jobs:lint:request:*");
            let mut sub = match lint_bus.subscribe(
                pattern,
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropOldest,
                64,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "canvas_video lint bus subscribe failed");
                    return;
                }
            };
            while let Some(delivery) = sub.rx.recv().await {
                let broadcast = serde_json::json!({
                    "type": "canvas_video_lint_request",
                    "payload": delivery.payload,
                });
                let env = DaemonEnvelope::broadcast(Channel::Chat, broadcast);
                let _ = lint_tx.send((b"*".to_vec(), env)).await;
            }
        });
    }

    // --- Inspect requests forwarded to proxies (mirror of lint above). ---
    // CanvasVideoService publishes jobs:inspect:request:{correlator}; we
    // subscribe and broadcast as canvas_video_inspect_request so the
    // extension's background handler renders the iframe + replies with
    // canvas_video_inspect_result.
    {
        let inspect_bus = event_bus.clone();
        let inspect_tx = response_tx.clone();
        tokio::spawn(async move {
            use crate::event_bus::types::TopicPattern;
            use crate::event_bus::{BackpressurePolicy, SubscriberIdentity};
            let pattern = TopicPattern::wildcard("jobs:inspect:request:*");
            let mut sub = match inspect_bus.subscribe(
                pattern,
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropOldest,
                64,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "canvas_video inspect bus subscribe failed");
                    return;
                }
            };
            while let Some(delivery) = sub.rx.recv().await {
                let broadcast = serde_json::json!({
                    "type": "canvas_video_inspect_request",
                    "payload": delivery.payload,
                });
                let env = DaemonEnvelope::broadcast(Channel::Chat, broadcast);
                let _ = inspect_tx.send((b"*".to_vec(), env)).await;
            }
        });
    }

    // Spawn message processing loop
    let process_router = router.clone();
    let process_response_tx = response_tx.clone();
    let process_config = agent_config.clone();
    let process_session_manager = session_manager.clone();
    let process_services = services.clone();
    let process_runtime = tokio::runtime::Handle::current();
    let process_browser_registry = browser_registry.clone();
    let process_cancellation_registry = cancellation_registry.clone();
    let process_interrupt_registry = interrupt_registry.clone();
    let process_plan_registry = plan_registry.clone();
    let process_tool_auth_registry = tool_auth_registry.clone();
    let process_extraction_registry = extraction_registry.clone();
    let process_event_bus = event_bus.clone();
    let process_subscription_router = subscription_router.clone();
    let process_trace_enabled = config.trace_enabled;
    let process_canvas_tool_registry = canvas_tool_registry.clone();
    let process_canvas_user_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nevoflux")
        .join("canvas-tools");
    let process_canvas_builtin_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("canvas-tools");
    let process_canvas_share_service = canvas_share_service.clone();
    let process_canvas_persist_service = canvas_persist_service.clone();
    let process_canvas_video_service = canvas_video_service.clone();
    tokio::spawn(async move {
        while let Some((identity, envelope)) = msg_rx.recv().await {
            let proxy_id = envelope.proxy_id.clone();
            let request_id = envelope.request_id.clone();
            let channel = envelope.channel;

            // Log all incoming messages
            let msg_type = envelope
                .payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            info!(
                "Message loop received: type={}, proxy_id={}, channel={:?}, identity_len={}",
                msg_type,
                proxy_id,
                channel,
                identity.len()
            );

            // Check for stop_generation messages - handle cancellation
            if msg_type == "stop_generation" {
                // Extract session_id from payload
                let session_id = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| p.get("session_id"))
                    .and_then(|s| s.as_str())
                    .unwrap_or_default();

                info!("Received stop_generation for session: {}", session_id);

                // Signal the agent to stop via interrupt flag
                {
                    let mut registry = process_interrupt_registry.lock().await;
                    if let Some(flag) = registry.remove(session_id) {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        info!("Set interrupt flag for session: {}", session_id);
                    }
                }

                // Cancel the active streaming session forwarder
                let cancelled = {
                    let mut registry = process_cancellation_registry.lock().await;
                    if let Some(token) = registry.remove(session_id) {
                        token.cancel();
                        true
                    } else {
                        false
                    }
                };

                // Send acknowledgment
                let response_payload = serde_json::json!({
                    "type": "agent_state",
                    "payload": {
                        "state": "idle",
                        "message": if cancelled { "Generation stopped" } else { "No active generation" },
                        "done": true
                    }
                });
                let response = DaemonEnvelope::new(&proxy_id, channel, response_payload)
                    .with_request_id(&request_id);
                if let Err(e) = process_response_tx.send((identity, response)).await {
                    error!("Failed to send stop_generation response: {}", e);
                }
                continue;
            }

            // Check for BrowserToolResponse messages
            if msg_type == "browser_tool_response" {
                info!("Processing browser_tool_response message");
                if let Some(payload) = envelope.payload.get("payload") {
                    if let Ok(response) = serde_json::from_value::<
                        nevoflux_protocol::BrowserToolResponse,
                    >(payload.clone())
                    {
                        let request_id = response.request_id.clone();
                        info!(
                            "Received browser tool response: id={}, success={}",
                            request_id, response.success
                        );

                        // Find the pending request and send the response
                        let sender = {
                            let mut registry = process_browser_registry.lock().await;
                            registry.remove(&request_id).map(|(_, sender)| sender)
                        };

                        if let Some(sender) = sender {
                            let browser_response = BrowserResponse {
                                request_id: response.request_id,
                                success: response.success,
                                result: response.result,
                                error: response.error,
                            };
                            if sender.send(browser_response).is_err() {
                                warn!("Failed to send browser response - receiver dropped");
                            } else {
                                info!("Browser response forwarded to agent");
                            }
                        } else {
                            warn!("No pending request for browser response: {}", request_id);
                        }
                        continue; // Don't process further
                    }
                }
            }

            // Check for PlanResponse messages from frontend
            if msg_type == "plan_response" {
                info!("Processing plan_response message");
                if let Some(payload) = envelope.payload.get("payload") {
                    // The sidebar sends PlanResponsePayload (object with session_id + response),
                    // not a bare PlanResponse string. Extract fields from the inner payload.
                    let session_id = payload
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Try parsing the "response" field as PlanResponse enum
                    let response = payload
                        .get("response")
                        .and_then(|v| serde_json::from_value::<PlanResponse>(v.clone()).ok());

                    if let Some(response) = response {
                        info!("Plan response: {:?} for session: {}", response, session_id);
                        if let Some(tx) = process_plan_registry.lock().await.remove(&session_id) {
                            let _ = tx.send(response);
                        } else {
                            warn!("No pending plan request for session: {}", session_id);
                        }
                    } else {
                        warn!("Failed to parse plan response from payload: {:?}", payload);
                    }
                }
                continue;
            }

            // Check for ToolAuthResponse messages from frontend
            if msg_type == "tool_auth_response" {
                info!("Processing tool_auth_response message");
                if let Some(payload) = envelope.payload.get("payload") {
                    if let Ok(response) =
                        serde_json::from_value::<ToolAuthResponse>(payload.clone())
                    {
                        let tool_id = response.tool_id.clone();
                        if let Some(tx) = process_tool_auth_registry.lock().await.remove(&tool_id) {
                            let _ = tx.send(response);
                        } else {
                            warn!("No pending tool auth request for tool_id: {}", tool_id);
                        }
                    }
                }
                continue;
            }

            // Handle loop_cancel_command from sidebar (/loop skill spec §8.3).
            // force=true is the second-click hard-cancel; false is the soft cancel
            // that lets the current iteration finish.
            if msg_type == "loop_cancel_command" {
                info!("Processing loop_cancel_command message");
                if let Some(payload) = envelope.payload.get("payload") {
                    let loop_id = payload
                        .get("loop_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let force = payload
                        .get("force")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if let Some(loop_id) = loop_id {
                        if let Some(mgr) = process_services.loop_manager.as_ref() {
                            let id = crate::loops::LoopId(loop_id.clone());
                            if let Err(e) = mgr.cancel_loop(&id, force).await {
                                warn!("loop cancel from sidebar failed for {}: {}", loop_id, e);
                            }
                        } else {
                            warn!(
                                "loop_cancel_command received for {} but no LoopManager configured",
                                loop_id
                            );
                        }
                    } else {
                        warn!("loop_cancel_command missing loop_id");
                    }
                }
                continue;
            }

            // Handle skill_command messages from sidebar — currently used
            // by the /loop slash command (Phase 17). Routes skill_name=="loop"
            // to LoopManager::create_loop. Other skill_names fall through and
            // get the slash-skill treatment via the chat path further down.
            if msg_type == "skill_command" {
                let payload = envelope.payload.get("payload");
                let skill_name = payload
                    .and_then(|p| p.get("skill_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Record session→proxy mapping from this skill_command so /loop
                // iterations spawned in this session can borrow a sidebar for
                // browser_* tool calls. Without this, a /loop created in a fresh
                // session (one that never sent a normal chat_message) cannot
                // resolve a sidebar proxy at iteration time and browser_* tools
                // hit "No writer for proxy , dropping message".
                if let Some(tracker) = process_services.session_proxy_tracker.as_ref() {
                    let sid = payload
                        .and_then(|p| p.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !sid.is_empty() {
                        tracker.note(sid, &proxy_id, &identity);
                    }
                }
                if skill_name == "loop" {
                    info!("Processing skill_command: loop");
                    let session_id = payload
                        .and_then(|p| p.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = payload
                        .and_then(|p| p.get("args"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    let trigger_expr = args
                        .get("trigger_expr")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if trigger_expr.is_empty() {
                        warn!("skill_command 'loop' missing trigger_expr");
                        continue;
                    }
                    let prompt_text = args
                        .get("prompt_text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let wrapped_skill = args
                        .get("wrapped_skill")
                        .filter(|v| !v.is_null())
                        .map(|v| v.to_string());
                    // Mode comes from the SkillCommandPayload's top-level
                    // `mode` field, populated by the sidebar from the current
                    // session's chat mode (chat/browser/agent).
                    let mode = payload
                        .and_then(|p| p.get("mode"))
                        .and_then(|v| v.as_str())
                        .map(parse_agent_mode)
                        .unwrap_or(AgentMode::Chat);

                    if let Some(mgr) = process_services.loop_manager.as_ref() {
                        match mgr
                            .create_loop(crate::loops::manager::CreateLoopArgs {
                                session_id,
                                trigger_expr_text: trigger_expr,
                                prompt_text,
                                wrapped_skill,
                                mode,
                            })
                            .await
                        {
                            Ok(id) => {
                                info!("Created loop {} from /loop slash command", id);
                            }
                            Err(e) => {
                                warn!("loop.create from /loop failed: {}", e);
                            }
                        }
                    } else {
                        warn!("skill_command 'loop' received but no LoopManager configured");
                    }
                    continue;
                }
                // Other skill_command names not handled here — fall through
                // (no `continue`) to the chat-message slash-skill path.
            }

            // Handle canvas_tool_list requests
            if msg_type == "canvas_tool_list" {
                info!("Processing canvas_tool_list message");
                let include_disabled = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| {
                        serde_json::from_value::<nevoflux_protocol::CanvasToolListRequest>(
                            p.clone(),
                        )
                        .ok()
                    })
                    .map(|req| req.include_disabled)
                    .unwrap_or(false);

                // Rescan canvas-tools directories so TOML files added after
                // daemon startup are picked up. This is what makes the
                // "I added it — Retry" button in canvas dialogs actually
                // work: the retry re-issues canvas_tool_list and the daemon
                // sees the newly-added file. Session-registered tools are
                // preserved by load_from_disk.
                process_canvas_tool_registry.load_from_disk().await;

                let tools = if include_disabled {
                    process_canvas_tool_registry.list_all()
                } else {
                    process_canvas_tool_registry.list_enabled()
                };

                let summaries: Vec<nevoflux_protocol::CanvasToolSummary> = tools
                    .iter()
                    .map(|t| {
                        let source_str = format!("{:?}", t.source).to_lowercase();
                        let is_override = process_canvas_tool_registry.is_override(&t.name);
                        nevoflux_protocol::CanvasToolSummary {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            kind: format!("{:?}", t.kind).to_lowercase(),
                            args_mode: Some(format!("{:?}", t.args_mode).to_lowercase()),
                            enabled: t.enabled,
                            source: source_str.clone(),
                            origin_source: source_str,
                            is_override,
                        }
                    })
                    .collect();

                let resp = nevoflux_protocol::AgentMessage::CanvasToolListResponse(
                    nevoflux_protocol::CanvasToolListResponse { tools: summaries },
                );
                let payload = serde_json::to_value(&resp).unwrap_or_default();
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_tool_get_raw — return the raw TOML text for the named tool.
            // Looks in the user dir first (for User/override entries), falls back to
            // the builtin dir. Session-source tools have no on-disk source; we report
            // `no_raw_for_session` in that case.
            if msg_type == "canvas_tool_get_raw" {
                info!("Processing canvas_tool_get_raw message");
                let req: Option<nevoflux_protocol::CanvasToolGetRawRequest> = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| serde_json::from_value(p.clone()).ok());

                let resp = match req {
                    Some(r) => {
                        let name = r.name;
                        let source = process_canvas_tool_registry
                            .get_any(&name)
                            .map(|t| t.source);

                        match source {
                            None => nevoflux_protocol::CanvasToolGetRawResponse {
                                success: false,
                                toml_text: None,
                                origin_source: None,
                                error: Some(nevoflux_protocol::CanvasToolError {
                                    code: "not_found".into(),
                                    message: format!("no tool named '{}'", name),
                                    field: None,
                                }),
                            },
                            Some(crate::canvas_tools::types::ToolSource::Session) => {
                                nevoflux_protocol::CanvasToolGetRawResponse {
                                    success: false,
                                    toml_text: None,
                                    origin_source: None,
                                    error: Some(nevoflux_protocol::CanvasToolError {
                                        code: "no_raw_for_session".into(),
                                        message: "session tools have no on-disk source".into(),
                                        field: None,
                                    }),
                                }
                            }
                            Some(src) => {
                                let dir = match src {
                                    crate::canvas_tools::types::ToolSource::User => {
                                        &process_canvas_user_dir
                                    }
                                    _ => &process_canvas_builtin_dir,
                                };
                                let path = dir.join(format!("{name}.toml"));
                                match std::fs::read_to_string(&path) {
                                    Ok(text) => nevoflux_protocol::CanvasToolGetRawResponse {
                                        success: true,
                                        toml_text: Some(text),
                                        origin_source: Some(format!("{:?}", src).to_lowercase()),
                                        error: None,
                                    },
                                    Err(e) => nevoflux_protocol::CanvasToolGetRawResponse {
                                        success: false,
                                        toml_text: None,
                                        origin_source: None,
                                        error: Some(nevoflux_protocol::CanvasToolError {
                                            code: "io".into(),
                                            message: format!("{}: {}", path.display(), e),
                                            field: None,
                                        }),
                                    },
                                }
                            }
                        }
                    }
                    None => nevoflux_protocol::CanvasToolGetRawResponse {
                        success: false,
                        toml_text: None,
                        origin_source: None,
                        error: Some(nevoflux_protocol::CanvasToolError {
                            code: "validation".into(),
                            message: "missing or malformed payload".into(),
                            field: None,
                        }),
                    },
                };

                let msg = nevoflux_protocol::AgentMessage::CanvasToolGetRawResponse(resp);
                let payload = serde_json::to_value(&msg).unwrap_or_default();
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            if msg_type == "canvas_tool_save" {
                info!("Processing canvas_tool_save message");
                let req: Option<nevoflux_protocol::CanvasToolSaveRequest> = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| serde_json::from_value(p.clone()).ok());

                let resp = (|| -> nevoflux_protocol::CanvasToolSaveResponse {
                    let req = match req {
                        Some(r) => r,
                        None => {
                            return nevoflux_protocol::CanvasToolSaveResponse {
                                success: false,
                                error: Some(nevoflux_protocol::CanvasToolError {
                                    code: "validation".into(),
                                    message: "missing or malformed payload".into(),
                                    field: None,
                                }),
                            }
                        }
                    };

                    // 1. Parse TOML.
                    let tool: crate::canvas_tools::types::CanvasTool =
                        match toml::from_str(&req.toml_text) {
                            Ok(t) => t,
                            Err(e) => {
                                return nevoflux_protocol::CanvasToolSaveResponse {
                                    success: false,
                                    error: Some(nevoflux_protocol::CanvasToolError {
                                        code: "toml_parse".into(),
                                        message: e.to_string(),
                                        field: None,
                                    }),
                                }
                            }
                        };

                    // 2. Validate semantics.
                    if let Err(ve) = crate::canvas_tools::validator::validate(&tool) {
                        return nevoflux_protocol::CanvasToolSaveResponse {
                            success: false,
                            error: Some(nevoflux_protocol::CanvasToolError {
                                code: ve.code.into(),
                                message: ve.message,
                                field: ve.field,
                            }),
                        };
                    }

                    // 3. Enforce expected_name (edit mode).
                    if let Some(expected) = &req.expected_name {
                        if expected != &tool.name {
                            return nevoflux_protocol::CanvasToolSaveResponse {
                                success: false,
                                error: Some(nevoflux_protocol::CanvasToolError {
                                    code: "name_changed".into(),
                                    message: format!(
                                        "renaming is not supported; expected '{expected}', found '{}'",
                                        tool.name
                                    ),
                                    field: Some("name".into()),
                                }),
                            };
                        }
                    } else {
                        // 4. New-mode only: reject collision with existing User tool.
                        // (A collision with a Builtin is allowed — that's the override path.)
                        if let Some(existing) = process_canvas_tool_registry.get_any(&tool.name) {
                            if existing.source == crate::canvas_tools::types::ToolSource::User {
                                return nevoflux_protocol::CanvasToolSaveResponse {
                                    success: false,
                                    error: Some(nevoflux_protocol::CanvasToolError {
                                        code: "name_conflict".into(),
                                        message: format!(
                                            "a user tool '{}' already exists",
                                            tool.name
                                        ),
                                        field: Some("name".into()),
                                    }),
                                };
                            }
                        }
                    }

                    // 5. Atomic write.
                    if let Err(e) = crate::canvas_tools::user_writer::write_user_tool_atomic(
                        &process_canvas_user_dir,
                        &tool.name,
                        &req.toml_text,
                    ) {
                        return nevoflux_protocol::CanvasToolSaveResponse {
                            success: false,
                            error: Some(nevoflux_protocol::CanvasToolError {
                                code: "io".into(),
                                message: e.to_string(),
                                field: None,
                            }),
                        };
                    }

                    // 6. Register in-memory as User source (shadowing any Builtin).
                    process_canvas_tool_registry.register_user_tool(tool);

                    nevoflux_protocol::CanvasToolSaveResponse {
                        success: true,
                        error: None,
                    }
                })();

                let msg = nevoflux_protocol::AgentMessage::CanvasToolSaveResponse(resp);
                let payload = serde_json::to_value(&msg).unwrap_or_default();
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            if msg_type == "canvas_tool_delete" {
                info!("Processing canvas_tool_delete message");
                let req: Option<nevoflux_protocol::CanvasToolDeleteRequest> = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| serde_json::from_value(p.clone()).ok());

                let resp = match req {
                    None => nevoflux_protocol::CanvasToolDeleteResponse {
                        success: false,
                        was_override: false,
                        error: Some(nevoflux_protocol::CanvasToolError {
                            code: "validation".into(),
                            message: "missing or malformed payload".into(),
                            field: None,
                        }),
                    },
                    Some(r) => {
                        let name = r.name;
                        let live = process_canvas_tool_registry.get_any(&name);
                        match live {
                            None => nevoflux_protocol::CanvasToolDeleteResponse {
                                success: false,
                                was_override: false,
                                error: Some(nevoflux_protocol::CanvasToolError {
                                    code: "not_found".into(),
                                    message: format!("no tool named '{}'", name),
                                    field: None,
                                }),
                            },
                            Some(t) if t.source != crate::canvas_tools::types::ToolSource::User => {
                                nevoflux_protocol::CanvasToolDeleteResponse {
                                    success: false,
                                    was_override: false,
                                    error: Some(nevoflux_protocol::CanvasToolError {
                                        code: "invalid_source".into(),
                                        message: "only user tools can be deleted".into(),
                                        field: None,
                                    }),
                                }
                            }
                            Some(_) => {
                                if let Err(e) =
                                    crate::canvas_tools::user_writer::delete_user_tool_file(
                                        &process_canvas_user_dir,
                                        &name,
                                    )
                                {
                                    nevoflux_protocol::CanvasToolDeleteResponse {
                                        success: false,
                                        was_override: false,
                                        error: Some(nevoflux_protocol::CanvasToolError {
                                            code: "io".into(),
                                            message: e.to_string(),
                                            field: None,
                                        }),
                                    }
                                } else {
                                    let outcome = process_canvas_tool_registry
                                        .remove_user_tool_with_restore(&name);
                                    nevoflux_protocol::CanvasToolDeleteResponse {
                                        success: true,
                                        was_override: outcome.restored_builtin,
                                        error: None,
                                    }
                                }
                            }
                        }
                    }
                };

                let msg = nevoflux_protocol::AgentMessage::CanvasToolDeleteResponse(resp);
                let payload = serde_json::to_value(&msg).unwrap_or_default();
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            if msg_type == "canvas_tool_validate" {
                info!("Processing canvas_tool_validate message");
                let req: Option<nevoflux_protocol::CanvasToolValidateRequest> = envelope
                    .payload
                    .get("payload")
                    .and_then(|p| serde_json::from_value(p.clone()).ok());

                let resp = match req {
                    None => nevoflux_protocol::CanvasToolValidateResponse {
                        success: false,
                        error: Some(nevoflux_protocol::CanvasToolError {
                            code: "validation".into(),
                            message: "missing or malformed payload".into(),
                            field: None,
                        }),
                    },
                    Some(r) => {
                        match toml::from_str::<crate::canvas_tools::types::CanvasTool>(&r.toml_text)
                        {
                            Err(e) => nevoflux_protocol::CanvasToolValidateResponse {
                                success: false,
                                error: Some(nevoflux_protocol::CanvasToolError {
                                    code: "toml_parse".into(),
                                    message: e.to_string(),
                                    field: None,
                                }),
                            },
                            Ok(tool) => match crate::canvas_tools::validator::validate(&tool) {
                                Err(ve) => nevoflux_protocol::CanvasToolValidateResponse {
                                    success: false,
                                    error: Some(nevoflux_protocol::CanvasToolError {
                                        code: ve.code.into(),
                                        message: ve.message,
                                        field: ve.field,
                                    }),
                                },
                                Ok(()) => nevoflux_protocol::CanvasToolValidateResponse {
                                    success: true,
                                    error: None,
                                },
                            },
                        }
                    }
                };

                let msg = nevoflux_protocol::AgentMessage::CanvasToolValidateResponse(resp);
                let payload = serde_json::to_value(&msg).unwrap_or_default();
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_tool_invoke requests (spawned as async task)
            if msg_type == "canvas_tool_invoke" {
                info!("Processing canvas_tool_invoke message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasToolInvokeRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let registry = process_canvas_tool_registry.clone();
                            let resp_tx = process_response_tx.clone();
                            let ident = identity.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            // Echo caller's call_id when supplied so they can correlate
                            // events back without owning the daemon's tracking.
                            let call_id = req.call_id.clone().unwrap_or_else(|| {
                                format!(
                                    "inv-{}",
                                    uuid::Uuid::new_v4()
                                        .to_string()
                                        .split('-')
                                        .next()
                                        .unwrap_or("0")
                                )
                            });

                            tokio::spawn(async move {
                                // Look up tool in registry
                                let tool = match registry.get(&req.tool_name) {
                                    Some(t) => t,
                                    None => {
                                        let resp = nevoflux_protocol::AgentMessage::CanvasToolInvokeResponse(
                                            nevoflux_protocol::CanvasToolInvokeResponse {
                                                tool_name: req.tool_name.clone(),
                                                success: false,
                                                stdout: None,
                                                stderr: None,
                                                exit_code: None,
                                                error: Some(format!("Tool not found or disabled: {}", req.tool_name)),
                                                duration_ms: 0,
                                                call_id: call_id.clone(),
                                            },
                                        );
                                        let payload =
                                            serde_json::to_value(&resp).unwrap_or_default();
                                        let env = DaemonEnvelope::new(&pid, Channel::Chat, payload)
                                            .with_request_id(&rid);
                                        let _ = resp_tx.send((ident, env)).await;
                                        return;
                                    }
                                };

                                // Send Started event
                                let started = nevoflux_protocol::AgentMessage::CanvasToolEvent(
                                    nevoflux_protocol::CanvasToolEvent::Started {
                                        call_id: call_id.clone(),
                                        tool_name: req.tool_name.clone(),
                                    },
                                );
                                let payload = serde_json::to_value(&started).unwrap_or_default();
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, payload);
                                let _ = resp_tx.send((ident.clone(), env)).await;

                                // Execute tool
                                let free_args = req.args.as_deref().unwrap_or(&[]);
                                let session_dir = std::env::temp_dir()
                                    .join(format!("nevoflux-canvas-{}", req.session_id));
                                // Ensure session dir exists
                                let _ = tokio::fs::create_dir_all(&session_dir).await;

                                // Set up a streaming channel and forwarder task that converts
                                // executor events into CanvasToolEvent::Stdout / Stderr messages
                                // and pushes them through the proxy as data arrives.
                                let (exec_evt_tx, mut exec_evt_rx) =
                                    tokio::sync::mpsc::channel::<
                                        crate::canvas_tools::executor::ExecutionEvent,
                                    >(64);

                                let fwd_call_id = call_id.clone();
                                let fwd_pid = pid.clone();
                                let fwd_resp_tx = resp_tx.clone();
                                let fwd_ident = ident.clone();
                                let forwarder = tokio::spawn(async move {
                                    while let Some(evt) = exec_evt_rx.recv().await {
                                        let cte = match evt {
                                            crate::canvas_tools::executor::ExecutionEvent::Stdout(data) => {
                                                nevoflux_protocol::CanvasToolEvent::Stdout {
                                                    call_id: fwd_call_id.clone(),
                                                    data,
                                                }
                                            }
                                            crate::canvas_tools::executor::ExecutionEvent::Stderr(data) => {
                                                nevoflux_protocol::CanvasToolEvent::Stderr {
                                                    call_id: fwd_call_id.clone(),
                                                    data,
                                                }
                                            }
                                        };
                                        let msg =
                                            nevoflux_protocol::AgentMessage::CanvasToolEvent(cte);
                                        let payload =
                                            serde_json::to_value(&msg).unwrap_or_default();
                                        let env =
                                            DaemonEnvelope::new(&fwd_pid, Channel::Chat, payload);
                                        if fwd_resp_tx.send((fwd_ident.clone(), env)).await.is_err()
                                        {
                                            break;
                                        }
                                    }
                                });

                                let result =
                                    crate::canvas_tools::executor::execute_whitelisted_tool_streaming(
                                        &tool,
                                        &req.params,
                                        free_args,
                                        &session_dir,
                                        exec_evt_tx,
                                    )
                                    .await;

                                // Wait for the forwarder to drain remaining events before we send
                                // the Finished event, so consumers see ordering: stdout/stderr
                                // chunks → finished.
                                let _ = forwarder.await;

                                let response = match result {
                                    Ok(exec_result) => {
                                        nevoflux_protocol::CanvasToolInvokeResponse {
                                            tool_name: req.tool_name.clone(),
                                            success: exec_result.success,
                                            stdout: if exec_result.stdout.is_empty() {
                                                None
                                            } else {
                                                Some(exec_result.stdout)
                                            },
                                            stderr: if exec_result.stderr.is_empty() {
                                                None
                                            } else {
                                                Some(exec_result.stderr)
                                            },
                                            exit_code: exec_result.exit_code,
                                            error: exec_result.error,
                                            duration_ms: exec_result.duration_ms,
                                            call_id: call_id.clone(),
                                        }
                                    }
                                    Err(e) => nevoflux_protocol::CanvasToolInvokeResponse {
                                        tool_name: req.tool_name.clone(),
                                        success: false,
                                        stdout: None,
                                        stderr: None,
                                        exit_code: None,
                                        error: Some(e.to_string()),
                                        duration_ms: 0,
                                        call_id: call_id.clone(),
                                    },
                                };

                                // Send Finished event
                                let finished = nevoflux_protocol::AgentMessage::CanvasToolEvent(
                                    nevoflux_protocol::CanvasToolEvent::Finished {
                                        call_id: call_id.clone(),
                                        success: response.success,
                                        exit_code: response.exit_code,
                                        duration_ms: response.duration_ms,
                                    },
                                );
                                let payload = serde_json::to_value(&finished).unwrap_or_default();
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, payload);
                                let _ = resp_tx.send((ident.clone(), env)).await;

                                // Send the final response
                                let resp =
                                    nevoflux_protocol::AgentMessage::CanvasToolInvokeResponse(
                                        response,
                                    );
                                let payload = serde_json::to_value(&resp).unwrap_or_default();
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, payload)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, env)).await;
                            });
                        }
                        Err(e) => {
                            warn!("Failed to parse CanvasToolInvokeRequest: {}", e);
                        }
                    }
                }
                continue;
            }

            // Handle canvas_share message: share an artifact.
            if msg_type == "canvas_share" {
                info!("Processing canvas_share message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasShareRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let svc = process_canvas_share_service.clone();
                            let resp_tx = process_response_tx.clone();
                            let ident = identity.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            tokio::spawn(async move {
                                let result = svc
                                    .share(&req.session_id, &req.artifact_id, req.ttl_secs)
                                    .await;
                                let resp_msg = match result {
                                    Ok(r) => serde_json::json!({
                                        "type": "canvas_share_response",
                                        "payload": {
                                            "share_id": r.share_id,
                                            "share_url": r.share_url,
                                            "password": r.password,
                                            "expires_at": r.expires_at,
                                        }
                                    }),
                                    Err(e) => serde_json::json!({
                                        "type": "error",
                                        "payload": {
                                            "code": "SHARE_FAILED",
                                            "message": e.to_string()
                                        }
                                    }),
                                };
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, resp_msg)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, env)).await;
                            });
                        }
                        Err(e) => warn!("Failed to parse CanvasShareRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle canvas_import message: import a shared canvas.
            if msg_type == "canvas_import" {
                info!("Processing canvas_import message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasImportRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let svc = process_canvas_share_service.clone();
                            let resp_tx = process_response_tx.clone();
                            let ident = identity.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            tokio::spawn(async move {
                                let result = svc
                                    .import(&req.session_id, &req.share_id, &req.password)
                                    .await;
                                let resp_msg = match result {
                                    Ok(r) => serde_json::json!({
                                        "type": "canvas_import_response",
                                        "payload": {
                                            "artifact_id": r.artifact_id,
                                            "artifact_name": r.artifact_name,
                                            "artifact_type": r.artifact_type,
                                            "imported_from_share_id": r.share_id,
                                        }
                                    }),
                                    Err(e) => {
                                        warn!(
                                            share_id = %req.share_id,
                                            "canvas_import failed: {:#}",
                                            e
                                        );
                                        serde_json::json!({
                                            "type": "error",
                                            "payload": {
                                                "code": "IMPORT_FAILED",
                                                "message": e.to_string()
                                            }
                                        })
                                    }
                                };
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, resp_msg)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, env)).await;
                            });
                        }
                        Err(e) => warn!("Failed to parse CanvasImportRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle canvas_share_extend message: extend a share's TTL.
            if msg_type == "canvas_share_extend" {
                info!("Processing canvas_share_extend message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasShareExtendRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let svc = process_canvas_share_service.clone();
                            let resp_tx = process_response_tx.clone();
                            let ident = identity.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            tokio::spawn(async move {
                                let result = svc.extend(&req.share_id, req.extend_secs).await;
                                let resp_msg = match result {
                                    Ok(expires_at) => serde_json::json!({
                                        "type": "canvas_share_extend_response",
                                        "payload": {
                                            "share_id": req.share_id,
                                            "expires_at": expires_at,
                                        }
                                    }),
                                    Err(e) => serde_json::json!({
                                        "type": "error",
                                        "payload": {
                                            "code": "EXTEND_FAILED",
                                            "message": e.to_string()
                                        }
                                    }),
                                };
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, resp_msg)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, env)).await;
                            });
                        }
                        Err(e) => warn!("Failed to parse CanvasShareExtendRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle canvas_share_delete message: delete a share.
            if msg_type == "canvas_share_delete" {
                info!("Processing canvas_share_delete message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasShareDeleteRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let svc = process_canvas_share_service.clone();
                            let resp_tx = process_response_tx.clone();
                            let ident = identity.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            tokio::spawn(async move {
                                let result = svc.delete(&req.share_id).await;
                                let resp_msg = match result {
                                    Ok(()) => serde_json::json!({
                                        "type": "canvas_share_delete_response",
                                        "payload": {
                                            "share_id": req.share_id,
                                            "success": true,
                                        }
                                    }),
                                    Err(e) => serde_json::json!({
                                        "type": "error",
                                        "payload": {
                                            "code": "DELETE_FAILED",
                                            "message": e.to_string()
                                        }
                                    }),
                                };
                                let env = DaemonEnvelope::new(&pid, Channel::Chat, resp_msg)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, env)).await;
                            });
                        }
                        Err(e) => warn!("Failed to parse CanvasShareDeleteRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle canvas_share_list message: list active shares (sync).
            if msg_type == "canvas_share_list" {
                info!("Processing canvas_share_list message");
                if let Some(inner) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::CanvasShareListRequest>(
                        inner.clone(),
                    ) {
                        Ok(req) => {
                            let result = process_canvas_share_service.list(&req.session_id);
                            let resp_msg = match result {
                                Ok(shares) => {
                                    let infos: Vec<nevoflux_protocol::CanvasShareInfo> = shares
                                        .into_iter()
                                        .map(|s| nevoflux_protocol::CanvasShareInfo {
                                            artifact_id: s.artifact_id,
                                            share_id: s.share_id,
                                            share_url: s.share_url,
                                            expires_at: s.expires_at,
                                            view_count: s.view_count,
                                            created_at: s.created_at,
                                        })
                                        .collect();
                                    serde_json::json!({
                                        "type": "canvas_share_list_response",
                                        "payload": {
                                            "shares": infos,
                                        }
                                    })
                                }
                                Err(e) => serde_json::json!({
                                    "type": "error",
                                    "payload": {
                                        "code": "LIST_FAILED",
                                        "message": e.to_string()
                                    }
                                }),
                            };
                            let env = DaemonEnvelope::new(&proxy_id, Channel::Chat, resp_msg)
                                .with_request_id(&request_id);
                            let _ = process_response_tx.send((identity.clone(), env)).await;
                        }
                        Err(e) => warn!("Failed to parse CanvasShareListRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle canvas_persist_list request: list My Canvas artifacts.
            if msg_type == "canvas_persist_list" {
                info!("Processing canvas_persist_list message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_persist::handle(
                    &process_canvas_persist_service,
                    msg_type,
                    payload,
                ) {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_persist_list_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_PERSIST_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_persist_save request: promote an artifact to persistent.
            if msg_type == "canvas_persist_save" {
                info!("Processing canvas_persist_save message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_persist::handle(
                    &process_canvas_persist_service,
                    msg_type,
                    payload,
                ) {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_persist_save_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_PERSIST_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_persist_rename request: rename a persistent artifact.
            if msg_type == "canvas_persist_rename" {
                info!("Processing canvas_persist_rename message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_persist::handle(
                    &process_canvas_persist_service,
                    msg_type,
                    payload,
                ) {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_persist_rename_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_PERSIST_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_persist_delete request: delete a persistent artifact.
            if msg_type == "canvas_persist_delete" {
                info!("Processing canvas_persist_delete message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_persist::handle(
                    &process_canvas_persist_service,
                    msg_type,
                    payload,
                ) {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_persist_delete_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_PERSIST_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_create_composition request.
            if msg_type == "canvas_video_create_composition" {
                info!("Processing canvas_video_create_composition message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_video_create_composition_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_render_start request.
            if msg_type == "canvas_video_render_start" {
                info!("Processing canvas_video_render_start message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_video_render_start_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                // If the render job was successfully created, broadcast a
                // canvas_video_open_render_tab frame to all connected proxies.
                // The extension listens for this and opens the
                // nevoflux://render/{job_id} tab; other proxies ignore it.
                // Without this, a render_start initiated by anyone other
                // than the extension (e.g. the PoC gate test proxy) would
                // have no way to cause the render page to load.
                if let Some(job_id) = resp_msg
                    .get("type")
                    .and_then(|t| t.as_str())
                    .filter(|t| *t == "canvas_video_render_start_response")
                    .and_then(|_| resp_msg.get("payload"))
                    .and_then(|p| p.get("job_id"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                {
                    let broadcast_payload = serde_json::json!({
                        "type": "canvas_video_open_render_tab",
                        "payload": { "job_id": job_id }
                    });
                    let broadcast_env = DaemonEnvelope::broadcast(channel, broadcast_payload);
                    let _ = process_response_tx
                        .send((b"*".to_vec(), broadcast_env))
                        .await;
                }
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_render_cancel request.
            if msg_type == "canvas_video_render_cancel" {
                info!("Processing canvas_video_render_cancel message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_video_render_cancel_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_lint_composition request.
            if msg_type == "canvas_video_lint_composition" {
                info!("Processing canvas_video_lint_composition message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(resp_json) => serde_json::json!({
                        "type": "canvas_video_lint_composition_response",
                        "payload": resp_json
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_lint_result — the extension's reply to a
            // broadcast lint request. Resolves the correlator's oneshot.
            if msg_type == "canvas_video_lint_result" {
                info!("Processing canvas_video_lint_result message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let correlator = payload
                    .get("job_correlator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let report: nevoflux_protocol::canvas_video::LintReport = payload
                    .get("report")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();
                process_canvas_video_service
                    .on_lint_result(&correlator, report)
                    .await;
                // No response — fire-and-forget.
                continue;
            }

            // Handle canvas_video_inspect_result — extension's reply to a
            // broadcast inspect request. Mirrors the lint path.
            if msg_type == "canvas_video_inspect_result" {
                info!("Processing canvas_video_inspect_result message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let correlator = payload
                    .get("job_correlator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let report: nevoflux_protocol::canvas_video::InspectReport = payload
                    .get("report")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();
                process_canvas_video_service
                    .on_inspect_result(&correlator, report)
                    .await;
                continue;
            }

            // Handle canvas_video_reveal_path — sidebar asks daemon to play or
            // reveal a rendered MP4 via the OS default app. Fire-and-forget
            // from the sidebar's POV, but we return a success/error response
            // so the card can show a toast on failure.
            if msg_type == "canvas_video_reveal_path" {
                info!("Processing canvas_video_reveal_path message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match serde_json::from_value::<
                    nevoflux_protocol::canvas_video::RevealPathRequest,
                >(payload)
                {
                    Ok(req) => match crate::canvas_video::reveal::reveal_path(req) {
                        Ok(r) => serde_json::json!({
                            "type": "canvas_video_reveal_path_response",
                            "payload": r,
                        }),
                        Err(e) => serde_json::json!({
                            "type": "error",
                            "payload": {"code":"CANVAS_VIDEO_ERROR","message":e.to_string()}
                        }),
                    },
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": format!("invalid canvas_video_reveal_path payload: {e}")
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Handle canvas_video_ready notification (extension -> daemon).
            if msg_type == "canvas_video_ready" {
                info!("Processing canvas_video_ready message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let _ = crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await;
                // No response needed — fire-and-forget.
                continue;
            }

            // Handle canvas_video_frame_chunk notification (extension -> daemon).
            if msg_type == "canvas_video_frame_chunk" {
                info!("Processing canvas_video_frame_chunk message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let _ = crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await;
                // No response needed — fire-and-forget.
                continue;
            }

            // Page-driven render complete (extension -> daemon).
            if msg_type == "canvas_video_render_done" {
                info!("Processing canvas_video_render_done message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let _ = crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await;
                continue;
            }

            // Page-driven render failure (extension -> daemon).
            if msg_type == "canvas_video_render_failed" {
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                // Surface the page's actual error message so we don't
                // have to chase the bridge end. Without this we get a
                // dry "Processing canvas_video_render_failed" line and
                // nothing else — the failure cause stays opaque.
                let job_id = payload
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let error = payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no error field in payload)");
                let frames_emitted = payload
                    .get("frames_emitted")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                tracing::warn!(
                    job_id = %job_id,
                    frames_emitted = %frames_emitted,
                    error = %error,
                    "canvas_video_render_failed (page-driven)",
                );
                let _ = crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await;
                continue;
            }

            // Page fetches composition HTML + spec for its job (extension -> daemon).
            if msg_type == "canvas_video_get_composition" {
                info!("Processing canvas_video_get_composition message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(val) => serde_json::json!({
                        "type": "canvas_video_get_composition_response",
                        "payload": val,
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Canvas Editor / preview fetches composition HTML by id
            // (asset-stream-plane Phase 2 URL-rewritten path; sibling of
            // canvas_video_get_composition but no job indirection).
            if msg_type == "canvas_video_load_composition_html" {
                info!("Processing canvas_video_load_composition_html message");
                let payload = envelope
                    .payload
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let resp_msg = match crate::canvas_video::handlers::handle(
                    &process_canvas_video_service,
                    msg_type,
                    payload,
                )
                .await
                {
                    Ok(val) => serde_json::json!({
                        "type": "canvas_video_load_composition_html_response",
                        "payload": val,
                    }),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "CANVAS_VIDEO_ERROR",
                            "message": e.to_string()
                        }
                    }),
                };
                let response =
                    DaemonEnvelope::new(&proxy_id, channel, resp_msg).with_request_id(&request_id);
                let _ = process_response_tx.send((identity, response)).await;
                continue;
            }

            // Check for EventBus request messages from frontend
            if msg_type == "events_request" {
                info!("Processing events_request message");
                if let Some(payload) = envelope.payload.get("payload") {
                    match serde_json::from_value::<nevoflux_protocol::EventBusRequest>(
                        payload.clone(),
                    ) {
                        Ok(request) => {
                            let eb = process_event_bus.clone();
                            let sub_router = process_subscription_router.clone();
                            let resp_tx = process_response_tx.clone();
                            let pid = proxy_id.clone();
                            let rid = request_id.clone();
                            let ident = identity.clone();

                            tokio::spawn(async move {
                                let response = handle_event_bus_request(
                                    request,
                                    &eb,
                                    &sub_router,
                                    &pid,
                                    &ident,
                                    resp_tx.clone(),
                                )
                                .await;
                                let msg = nevoflux_protocol::AgentMessage::EventsResponse(response);
                                let payload = serde_json::to_value(&msg).unwrap_or_default();
                                let envelope = DaemonEnvelope::new(&pid, Channel::Chat, payload)
                                    .with_request_id(&rid);
                                let _ = resp_tx.send((ident, envelope)).await;
                            });
                        }
                        Err(e) => warn!("Failed to parse EventBusRequest: {}", e),
                    }
                }
                continue;
            }

            // Handle internal proxy disconnect notification for EventBus cleanup.
            //
            // PREVIOUS BEHAVIOR (removed): synchronously cleaned subs by
            // proxy_id. The bug: native-messaging often early-EOFs at
            // boot — the underlying TCP bridge cycles while the
            // WebExtension keeps using the same proxy_id. The OLD
            // connection's EOF arrives at the daemon AFTER the NEW
            // connection has already registered (and possibly already
            // re-subscribed via `replaySubscriptions`). Cleanup-by-
            // proxy_id then nukes the just-added subs, and downstream
            // subscribers (sidebar render-progress, etc.) silently stop
            // receiving events.
            //
            // We can't reliably distinguish "old conn's stale disconnect"
            // from "true disconnect of currently-only conn" without
            // per-connection identity tracking (the writers map is also
            // keyed by proxy_id and gets clobbered the same way). For the
            // single-browser-session use case, zombie subscriptions left
            // behind by a truly-gone proxy are harmless — they take a
            // negligible amount of memory and clear when the daemon
            // exits. The LLM client / sidebar / canvas page all live for
            // the duration of the daemon process anyway.
            //
            // Bigger architectural fix (per-connection identity → cleanup
            // by identity instead of proxy_id) is tracked separately. For
            // now, just NEVER auto-clean subs on disconnect.
            if msg_type == "_proxy_disconnected" {
                let _disconnected_id = envelope
                    .payload
                    .get("proxy_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                continue;
            }

            // Register proxy if not already registered (pid 0 for native messaging)
            if !process_router.proxy_registry().is_registered(&proxy_id) {
                process_router.proxy_registry().register(&proxy_id, 0);
                debug!("Registered new proxy: {}", proxy_id);
            }

            // Route the message
            let decision = process_router.route_incoming(&envelope);
            debug!("Route decision for {}: {:?}", proxy_id, decision);

            // Process based on route decision
            match decision {
                RouteDecision::RejectUnregistered => {
                    let response_payload = serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "UNREGISTERED",
                            "message": "Proxy not registered"
                        }
                    });
                    let response = DaemonEnvelope::new(&proxy_id, channel, response_payload)
                        .with_request_id(&request_id);
                    if let Err(e) = process_response_tx.send((identity, response)).await {
                        error!("Failed to queue response: {}", e);
                    }
                }
                RouteDecision::ProcessChat { .. } => {
                    // Handle chat messages via Agent with streaming support
                    // IMPORTANT: Spawn as a separate task to avoid blocking the message loop
                    // This allows browser_tool_response messages to be processed while
                    // the agent is waiting for browser tool results.
                    let payload = envelope.payload.clone();
                    let config = process_config.read().unwrap().clone();
                    let shared_config = process_config.clone();
                    let session_manager = process_session_manager.clone();
                    let services = process_services.clone();
                    let runtime = process_runtime.clone();
                    let response_tx = process_response_tx.clone();
                    let cancellation_registry = process_cancellation_registry.clone();
                    let interrupt_registry = process_interrupt_registry.clone();
                    let plan_registry = process_plan_registry.clone();
                    let trace_enabled = process_trace_enabled;
                    let extraction_registry = process_extraction_registry.clone();
                    let canvas_video_service = process_canvas_video_service.clone();
                    tokio::spawn(async move {
                        handle_chat_message_streaming(
                            &payload,
                            &config,
                            &shared_config,
                            &session_manager,
                            &services,
                            runtime,
                            identity,
                            proxy_id,
                            request_id,
                            channel,
                            response_tx,
                            cancellation_registry,
                            interrupt_registry,
                            plan_registry,
                            trace_enabled,
                            extraction_registry,
                            canvas_video_service,
                        )
                        .await;
                    });
                }
                RouteDecision::ProcessMcp { .. } => {
                    // Handle MCP messages
                    let response_payload = handle_mcp_message(&envelope.payload).await;
                    let response = DaemonEnvelope::new(&proxy_id, channel, response_payload)
                        .with_request_id(&request_id);
                    if let Err(e) = process_response_tx.send((identity, response)).await {
                        error!("Failed to queue response: {}", e);
                    }
                }
            }
        }
    });

    Ok(Server {
        port,
        shutdown_tx: Some(shutdown_tx),
    })
}

/// Background task to generate embeddings for existing entries that lack them.
///
/// Runs at startup with a small delay, backfilling MemoryChunks and Knowledge entries
/// that were created before embedding was enabled. Stops on the first embedding error
/// (the provider may be unavailable) and yields between items to avoid blocking the runtime.
async fn backfill_embeddings(
    provider: Arc<dyn nevoflux_llm::EmbeddingProvider>,
    storage: Arc<nevoflux_storage::Storage>,
    vector_index: Arc<std::sync::RwLock<nevoflux_storage::SimpleVectorIndex>>,
) {
    // Small delay to let startup complete
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Backfill MemoryChunks
    match storage.database().memory().list_without_embeddings(1000) {
        Ok(chunks) => {
            let mut count = 0;
            for chunk in chunks {
                match provider.embed(&chunk.content).await {
                    Ok(emb) => {
                        if storage
                            .database()
                            .memory()
                            .update_embedding(&chunk.id, &emb)
                            .is_ok()
                        {
                            if let Ok(mut idx) = vector_index.write() {
                                idx.add(&chunk.id, emb);
                            }
                            count += 1;
                        }
                    }
                    Err(e) => {
                        debug!(chunk_id = %chunk.id, error = %e, "Memory backfill embedding failed");
                        break; // Provider may be unavailable
                    }
                }
                // Small yield to avoid blocking
                tokio::task::yield_now().await;
            }
            if count > 0 {
                info!(count, "Backfilled memory chunk embeddings");
            }
        }
        Err(e) => warn!(error = %e, "Failed to query chunks for backfill"),
    }

    // Backfill Knowledge entries
    match storage.knowledge().list_without_embeddings(1000) {
        Ok(entries) => {
            let mut count = 0;
            for entry in entries {
                let text = format!("{} {}", entry.summary, entry.details);
                match provider.embed(&text).await {
                    Ok(emb) => {
                        if storage
                            .knowledge()
                            .update_embedding(&entry.id, &emb)
                            .is_ok()
                        {
                            count += 1;
                        }
                    }
                    Err(e) => {
                        debug!(entry_id = %entry.id, error = %e, "Knowledge backfill embedding failed");
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
            if count > 0 {
                info!(count, "Backfilled knowledge embeddings");
            }
        }
        Err(e) => warn!(error = %e, "Failed to query knowledge for backfill"),
    }
}

/// Handle an EventBus request from a proxy and return the corresponding response.
///
/// Dispatches Subscribe, Unsubscribe, Publish, and History requests.
/// For subscriptions, spawns a delivery forwarder task that relays events
/// back to the originating proxy.
async fn handle_event_bus_request(
    request: nevoflux_protocol::EventBusRequest,
    event_bus: &Arc<crate::event_bus::EventBus>,
    subscription_router: &SubscriptionRouter,
    proxy_id: &str,
    identity: &[u8],
    response_tx: mpsc::Sender<(Vec<u8>, DaemonEnvelope)>,
) -> nevoflux_protocol::EventBusResponse {
    use crate::event_bus::*;
    use nevoflux_protocol::events::*;

    match request {
        EventBusRequest::Subscribe(opts) => {
            if opts.patterns.is_empty() {
                return EventBusResponse::Error {
                    code: "SUBSCRIBE_FAILED".into(),
                    message: "no patterns supplied".into(),
                };
            }

            let mut sub_ids: Vec<String> = Vec::with_capacity(opts.patterns.len());
            let mut first_error: Option<(String, String)> = None; // (pattern, err)

            for pattern_str in &opts.patterns {
                let pattern = if pattern_str.contains('*') {
                    TopicPattern::wildcard(pattern_str)
                } else {
                    TopicPattern::exact(pattern_str)
                };
                let pattern_dbg = format!("{:?}", pattern);
                let subscriber = SubscriberIdentity::Extension {
                    proxy_id: proxy_id.to_string(),
                };

                match event_bus.subscribe_with_options(
                    pattern,
                    subscriber,
                    BackpressurePolicy::DropOldest,
                    opts.buffer_size,
                    opts.replay_sticky,
                ) {
                    Ok(mut sub_handle) => {
                        tracing::info!(
                            pattern = %pattern_dbg,
                            proxy = %proxy_id,
                            sub = %sub_handle.id,
                            "EventBus subscribe OK",
                        );
                        let sub_id = sub_handle.id.clone();
                        let cancel_token = tokio_util::sync::CancellationToken::new();
                        let token_clone = cancel_token.clone();
                        let fwd_proxy_id = proxy_id.to_string();
                        let fwd_identity = identity.to_vec();
                        let fwd_response_tx = response_tx.clone();
                        let fwd_sub_id = sub_id.clone();

                        // Forwarder per subscription (verbatim lift from the old single-pattern path)
                        tokio::spawn(async move {
                            loop {
                                tokio::select! {
                                    _ = token_clone.cancelled() => break,
                                    event = sub_handle.rx.recv() => {
                                        match event {
                                            Some(bus_event) => {
                                                let delivery = EventBusDelivery {
                                                    subscription_id: fwd_sub_id.clone(),
                                                    event: BusEventPayload {
                                                        event_id: bus_event.id.clone(),
                                                        topic: bus_event.topic.clone(),
                                                        payload: bus_event.payload.clone(),
                                                        delivery: match bus_event.delivery {
                                                            Delivery::Ephemeral => DeliveryMode::Ephemeral,
                                                            Delivery::Sticky => DeliveryMode::Sticky,
                                                            Delivery::Persistent => DeliveryMode::Persistent {
                                                                ttl_secs: bus_event.ttl.map(|d| d.as_secs()),
                                                            },
                                                        },
                                                        publisher: format!("{:?}", bus_event.publisher),
                                                        timestamp_ms: bus_event.created_at.timestamp_millis() as u64,
                                                    },
                                                };
                                                let msg = nevoflux_protocol::AgentMessage::EventsDelivery(delivery);
                                                let payload = serde_json::to_value(&msg).unwrap_or_default();
                                                let env = DaemonEnvelope::new(
                                                    &fwd_proxy_id,
                                                    Channel::Chat,
                                                    payload,
                                                );
                                                if fwd_response_tx
                                                    .send((fwd_identity.clone(), env))
                                                    .await
                                                    .is_err()
                                                {
                                                    break;
                                                }
                                            }
                                            None => break,
                                        }
                                    }
                                }
                            }
                        });

                        subscription_router.lock().await.insert(
                            sub_id.clone(),
                            SubscriptionEntry {
                                proxy_id: proxy_id.to_string(),
                                identity: identity.to_vec(),
                                cancel_token,
                            },
                        );
                        sub_ids.push(sub_id);
                    }
                    Err(e) => {
                        tracing::warn!(
                            pattern = %pattern_dbg,
                            proxy = %proxy_id,
                            error = %e,
                            "EventBus subscribe DENIED/FAILED",
                        );
                        if first_error.is_none() {
                            first_error = Some((pattern_str.clone(), e.to_string()));
                        }
                    }
                }
            }

            // If at least one pattern subscribed successfully, return the first
            // sub_id as the caller-visible "group anchor". Remaining sub_ids
            // stay registered in subscription_router under their own ids; they
            // get cleaned up independently on proxy disconnect. For explicit
            // Unsubscribe, caller only removes the group anchor — this is a
            // known shortcoming (tracked as a future cleanup), but acceptable
            // because per-proxy cleanup catches everything on disconnect.
            if let Some(anchor) = sub_ids.first().cloned() {
                EventBusResponse::Subscribed {
                    subscription_id: anchor,
                    patterns: opts.patterns,
                }
            } else {
                let (pat, msg) =
                    first_error.unwrap_or_else(|| ("?".to_string(), "unknown".to_string()));
                EventBusResponse::Error {
                    code: "SUBSCRIBE_FAILED".into(),
                    message: format!("all patterns failed; first: {} — {}", pat, msg),
                }
            }
        }

        EventBusRequest::Unsubscribe { subscription_id } => {
            if let Some(entry) = subscription_router.lock().await.remove(&subscription_id) {
                entry.cancel_token.cancel();
            }
            event_bus.unsubscribe(&subscription_id);
            EventBusResponse::Unsubscribed { subscription_id }
        }

        EventBusRequest::Publish(opts) => {
            let publisher = PublisherIdentity::Extension {
                proxy_id: proxy_id.to_string(),
            };
            tracing::info!(
                topic = %opts.topic,
                proxy = %proxy_id,
                delivery = ?opts.delivery,
                "EventBus publish received"
            );
            let event = match opts.delivery {
                DeliveryMode::Ephemeral => {
                    BusEvent::ephemeral(opts.topic.clone(), opts.payload, publisher)
                }
                DeliveryMode::Sticky => {
                    BusEvent::sticky(opts.topic.clone(), opts.payload, publisher)
                }
                DeliveryMode::Persistent { ttl_secs } => BusEvent::persistent(
                    opts.topic.clone(),
                    opts.payload,
                    publisher,
                    ttl_secs.map(std::time::Duration::from_secs),
                ),
            };
            let event_id = event.id.clone();
            match event_bus.publish(event).await {
                Ok(()) => EventBusResponse::Published { event_id },
                Err(e) => EventBusResponse::Error {
                    code: "PUBLISH_FAILED".into(),
                    message: e.to_string(),
                },
            }
        }

        EventBusRequest::History(_query) => {
            // History queries require direct SQLite access -- defer to v2
            EventBusResponse::Error {
                code: "NOT_IMPLEMENTED".into(),
                message: "History queries not yet implemented".into(),
            }
        }
    }
}

/// Clean up all EventBus subscriptions belonging to a disconnected proxy.
///
/// Cancels the delivery forwarder tasks and removes the subscriptions from
/// both the router and the EventBus itself.
async fn cleanup_proxy_subscriptions(
    proxy_id: &str,
    subscription_router: &SubscriptionRouter,
    event_bus: &Arc<crate::event_bus::EventBus>,
) {
    let mut router = subscription_router.lock().await;
    let to_remove: Vec<String> = router
        .iter()
        .filter(|(_, entry)| entry.proxy_id == proxy_id)
        .map(|(sub_id, _)| sub_id.clone())
        .collect();
    for sub_id in &to_remove {
        if let Some(entry) = router.remove(sub_id) {
            entry.cancel_token.cancel();
        }
        event_bus.unsubscribe(sub_id);
    }
    if !to_remove.is_empty() {
        info!(
            "Cleaned up {} EventBus subscriptions for proxy {}",
            to_remove.len(),
            proxy_id
        );
    }
}

/// Build soul context string from the knowledge retriever's soul cache,
/// plus hot knowledge entries from SQLite (Layer 1).
///
/// Returns `None` if no retriever is available or all soul documents are empty.
fn build_soul_context(services: &HostServices) -> Option<String> {
    let retriever = services.knowledge_retriever.as_ref()?;
    let cache = retriever.soul_cache();

    let mut sections = Vec::new();
    if !cache.identity_raw.trim().is_empty() {
        sections.push(cache.identity_raw.trim().to_string());
    }
    if !cache.soul_raw.trim().is_empty() {
        sections.push(cache.soul_raw.trim().to_string());
    }
    if !cache.user_raw.trim().is_empty() {
        sections.push(cache.user_raw.trim().to_string());
    }
    if !cache.tools_raw.trim().is_empty() {
        let mut tools_content = cache.tools_raw.trim().to_string();
        // Replace MCP Tool Inventory placeholder with actual tool data
        tools_content = populate_mcp_tool_inventory(tools_content, services);
        sections.push(tools_content);
    }
    if !cache.agents_raw.trim().is_empty() {
        sections.push(cache.agents_raw.trim().to_string());
    }

    // Layer 1: inject hot knowledge entries from SQLite
    let hot_section = build_hot_knowledge_section(&services.database);
    if let Some(hot) = hot_section {
        sections.push(hot);
    }

    if sections.is_empty() {
        return None;
    }
    Some(sections.join("\n\n"))
}

/// Replace the MCP Tool Inventory placeholder in TOOLS.md with actual tool data
/// from connected MCP servers.
fn populate_mcp_tool_inventory(mut content: String, services: &HostServices) -> String {
    const PLACEHOLDER: &str = "| (Populated at runtime from MCP registry) | | | | |";

    if !content.contains(PLACEHOLDER) {
        return content;
    }

    // Read tool names from the search index (try_read to avoid blocking tokio runtime)
    let tool_rows = if let Some(ref index) = services.tool_search {
        let Ok(index) = index.try_read() else {
            return content; // Lock contended, skip replacement
        };
        let tools = index.all_tools();
        if tools.is_empty() {
            "| (No MCP tools connected) | | | | |".to_string()
        } else {
            tools
                .iter()
                .map(|t| format!("| `{}` | MCP | - | - | - |", t.name))
                .collect::<Vec<_>>()
                .join("\n")
        }
    } else {
        "| (No MCP tool search index) | | | | |".to_string()
    };

    content = content.replace(PLACEHOLDER, &tool_rows);
    content
}

/// Build a markdown section from hot knowledge entries, grouped by category.
///
/// Returns `None` if there are no hot entries.
fn build_hot_knowledge_section(database: &nevoflux_storage::Database) -> Option<String> {
    let repo = nevoflux_storage::KnowledgeRepository::new(database);
    let hot_entries = repo.list_hot().ok()?;

    if hot_entries.is_empty() {
        return None;
    }

    let mut site_lines = Vec::new();
    let mut tool_lines = Vec::new();
    let mut pref_lines = Vec::new();
    let mut project_lines = Vec::new();
    let mut error_lines = Vec::new();

    for entry in &hot_entries {
        let line = entry.hot_summary.as_deref().unwrap_or(&entry.summary);

        // Freshness warning for stale entries (> 1 day old)
        let freshness = if let Ok(updated) = chrono::DateTime::parse_from_rfc3339(&entry.updated_at)
        {
            let days = (chrono::Utc::now() - updated.with_timezone(&chrono::Utc)).num_days();
            if days > 1 {
                format!(" [{}d old, verify before acting]", days)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let formatted = format!("- {}{}", line, freshness);
        match entry.category.as_str() {
            "site_interaction" | "siteinteraction" => site_lines.push(formatted),
            "tool_optimization" | "tooloptimization" => tool_lines.push(formatted),
            "user_preference" | "userpreference" => pref_lines.push(formatted),
            "workspace_context" | "workspacecontext" | "project_context" | "projectcontext" => {
                project_lines.push(formatted)
            }
            "error_pattern" | "errorpattern" => error_lines.push(formatted),
            _ => pref_lines.push(formatted),
        }
    }

    let mut parts = Vec::new();
    parts.push("## Learned Knowledge / 已学习的知识".to_string());

    if !site_lines.is_empty() {
        parts.push("### Site Interactions / 网站交互".to_string());
        parts.extend(site_lines);
    }
    if !tool_lines.is_empty() {
        parts.push("### Tool Optimizations / 工具优化".to_string());
        parts.extend(tool_lines);
    }
    if !pref_lines.is_empty() {
        parts.push("### User Preferences / 用户偏好".to_string());
        parts.extend(pref_lines);
    }
    if !project_lines.is_empty() {
        parts.push("### Workspace Context / 工作环境".to_string());
        parts.extend(project_lines);
    }
    if !error_lines.is_empty() {
        parts.push("### Error Patterns / 错误模式".to_string());
        parts.extend(error_lines);
    }

    Some(parts.join("\n"))
}

/// Convert storage messages to wasm messages for the agent history.
fn convert_history_messages(messages: Vec<StorageMessage>) -> Vec<WasmMessage> {
    messages
        .into_iter()
        .filter_map(|msg| match msg.role {
            MessageRole::User => Some(WasmMessage::user(msg.content)),
            MessageRole::Assistant => {
                // Skip tool use messages — they are internal implementation details
                // of the WASM agent's tool loop and should not appear as plain text
                // in the conversation history sent to the LLM.
                if msg.content_type == ContentType::ToolUse {
                    return None;
                }
                Some(WasmMessage::assistant(msg.content))
            }
            MessageRole::System => None,
        })
        .collect()
}

/// Load session history messages for the agent.
///
/// Retrieves only the most recent messages using an efficient SQL query with
/// `ORDER BY created_at DESC LIMIT N` (leverages composite index), then removes
/// the last message (the current user message just saved).
async fn load_session_history(
    session_manager: &SessionManager,
    session_id: &str,
    max_messages: u32,
) -> Vec<WasmMessage> {
    // Fetch max_messages + 1 so we can pop the current user message and still
    // have max_messages of history.
    match session_manager
        .get_recent_messages(session_id, max_messages + 1)
        .await
    {
        Ok(mut messages) => {
            // Remove the last message (the current user message we just saved)
            if !messages.is_empty() {
                messages.pop();
            }
            convert_history_messages(messages)
        }
        Err(e) => {
            warn!("Failed to load session history for {}: {}", session_id, e);
            vec![]
        }
    }
}

/// Handle chat channel messages with streaming support.
///
/// This function processes chat messages and streams the response back to the sidebar
/// in real-time as the LLM generates output.
#[allow(clippy::too_many_arguments)]
async fn handle_chat_message_streaming(
    payload: &serde_json::Value,
    config: &Arc<AgentConfig>,
    shared_config: &SharedAgentConfig,
    session_manager: &Arc<SessionManager>,
    services: &HostServices,
    runtime: tokio::runtime::Handle,
    identity: Vec<u8>,
    proxy_id: String,
    request_id: String,
    channel: Channel,
    response_tx: mpsc::Sender<(Vec<u8>, DaemonEnvelope)>,
    cancellation_registry: CancellationRegistry,
    interrupt_registry: InterruptRegistry,
    plan_registry: PlanRequestRegistry,
    trace_enabled: bool,
    extraction_registry: ExtractionRegistry,
    canvas_video_service: Arc<crate::canvas_video::CanvasVideoService>,
) {
    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

    // Debug: log raw attachments from payload
    let raw_attachments = payload.get("payload").and_then(|p| p.get("attachments"));
    info!(
        "handle_chat_message_streaming: raw_attachments present={}, is_array={}, len={:?}",
        raw_attachments.is_some(),
        raw_attachments.map(|a| a.is_array()).unwrap_or(false),
        raw_attachments
            .and_then(|a| a.as_array())
            .map(|arr| arr.len())
    );

    // For non-chat_message types, handle synchronously
    if msg_type != "chat_message" {
        let mut response_payload = handle_chat_message(
            payload,
            config,
            shared_config,
            session_manager,
            services,
            runtime,
            proxy_id.clone(),
            identity.clone(),
            canvas_video_service.clone(),
        )
        .await;
        // Add done: true to signal this is a complete response (not streaming)
        if let Some(obj) = response_payload.as_object_mut() {
            if let Some(payload_obj) = obj.get_mut("payload").and_then(|p| p.as_object_mut()) {
                payload_obj.insert("done".to_string(), serde_json::json!(true));
            }
        }
        let response =
            DaemonEnvelope::new(&proxy_id, channel, response_payload).with_request_id(&request_id);
        if let Err(e) = response_tx.send((identity, response)).await {
            error!("Failed to queue response: {}", e);
        }
        return;
    }

    // Extract message content from payload
    let message_content = payload
        .get("payload")
        .and_then(|p| p.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if message_content.is_empty() {
        let response_payload = serde_json::json!({
            "type": "error",
            "payload": {
                "code": "EMPTY_MESSAGE",
                "message": "Message content is empty"
            }
        });
        let response =
            DaemonEnvelope::new(&proxy_id, channel, response_payload).with_request_id(&request_id);
        if let Err(e) = response_tx.send((identity, response)).await {
            error!("Failed to queue response: {}", e);
        }
        return;
    }

    // Extract session_id if provided
    let session_id = payload
        .get("payload")
        .and_then(|p| p.get("session_id"))
        .and_then(|s| s.as_str())
        .unwrap_or("default")
        .to_string();

    // Record session→proxy mapping so /loop iterations spawned in this
    // session can borrow this sidebar's proxy_id/client_identity for
    // browser_* tool calls.
    if let Some(tracker) = services.session_proxy_tracker.as_ref() {
        tracker.note(&session_id, &proxy_id, &identity);
    }

    // Extract mode if provided (default to Chat)
    let mode = payload
        .get("payload")
        .and_then(|p| p.get("mode"))
        .and_then(|m| m.as_str())
        .map(parse_agent_mode)
        .unwrap_or(AgentMode::Chat);

    // Extract attachments (multimodal: images, files)
    let mut attachments: Vec<Attachment> = payload
        .get("payload")
        .and_then(|p| p.get("attachments"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let name = v.get("name")?.as_str()?.to_string();
                    let mime_type = v.get("mime_type")?.as_str()?.to_string();
                    let data = v.get("data")?.as_str()?.to_string();
                    Some(Attachment {
                        name,
                        mime_type,
                        data,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract local file references (from file picker)
    let mut local_files: Vec<nevoflux_protocol::FileInfo> = payload
        .get("payload")
        .and_then(|p| p.get("local_files"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let path = v.get("path")?.as_str()?.to_string();
                    let is_directory = v.get("is_directory")?.as_bool()?;
                    let size = v.get("size").and_then(|s| s.as_u64());
                    let modified = v.get("modified").and_then(|m| m.as_u64());
                    Some(nevoflux_protocol::FileInfo {
                        path,
                        is_directory,
                        size,
                        modified,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract tab_id if provided (from browser sidebar)
    let tab_id = payload
        .get("payload")
        .and_then(|p| p.get("tab_id"))
        .and_then(|t| t.as_i64());

    // Extract tab_ids list if provided (all available tabs)
    let tab_ids: Vec<nevoflux_builtin_wasm::TabInfo> = payload
        .get("payload")
        .and_then(|p| p.get("tab_ids"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let space = v
                        .get("space")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tab_id = v.get("tab_id").and_then(|t| t.as_i64())?;
                    let tab_title = v
                        .get("tab_title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = v
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(nevoflux_builtin_wasm::TabInfo {
                        space,
                        tab_id,
                        tab_title,
                        url,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Detect and process /skillname commands (same logic as non-streaming path)
    let (effective_message, skill_context) = if let Some(trimmed) =
        message_content.strip_prefix('/')
    {
        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let skill_name = parts[0].trim();
        let args = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();

        if skill_name.is_empty() {
            (message_content.to_string(), None)
        } else {
            let registry = services.skills.read().await;
            if let Some(skill) = registry.get(skill_name) {
                // Check if required tools are available
                let available_tools = gather_available_tools(services).await;
                match check_tool_availability(&skill.metadata, &available_tools) {
                    ToolCheckResult::Satisfied => {}
                    ToolCheckResult::Missing(missing) => {
                        let message = format_missing_tools_message(skill_name, &missing);
                        warn!(
                            "Skill '{}' requires unavailable tools: {:?}",
                            skill_name, missing
                        );
                        let response_payload = serde_json::json!({
                            "type": "error",
                            "payload": {
                                "code": "SKILL_TOOLS_UNAVAILABLE",
                                "message": message,
                                "recoverable": true,
                                "missing_tools": missing
                            }
                        });
                        let response = DaemonEnvelope::new(&proxy_id, channel, response_payload)
                            .with_request_id(&request_id);
                        if let Err(e) = response_tx.send((identity, response)).await {
                            error!("Failed to queue response: {}", e);
                        }
                        return;
                    }
                }

                let base_path = skill
                    .file_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let available_files = if !base_path.is_empty() {
                    match std::fs::read_dir(&base_path) {
                        Ok(entries) => {
                            let mut files: Vec<String> = entries
                                .filter_map(|e| e.ok())
                                .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                                .filter_map(|e| {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    if name.to_uppercase() == "SKILL.MD" {
                                        None
                                    } else {
                                        Some(name)
                                    }
                                })
                                .collect();
                            files.sort();
                            files
                        }
                        Err(e) => {
                            warn!("Failed to enumerate skill directory {}: {}", base_path, e);
                            vec![]
                        }
                    }
                } else {
                    vec![]
                };

                info!(
                    "Injecting skill '{}' into streaming system prompt (base_path={}, files={:?})",
                    skill_name, base_path, available_files
                );

                let ctx = nevoflux_builtin_wasm::SkillContext {
                    name: skill.metadata.name.clone(),
                    base_path,
                    content: skill.content.clone(),
                    available_files,
                };
                (args, Some(ctx))
            } else {
                warn!("Skill '{}' not found in streaming path", skill_name);
                let response_payload = serde_json::json!({
                    "type": "error",
                    "payload": {
                        "code": "SKILL_NOT_FOUND",
                        "message": format!("Skill '{}' not found. Type / to see available skills.", skill_name),
                        "recoverable": true
                    }
                });
                let response = DaemonEnvelope::new(&proxy_id, channel, response_payload)
                    .with_request_id(&request_id);
                if let Err(e) = response_tx.send((identity, response)).await {
                    error!("Failed to queue response: {}", e);
                }
                return;
            }
        }
    } else {
        (message_content.to_string(), None)
    };

    // Promote image-typed local_files into real attachments so the LLM can
    // actually SEE the picture instead of being told "use the read tool".
    // Without this, the agent runs read() on a binary PNG, gets a UTF-8
    // decode error, and silently gives up — observed in
    // /tmp/nevoflux-debug.log: round 2 produces 0 text and 0 tool calls.
    promote_image_local_files_to_attachments(&mut attachments, &mut local_files);

    info!(
        "Processing streaming chat message with mode={:?}, session={}, attachments={}, local_files={}, tab_id={:?}, tab_ids={}, skill={:?}",
        mode,
        session_id,
        attachments.len(),
        local_files.len(),
        tab_id,
        tab_ids.len(),
        skill_context.as_ref().map(|s| &s.name),
    );

    // Ensure session exists and save user message
    match session_manager.get_or_create_session(&session_id).await {
        Ok(session) => {
            info!(
                "Session ready: id={}, created_at={}",
                session.id, session.created_at
            );
        }
        Err(e) => {
            error!("Failed to get/create session {}: {}", session_id, e);
        }
    }

    // Build attachment metadata for history display (no base64 data stored)
    let attachment_metadata = build_attachment_metadata(&attachments, &local_files);

    // Save user message to database
    let mut generated_title: Option<String> = None;
    match session_manager
        .add_message_with_metadata(
            &session_id,
            MessageRole::User,
            message_content,
            attachment_metadata,
        )
        .await
    {
        Ok(msg) => {
            info!("Saved user message: id={}, session={}", msg.id, session_id);

            // Generate title from first message if session has no title yet
            match session_manager.generate_title(&session_id).await {
                Ok(Some(title)) => {
                    info!("Generated session title: {}", title);
                    generated_title = Some(title);
                }
                Ok(None) => {
                    // Session already has a title or no messages
                }
                Err(e) => {
                    error!("Failed to generate title: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to save user message to {}: {}", session_id, e);
        }
    }

    // Create trace collector for this session
    let trace_collector = {
        let file_writer = if trace_enabled {
            let data_dir = std::env::var("NEVOFLUX_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    directories::ProjectDirs::from("com", "nevoflux", "nevoflux")
                        .map(|dirs| dirs.data_dir().to_path_buf())
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                });
            let traces_dir = data_dir.join("traces");
            TraceFileWriter::new(&traces_dir, &session_id).ok()
        } else {
            None
        };
        match file_writer {
            Some(writer) => Arc::new(TraceCollector::with_file_writer(
                session_manager.shared_storage(),
                writer,
            )),
            None => Arc::new(TraceCollector::new(session_manager.shared_storage())),
        }
    };

    // Create unbounded channel for streaming chunks
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<SidebarStreamChunk>();

    // Create host functions with streaming support
    // Set client context on services so browser tool requests can be routed back
    let mut services_with_context = services
        .clone()
        .with_client_context(identity.clone(), proxy_id.clone())
        .with_session_id(session_id.clone());

    // Create a per-session interrupt flag and register it so stop_generation can find it
    let session_interrupt_flag = Arc::new(AtomicBool::new(false));
    services_with_context.interrupt_flag = session_interrupt_flag.clone();
    {
        let mut registry = interrupt_registry.lock().await;
        registry.insert(session_id.clone(), session_interrupt_flag);
        debug!("Registered interrupt flag for session: {}", session_id);
    }

    // Get or create session-level extractor from registry
    let session_extractor = {
        let mut registry = extraction_registry.lock().await;
        let entry = registry.entry(session_id.clone()).or_insert_with(|| {
            (
                std::time::Instant::now(),
                Arc::new(
                    crate::learning::session_extractor::SessionMemoryExtractor::new(
                        config.learning.extraction_interval,
                    ),
                ),
            )
        });
        // Update last-accessed timestamp
        entry.0 = std::time::Instant::now();
        entry.1.clone()
    };

    // Share session extractor with HostServices so MCP tool executor can use it
    services_with_context.session_extractor = Some(session_extractor.clone());

    let mut host = DaemonHostFunctions::new(config.clone(), runtime.clone())
        .with_services(services_with_context)
        .with_sidebar_stream(stream_tx)
        .with_session_id(session_id.clone())
        .with_trace_collector(trace_collector.clone())
        .with_session_extractor(session_extractor.clone())
        .with_canvas_video_service(canvas_video_service.clone());

    // Pass skill base path to host for relative path resolution
    if let Some(ref ctx) = skill_context {
        if !ctx.base_path.is_empty() {
            host = host.with_skill_base_path(&ctx.base_path);
        }
    }

    // Track user message for session extraction
    session_extractor.on_user_message();
    session_extractor.reset_turn_flags();

    let extraction_config = config.clone();
    let extraction_database = services.database.clone();
    let extraction_user_message = message_content.to_string();

    // Create agent with host functions
    let agent = Agent::new(host);

    // Clone tab_ids for potential plan re-run (before move into AgentInput)
    let tab_ids_for_rerun = tab_ids.clone();

    // Build agent input
    // Load MCP server names for system prompt injection
    let mcp_servers: Vec<String> = crate::mcp_config::McpServersConfig::load()
        .map(|c| {
            c.servers
                .iter()
                .filter(|s| s.enabled)
                .map(|s| s.name.clone())
                .collect()
        })
        .unwrap_or_default();

    let input = AgentInput {
        session_id: session_id.clone(),
        mode,
        user_message: effective_message,
        history: load_session_history(
            session_manager,
            &session_id,
            config.daemon.context.max_history_messages,
        )
        .await,
        attachments,
        local_files,
        custom_system_prompt: None, // Use default mode-based prompt
        tab_id,
        tab_ids,
        skill_context,
        available_models: config.llm.configured_providers(),
        mcp_servers: mcp_servers.clone(),
        soul_context: {
            let sc = build_soul_context(&services);
            debug!(
                "soul_context for AgentInput: has_retriever={}, len={:?}",
                services.knowledge_retriever.is_some(),
                sc.as_ref().map(|s| s.len())
            );
            sc
        },
        tools_config: None,
        os_platform: Some(std::env::consts::OS.to_string()),
    };

    // Create cancellation token for this streaming session
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    {
        let mut registry = cancellation_registry.lock().await;
        registry.insert(session_id.clone(), cancellation_token.clone());
        debug!("Registered cancellation token for session: {}", session_id);
    }

    // Clone variables for the streaming forwarder task
    let stream_proxy_id = proxy_id.clone();
    let stream_channel = channel;
    let stream_request_id = request_id.clone();
    let stream_identity = identity.clone();
    let stream_response_tx = response_tx.clone();
    let stream_title = generated_title.clone();
    let forwarder_cancellation = cancellation_token.clone();

    // Spawn task to forward stream chunks to the sidebar
    // Uses 300ms batch throttle: text chunks are buffered and flushed on interval tick,
    // while tool events and done signals flush immediately.
    let forwarder_handle = tokio::spawn(async move {
        let mut accumulated_text = String::new();
        let mut cancelled = false;
        let mut buffer = String::new();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Skip the immediate first tick
        interval.tick().await;

        // Helper closure to build and send a chunk payload
        macro_rules! send_chunk {
            ($text:expr, $done:expr, $event:expr, $thinking:expr, $first:expr) => {{
                let mut chunk_payload = serde_json::json!({
                    "type": "stream_chunk",
                    "payload": {
                        "content": $text,
                        "done": $done
                    }
                });
                if let Some(event) = $event {
                    if let Some(p) = chunk_payload.get_mut("payload") {
                        p["event"] = serde_json::to_value(event).unwrap_or_default();
                    }
                }
                if let Some(thinking) = $thinking {
                    if let Some(p) = chunk_payload.get_mut("payload") {
                        p["thinking_event"] = serde_json::to_value(thinking).unwrap_or_default();
                    }
                }
                if $first {
                    if let Some(ref title) = stream_title {
                        chunk_payload["payload"]["session_title"] =
                            serde_json::Value::String(title.clone());
                    }
                }
                let response = DaemonEnvelope::new(&stream_proxy_id, stream_channel, chunk_payload)
                    .with_request_id(&stream_request_id);
                stream_response_tx
                    .send((stream_identity.clone(), response))
                    .await
            }};
        }

        // Flush buffered text to sidebar.
        // Returns true if text was sent (or nothing to send), false on send error.
        // Large payloads are split into chunks to stay under native messaging size limits (~1MB).
        const MAX_PROXY_CHUNK: usize = 800_000;

        macro_rules! flush_buffer {
            ($done:expr) => {{
                let text = std::mem::take(&mut buffer);
                let is_first_chunk = accumulated_text.is_empty();
                accumulated_text.push_str(&text);

                if !text.is_empty() {
                    if text.len() <= MAX_PROXY_CHUNK {
                        // Small enough to send in one message
                        send_chunk!(
                            text,
                            $done,
                            None::<&serde_json::Value>,
                            None::<&nevoflux_protocol::ThinkingEvent>,
                            is_first_chunk
                        )
                        .is_ok()
                    } else {
                        // Split large payload into multiple proxy messages
                        let mut offset = 0;
                        let mut first = is_first_chunk;
                        let mut ok = true;
                        while offset < text.len() {
                            let mut end = (offset + MAX_PROXY_CHUNK).min(text.len());
                            // Ensure we don't split a multi-byte UTF-8 character
                            while end < text.len() && !text.is_char_boundary(end) {
                                end -= 1;
                            }
                            let chunk_text = &text[offset..end];
                            let is_last = end >= text.len();
                            let done_flag = is_last && $done;
                            if send_chunk!(
                                chunk_text.to_string(),
                                done_flag,
                                None::<&serde_json::Value>,
                                None::<&nevoflux_protocol::ThinkingEvent>,
                                first
                            )
                            .is_err()
                            {
                                ok = false;
                                break;
                            }
                            first = false;
                            offset = end;
                        }
                        ok
                    }
                } else if $done {
                    send_chunk!(
                        "",
                        true,
                        None::<&serde_json::Value>,
                        None::<&nevoflux_protocol::ThinkingEvent>,
                        is_first_chunk
                    )
                    .is_ok()
                } else {
                    true
                }
            }};
        }

        loop {
            tokio::select! {
                biased;

                // Check cancellation first
                _ = forwarder_cancellation.cancelled() => {
                    info!("Stream forwarder cancelled");
                    cancelled = true;
                    break;
                }

                // Flush buffer on 300ms tick
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        if !flush_buffer!(false) {
                            error!("Failed to send stream chunk");
                            break;
                        }
                    }
                }

                // Receive stream chunks
                chunk = stream_rx.recv() => {
                    match chunk {
                        Some(chunk) => {
                            if chunk.event.is_some() || chunk.thinking_event.is_some() {
                                // Tool/thinking event: flush text buffer first, then send event immediately
                                if !buffer.is_empty() {
                                    if !flush_buffer!(false) {
                                        error!("Failed to send stream chunk");
                                        break;
                                    }
                                }
                                // Send event chunk with any accompanying text
                                let is_first = accumulated_text.is_empty();
                                accumulated_text.push_str(&chunk.text);
                                if let Err(e) = send_chunk!(chunk.text, chunk.done, chunk.event.as_ref(), chunk.thinking_event.as_ref(), is_first) {
                                    error!("Failed to send stream chunk: {}", e);
                                    break;
                                }
                                if chunk.done {
                                    debug!(
                                        "Stream completed, total accumulated: {} bytes",
                                        accumulated_text.len()
                                    );
                                    break;
                                }
                            } else if chunk.done {
                                // Done: flush remaining buffer + final text
                                buffer.push_str(&chunk.text);
                                if !flush_buffer!(true) {
                                    error!("Failed to send final stream chunk");
                                }
                                debug!(
                                    "Stream completed, total accumulated: {} bytes",
                                    accumulated_text.len()
                                );
                                break;
                            } else {
                                // Normal text: just buffer it
                                buffer.push_str(&chunk.text);
                            }
                        }
                        None => {
                            // Channel closed, flush remaining
                            if !buffer.is_empty() {
                                let _ = flush_buffer!(false);
                            }
                            debug!("Stream channel closed");
                            break;
                        }
                    }
                }
            }
        }

        (accumulated_text, cancelled)
    });

    // Run agent (this will call stream_emit() for each chunk)
    let agent_result = tokio::task::spawn_blocking(move || agent.run(&input)).await;

    // Wait for the forwarder to complete
    let (accumulated_text, was_cancelled) = match forwarder_handle.await {
        Ok(result) => result,
        Err(e) => {
            error!("Stream forwarder task failed: {}", e);
            (String::new(), false)
        }
    };

    // Cleanup cancellation token and interrupt flag from registries
    {
        let mut registry = cancellation_registry.lock().await;
        registry.remove(&session_id);
        debug!("Removed cancellation token for session: {}", session_id);
    }
    {
        let mut registry = interrupt_registry.lock().await;
        registry.remove(&session_id);
        debug!("Removed interrupt flag for session: {}", session_id);
    }

    // Note: Do NOT call trace_collector.cleanup_session() here —
    // it deletes trace_spans from SQLite, preventing the learning
    // collector from reading tool failure data. Trace spans are
    // cleaned up by the learning collector after processing.

    // If cancelled, don't send final response (stop_generation handler already did)
    if was_cancelled {
        info!(
            "Streaming session {} was cancelled, skipping final response",
            session_id
        );
        return;
    }

    // Handle agent result
    match agent_result {
        Ok(Ok(output)) => {
            // Handle plan proposal if present
            if let Some(proposal) = &output.plan_proposal {
                info!("Agent returned plan proposal for session {}", session_id);

                // Register oneshot channel BEFORE sending proposal to frontend
                // to avoid race condition if frontend responds very quickly
                let (plan_tx, plan_rx) = oneshot::channel();
                plan_registry
                    .lock()
                    .await
                    .insert(session_id.clone(), plan_tx);

                // Send proposal to frontend
                let msg = AgentMessage::PlanProposal(proposal.clone());
                let payload = serde_json::to_value(&msg).unwrap();
                let envelope =
                    DaemonEnvelope::new(&proxy_id, channel, payload).with_request_id(&request_id);
                if let Err(e) = response_tx.send((identity.clone(), envelope)).await {
                    error!("Failed to send plan proposal: {}", e);
                }

                match plan_rx.await {
                    Ok(PlanResponse::Confirmed) => {
                        info!(
                            "Plan confirmed for session {}, re-running agent with plan context",
                            session_id
                        );
                        let plan_text = format_plan_as_context(proposal);

                        // Save plan proposal as assistant message for history
                        if let Err(e) = session_manager
                            .add_message(
                                &session_id,
                                MessageRole::Assistant,
                                &format!("Plan proposed:\n{}", plan_text),
                            )
                            .await
                        {
                            error!("Failed to save plan message: {}", e);
                        }

                        // Save plan text as user message (the "execute" instruction)
                        if let Err(e) = session_manager
                            .add_message(&session_id, MessageRole::User, &plan_text)
                            .await
                        {
                            error!("Failed to save plan user message: {}", e);
                        }

                        // Create new streaming channel for re-run
                        let (rerun_stream_tx, mut rerun_stream_rx) =
                            tokio::sync::mpsc::unbounded_channel::<SidebarStreamChunk>();

                        // Create new host functions
                        let rerun_services = services
                            .clone()
                            .with_client_context(identity.clone(), proxy_id.clone());
                        let rerun_host = DaemonHostFunctions::new(config.clone(), runtime.clone())
                            .with_services(rerun_services)
                            .with_sidebar_stream(rerun_stream_tx)
                            .with_session_id(session_id.clone())
                            .with_trace_collector(trace_collector.clone())
                            .with_session_extractor(session_extractor.clone())
                            .with_canvas_video_service(canvas_video_service.clone());

                        let rerun_agent = Agent::new(rerun_host);

                        // Build new input with plan as user message
                        let rerun_input = AgentInput {
                            session_id: session_id.clone(),
                            mode,
                            user_message: plan_text.clone(),
                            history: load_session_history(
                                session_manager,
                                &session_id,
                                config.daemon.context.max_history_messages,
                            )
                            .await,
                            attachments: vec![],
                            local_files: vec![],
                            custom_system_prompt: None,
                            tab_id,
                            tab_ids: tab_ids_for_rerun.clone(),
                            skill_context: None,
                            available_models: config.llm.configured_providers(),
                            mcp_servers: mcp_servers.clone(),
                            soul_context: build_soul_context(&services),
                            tools_config: None,
                            os_platform: Some(std::env::consts::OS.to_string()),
                        };

                        // Spawn stream forwarder for re-run
                        let rerun_proxy_id = proxy_id.clone();
                        let rerun_channel = channel;
                        let rerun_request_id = request_id.clone();
                        let rerun_identity = identity.clone();
                        let rerun_response_tx = response_tx.clone();

                        let rerun_forwarder = tokio::spawn(async move {
                            let mut rerun_accumulated = String::new();
                            let mut buffer = String::new();
                            let mut interval =
                                tokio::time::interval(tokio::time::Duration::from_millis(300));
                            interval
                                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                            interval.tick().await; // skip immediate first tick

                            loop {
                                tokio::select! {
                                    biased;

                                    _ = interval.tick() => {
                                        if !buffer.is_empty() {
                                            let text = std::mem::take(&mut buffer);
                                            rerun_accumulated.push_str(&text);
                                            let chunk_payload = serde_json::json!({
                                                "type": "stream_chunk",
                                                "payload": {
                                                    "content": text,
                                                    "done": false
                                                }
                                            });
                                            let response = DaemonEnvelope::new(
                                                &rerun_proxy_id,
                                                rerun_channel,
                                                chunk_payload,
                                            )
                                            .with_request_id(&rerun_request_id);
                                            if let Err(e) = rerun_response_tx
                                                .send((rerun_identity.clone(), response))
                                                .await
                                            {
                                                error!("Failed to send rerun stream chunk: {}", e);
                                                break;
                                            }
                                        }
                                    }

                                    chunk = rerun_stream_rx.recv() => {
                                        match chunk {
                                            Some(chunk) => {
                                                if chunk.event.is_some() || chunk.thinking_event.is_some() {
                                                    // Tool/thinking event: flush buffer, then send event immediately
                                                    if !buffer.is_empty() {
                                                        let text = std::mem::take(&mut buffer);
                                                        rerun_accumulated.push_str(&text);
                                                        let flush_payload = serde_json::json!({
                                                            "type": "stream_chunk",
                                                            "payload": {
                                                                "content": text,
                                                                "done": false
                                                            }
                                                        });
                                                        let response = DaemonEnvelope::new(
                                                            &rerun_proxy_id,
                                                            rerun_channel,
                                                            flush_payload,
                                                        )
                                                        .with_request_id(&rerun_request_id);
                                                        if let Err(e) = rerun_response_tx
                                                            .send((rerun_identity.clone(), response))
                                                            .await
                                                        {
                                                            error!("Failed to send rerun stream chunk: {}", e);
                                                            break;
                                                        }
                                                    }
                                                    rerun_accumulated.push_str(&chunk.text);
                                                    let mut chunk_payload = serde_json::json!({
                                                        "type": "stream_chunk",
                                                        "payload": {
                                                            "content": chunk.text,
                                                            "done": chunk.done
                                                        }
                                                    });
                                                    if let Some(event) = &chunk.event {
                                                        if let Some(p) = chunk_payload.get_mut("payload") {
                                                            p["event"] =
                                                                serde_json::to_value(event).unwrap_or_default();
                                                        }
                                                    }
                                                    if let Some(thinking) = &chunk.thinking_event {
                                                        if let Some(p) = chunk_payload.get_mut("payload") {
                                                            p["thinking_event"] =
                                                                serde_json::to_value(thinking).unwrap_or_default();
                                                        }
                                                    }
                                                    let response = DaemonEnvelope::new(
                                                        &rerun_proxy_id,
                                                        rerun_channel,
                                                        chunk_payload,
                                                    )
                                                    .with_request_id(&rerun_request_id);
                                                    if let Err(e) = rerun_response_tx
                                                        .send((rerun_identity.clone(), response))
                                                        .await
                                                    {
                                                        error!("Failed to send rerun stream chunk: {}", e);
                                                        break;
                                                    }
                                                    if chunk.done {
                                                        break;
                                                    }
                                                } else if chunk.done {
                                                    buffer.push_str(&chunk.text);
                                                    let final_text = std::mem::take(&mut buffer);
                                                    rerun_accumulated.push_str(&final_text);
                                                    let chunk_payload = serde_json::json!({
                                                        "type": "stream_chunk",
                                                        "payload": {
                                                            "content": final_text,
                                                            "done": true
                                                        }
                                                    });
                                                    let response = DaemonEnvelope::new(
                                                        &rerun_proxy_id,
                                                        rerun_channel,
                                                        chunk_payload,
                                                    )
                                                    .with_request_id(&rerun_request_id);
                                                    if let Err(e) = rerun_response_tx
                                                        .send((rerun_identity.clone(), response))
                                                        .await
                                                    {
                                                        error!("Failed to send rerun stream chunk: {}", e);
                                                    }
                                                    break;
                                                } else {
                                                    // Buffer text for batched sending
                                                    buffer.push_str(&chunk.text);
                                                }
                                            }
                                            None => {
                                                if !buffer.is_empty() {
                                                    let text = std::mem::take(&mut buffer);
                                                    rerun_accumulated.push_str(&text);
                                                    let chunk_payload = serde_json::json!({
                                                        "type": "stream_chunk",
                                                        "payload": {
                                                            "content": text,
                                                            "done": false
                                                        }
                                                    });
                                                    let response = DaemonEnvelope::new(
                                                        &rerun_proxy_id,
                                                        rerun_channel,
                                                        chunk_payload,
                                                    )
                                                    .with_request_id(&rerun_request_id);
                                                    let _ = rerun_response_tx
                                                        .send((rerun_identity.clone(), response))
                                                        .await;
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            rerun_accumulated
                        });

                        // Run agent with plan context
                        let rerun_result =
                            tokio::task::spawn_blocking(move || rerun_agent.run(&rerun_input))
                                .await;

                        // Wait for forwarder
                        let rerun_text = match rerun_forwarder.await {
                            Ok(result) => result,
                            Err(e) => {
                                error!("Rerun forwarder failed: {}", e);
                                String::new()
                            }
                        };

                        // Handle rerun result
                        match rerun_result {
                            Ok(Ok(output)) => {
                                let final_text = if output.text.is_empty() {
                                    rerun_text.clone()
                                } else {
                                    output.text.clone()
                                };

                                if !final_text.is_empty() {
                                    if let Err(e) = session_manager
                                        .add_message(
                                            &session_id,
                                            MessageRole::Assistant,
                                            &final_text,
                                        )
                                        .await
                                    {
                                        error!("Failed to save rerun response: {}", e);
                                    }
                                }

                                // Send final completion
                                let final_payload = serde_json::json!({
                                    "type": "stream_chunk",
                                    "payload": {
                                        "content": "",
                                        "tool_calls": output.tool_calls,
                                        "done": true
                                    }
                                });
                                let response =
                                    DaemonEnvelope::new(&proxy_id, channel, final_payload)
                                        .with_request_id(&request_id);
                                if let Err(e) = response_tx.send((identity, response)).await {
                                    error!("Failed to send rerun final response: {}", e);
                                }
                            }
                            Ok(Err(e)) => {
                                error!("Plan execution failed: {}", e);
                                let error_payload = serde_json::json!({
                                    "type": "error",
                                    "payload": {
                                        "code": "PLAN_EXECUTION_ERROR",
                                        "message": format!("Plan execution error: {}", e)
                                    }
                                });
                                let response =
                                    DaemonEnvelope::new(&proxy_id, channel, error_payload)
                                        .with_request_id(&request_id);
                                if let Err(e) = response_tx.send((identity, response)).await {
                                    error!("Failed to send plan error: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Plan execution task panicked: {}", e);
                                let error_payload = serde_json::json!({
                                    "type": "error",
                                    "payload": {
                                        "code": "PLAN_EXECUTION_PANIC",
                                        "message": format!("Plan execution task failed: {}", e)
                                    }
                                });
                                let response =
                                    DaemonEnvelope::new(&proxy_id, channel, error_payload)
                                        .with_request_id(&request_id);
                                if let Err(e) = response_tx.send((identity, response)).await {
                                    error!("Failed to send plan panic error: {}", e);
                                }
                            }
                        }
                    }
                    Ok(PlanResponse::Cancelled) => {
                        info!("Plan cancelled for session {}", session_id);
                        let cancel_payload = serde_json::json!({
                            "type": "stream_chunk",
                            "payload": {
                                "content": "Plan cancelled by user.",
                                "done": true
                            }
                        });
                        let response = DaemonEnvelope::new(&proxy_id, channel, cancel_payload)
                            .with_request_id(&request_id);
                        if let Err(e) = response_tx.send((identity, response)).await {
                            error!("Failed to send plan cancellation response: {}", e);
                        }
                    }
                    Err(_) => {
                        warn!(
                            "Plan response channel dropped for session {}, treating as cancelled",
                            session_id
                        );
                        let cancel_payload = serde_json::json!({
                            "type": "stream_chunk",
                            "payload": {
                                "content": "Plan cancelled (connection lost).",
                                "done": true
                            }
                        });
                        let response = DaemonEnvelope::new(&proxy_id, channel, cancel_payload)
                            .with_request_id(&request_id);
                        if let Err(e) = response_tx.send((identity, response)).await {
                            error!("Failed to send plan drop response: {}", e);
                        }
                    }
                }
                return;
            }

            // Handle artifact if present (fire-and-forget, no user response needed)
            if let Some(artifact) = &output.artifact {
                info!(
                    "Agent created artifact '{}' for session {}",
                    artifact.title, session_id
                );
                send_artifact_stream(
                    artifact,
                    &session_id,
                    session_manager,
                    &proxy_id,
                    channel,
                    &request_id,
                    &identity,
                    &response_tx,
                )
                .await;
            }

            // Handle pending artifacts from MCP bridge mode (create_artifact via MCP tool calls)
            {
                #[allow(unused_imports)]
                use nevoflux_llm::providers::acp::mcp_bridge::McpToolBridge;
                let acp_providers = crate::wasm::llm::acp_providers();
                let providers = acp_providers.lock().await;
                info!(
                    "Checking {} ACP providers for pending artifacts",
                    providers.len()
                );
                // Check all providers for pending artifacts
                for (key, provider) in providers.iter() {
                    info!(
                        "ACP provider '{}': has_tool_bridge={}",
                        key,
                        provider.tool_bridge().is_some()
                    );
                    if let Some(bridge) = provider.tool_bridge() {
                        let pending = bridge.drain_artifacts();
                        info!(
                            "ACP provider '{}': drained {} pending artifacts",
                            key,
                            pending.len()
                        );
                        for pa in pending {
                            let artifact = Artifact {
                                id: pa.id,
                                title: pa.title,
                                content_type: pa.content_type,
                                description: pa.description,
                                content: pa.content,
                                files: pa.files,
                                entry: pa.entry,
                                is_persistent: false,
                            };
                            info!(
                                "MCP bridge: sending artifact '{}' to sidebar for session {}",
                                artifact.title, session_id
                            );
                            send_artifact_stream(
                                &artifact,
                                &session_id,
                                session_manager,
                                &proxy_id,
                                channel,
                                &request_id,
                                &identity,
                                &response_tx,
                            )
                            .await;
                        }
                    }
                }
            }

            // Execute Code Mode if agent returned Python code
            let raw_text = if output.text.is_empty() {
                &accumulated_text
            } else {
                &output.text
            };

            // orchestrate is a tool call handled inside the agent loop
            // (agent_host.rs intercepts tool_call_dynamic("orchestrate", ...)).
            // No post-hoc fence extraction needed.
            let final_text = raw_text.to_string();

            // Save assistant response to database
            if !final_text.is_empty() {
                match session_manager
                    .add_message(&session_id, MessageRole::Assistant, &final_text)
                    .await
                {
                    Ok(msg) => {
                        info!(
                            "Saved assistant message: id={}, session={}",
                            msg.id, session_id
                        );
                    }
                    Err(e) => {
                        error!("Failed to save assistant message to {}: {}", session_id, e);
                    }
                }
            }

            // Session memory extraction (background, non-blocking)
            if extraction_config.learning.enable_session_extraction
                && session_extractor.should_extract()
            {
                let ext_config = extraction_config.clone();
                let ext_db = extraction_database.clone();
                let ext_session_id = session_id.clone();
                // Build context messages from the user message and assistant response
                let mut ext_messages: Vec<crate::context::ContextMessage> = Vec::new();
                ext_messages.push(crate::context::ContextMessage {
                    role: "user".to_string(),
                    content: extraction_user_message.clone(),
                });
                if !final_text.is_empty() {
                    ext_messages.push(crate::context::ContextMessage {
                        role: "assistant".to_string(),
                        content: final_text.clone(),
                    });
                }
                tokio::spawn(async move {
                    match crate::learning::session_extractor::extract_session_memories(
                        ext_config,
                        ext_db,
                        ext_messages,
                    )
                    .await
                    {
                        Ok(n) if n > 0 => {
                            tracing::info!(
                                session_id = %ext_session_id,
                                count = n,
                                "Session memory extraction completed"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                session_id = %ext_session_id,
                                error = %e,
                                "Session memory extraction failed"
                            );
                        }
                        _ => {}
                    }
                });
            }

            // Save tool calls to session history
            for tool_call in &output.tool_calls {
                if let Err(e) = session_manager
                    .add_tool_use_message(
                        &session_id,
                        &tool_call.id,
                        &tool_call.name,
                        &tool_call.arguments,
                        None,
                    )
                    .await
                {
                    error!("Failed to save tool call {}: {}", tool_call.name, e);
                }
            }

            // Merge MCP bridge tool calls with WASM agent tool calls for sidebar display.
            // ACP providers with use_mcp_bridge=true handle tool calls natively via MCP —
            // these never appear in output.tool_calls from the WASM agent. We drain them
            // from the bridge's log so they show up in the sidebar.
            let mut all_tool_calls = output.tool_calls.clone();
            {
                let acp_providers = crate::wasm::llm::acp_providers();
                let providers = acp_providers.lock().await;
                for provider in providers.values() {
                    if let Some(bridge) = provider.tool_bridge() {
                        let mcp_calls = bridge.drain_tool_calls();
                        if !mcp_calls.is_empty() {
                            tracing::info!(
                                "Draining {} MCP tool calls for sidebar display",
                                mcp_calls.len()
                            );
                        }
                        for tc in mcp_calls {
                            all_tool_calls.push(nevoflux_builtin_wasm::ToolCall {
                                id: tc.id,
                                call_id: None,
                                name: tc.name,
                                arguments: tc.arguments,
                                signature: None,
                            });
                        }
                    }
                }
            }

            // Send final completion message
            let mut final_payload = serde_json::json!({
                "type": "stream_chunk",
                "payload": {
                    "content": "",
                    "tool_calls": all_tool_calls,
                    "done": true
                }
            });

            if let Some(title) = generated_title {
                final_payload["payload"]["session_title"] = serde_json::Value::String(title);
            }

            let response =
                DaemonEnvelope::new(&proxy_id, channel, final_payload).with_request_id(&request_id);
            info!(
                "Sending final stream_chunk: tool_calls={}, has_title={}",
                all_tool_calls.len(),
                response
                    .payload
                    .get("payload")
                    .and_then(|p| p.get("session_title"))
                    .is_some()
            );
            if let Err(e) = response_tx.send((identity, response)).await {
                error!("Failed to send final response: {}", e);
            } else {
                info!("Final stream_chunk queued for writer");
            }
        }
        Ok(Err(e)) => {
            error!("Agent run failed: {}", e);
            // Send a stream_chunk with done:true first so the sidebar
            // properly ends its streaming state, then send the error.
            let done_payload = serde_json::json!({
                "type": "stream_chunk",
                "payload": {
                    "content": format!("\n\nAgent error: {}", e),
                    "tool_calls": [],
                    "done": true
                }
            });
            let done_response =
                DaemonEnvelope::new(&proxy_id, channel, done_payload).with_request_id(&request_id);
            if let Err(e) = response_tx.send((identity.clone(), done_response)).await {
                error!("Failed to send error done response: {}", e);
            }
            let error_payload = serde_json::json!({
                "type": "error",
                "payload": {
                    "code": "AGENT_ERROR",
                    "message": format!("Agent error: {}", e)
                }
            });
            let response =
                DaemonEnvelope::new(&proxy_id, channel, error_payload).with_request_id(&request_id);
            if let Err(e) = response_tx.send((identity, response)).await {
                error!("Failed to send error response: {}", e);
            }
        }
        Err(e) => {
            error!("Agent task panicked: {}", e);
            // Send a stream_chunk with done:true first so the sidebar
            // properly ends its streaming state.
            let done_payload = serde_json::json!({
                "type": "stream_chunk",
                "payload": {
                    "content": format!("\n\nAgent task failed: {}", e),
                    "tool_calls": [],
                    "done": true
                }
            });
            let done_response =
                DaemonEnvelope::new(&proxy_id, channel, done_payload).with_request_id(&request_id);
            if let Err(e) = response_tx.send((identity.clone(), done_response)).await {
                error!("Failed to send error done response: {}", e);
            }
            let error_payload = serde_json::json!({
                "type": "error",
                "payload": {
                    "code": "AGENT_PANIC",
                    "message": format!("Agent task failed: {}", e)
                }
            });
            let response =
                DaemonEnvelope::new(&proxy_id, channel, error_payload).with_request_id(&request_id);
            if let Err(e) = response_tx.send((identity, response)).await {
                error!("Failed to send error response: {}", e);
            }
        }
    }
}

/// Stream an artifact to the sidebar as start/delta/complete messages,
/// persisting it to storage first.
///
/// Splits `artifact.content` into ~4 KB chunks (respecting UTF-8 char boundaries)
/// and sends them as `ArtifactDelta` messages bracketed by `ArtifactStart` and
/// `ArtifactComplete`.
#[allow(clippy::too_many_arguments)]
async fn send_artifact_stream(
    artifact: &Artifact,
    session_id: &str,
    session_manager: &Arc<SessionManager>,
    proxy_id: &str,
    channel: Channel,
    request_id: &str,
    identity: &[u8],
    response_tx: &mpsc::Sender<(Vec<u8>, DaemonEnvelope)>,
) {
    const CHUNK_SIZE: usize = 4096;

    // For project-type artifacts with files but no content, use the entry file
    // content as fallback so older sidebars that only read artifact_delta can
    // still display something meaningful.
    let effective_content = if artifact.content.is_empty() {
        if let (Some(files), Some(entry)) = (&artifact.files, &artifact.entry) {
            files.get(entry).cloned().unwrap_or_default()
        } else if let Some(files) = &artifact.files {
            // No entry specified — pick the first file
            files.values().next().cloned().unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        artifact.content.clone()
    };

    // Persist artifact to storage (use effective_content so entry file is stored)
    let mut params = nevoflux_storage::CreateArtifactParams::new(
        &artifact.id,
        session_id,
        &artifact.title,
        &artifact.content_type,
    )
    .with_content(&effective_content);
    if let Some(desc) = &artifact.description {
        params = params.with_description(desc);
    }
    if let Some(files) = &artifact.files {
        params = params.with_files(files.clone());
    }
    if let Some(entry) = &artifact.entry {
        params = params.with_entry(entry);
    }
    let files_count = artifact.files.as_ref().map(|f| f.len()).unwrap_or(0);
    match session_manager.save_artifact(params) {
        Ok(_) => info!(
            "Persisted artifact {} (session={}, type={}, content_len={}, files_count={})",
            artifact.id,
            session_id,
            artifact.content_type,
            effective_content.len(),
            files_count
        ),
        Err(e) => error!("Failed to persist artifact {}: {}", artifact.id, e),
    }

    // 1. Send artifact_start (includes files/entry for project-type artifacts)
    let start_msg = AgentMessage::ArtifactStart(ArtifactStart {
        id: artifact.id.clone(),
        title: artifact.title.clone(),
        content_type: artifact.content_type.clone(),
        description: artifact.description.clone(),
        files: artifact.files.clone(),
        entry: artifact.entry.clone(),
        is_persistent: artifact.is_persistent,
    });
    let payload = serde_json::to_value(&start_msg).unwrap();
    let envelope = DaemonEnvelope::new(proxy_id, channel, payload).with_request_id(request_id);
    if let Err(e) = response_tx.send((identity.to_vec(), envelope)).await {
        error!("Failed to send artifact_start: {}", e);
        return;
    }

    // 2. Send artifact_delta chunks
    // For project-type artifacts with empty content, send the entry file as
    // fallback so sidebars that only handle delta can still render content.
    if !effective_content.is_empty() {
        let bytes = effective_content.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            let mut end = (offset + CHUNK_SIZE).min(bytes.len());
            // Ensure we don't split a multi-byte UTF-8 character
            while end < bytes.len() && !effective_content.is_char_boundary(end) {
                end += 1;
            }
            let chunk = &effective_content[offset..end];

            let delta_msg = AgentMessage::ArtifactDelta(ArtifactDelta {
                id: artifact.id.clone(),
                delta: chunk.to_string(),
            });
            let payload = serde_json::to_value(&delta_msg).unwrap();
            let envelope =
                DaemonEnvelope::new(proxy_id, channel, payload).with_request_id(request_id);
            if let Err(e) = response_tx.send((identity.to_vec(), envelope)).await {
                error!("Failed to send artifact_delta: {}", e);
                return;
            }
            offset = end;
        }
    }

    // 3. Send artifact_complete
    let complete_msg = AgentMessage::ArtifactComplete(ArtifactComplete {
        id: artifact.id.clone(),
    });
    let payload = serde_json::to_value(&complete_msg).unwrap();
    let envelope = DaemonEnvelope::new(proxy_id, channel, payload).with_request_id(request_id);
    if let Err(e) = response_tx.send((identity.to_vec(), envelope)).await {
        error!("Failed to send artifact_complete: {}", e);
    }
}

fn format_plan_as_context(proposal: &PlanProposal) -> String {
    let mut text = format!("Approved plan: {}\n\n", proposal.summary);
    for (i, step) in proposal.steps.iter().enumerate() {
        text.push_str(&format!("{}. {}", i + 1, step.description));
        if let Some(model) = &step.model {
            text.push_str(&format!(" [model: {}]", model));
        }
        text.push('\n');
    }
    text.push_str("\nExecute this plan now.");
    text
}

/// Handle chat channel messages using the Agent (non-streaming).
async fn handle_chat_message(
    payload: &serde_json::Value,
    config: &Arc<AgentConfig>,
    shared_config: &SharedAgentConfig,
    session_manager: &Arc<SessionManager>,
    services: &HostServices,
    runtime: tokio::runtime::Handle,
    _proxy_id: String,
    _client_identity: Vec<u8>,
    canvas_video_service: Arc<crate::canvas_video::CanvasVideoService>,
) -> serde_json::Value {
    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "ping" => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            serde_json::json!({
                "type": "pong",
                "payload": {
                    "timestamp": timestamp
                }
            })
        }
        "chat_message" => {
            // Extract message content from payload
            let message_content = payload
                .get("payload")
                .and_then(|p| p.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");

            if message_content.is_empty() {
                return serde_json::json!({
                    "type": "error",
                    "payload": {
                        "code": "EMPTY_MESSAGE",
                        "message": "Message content is empty"
                    }
                });
            }

            // Detect and process /skillname commands
            // Returns (user_message, skill_context)
            let (user_message, skill_context) = if let Some(trimmed) =
                message_content.strip_prefix('/')
            {
                // Parse: "/skillname args" -> ("skillname", "args")
                let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
                let skill_name = parts[0].trim();
                let args = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();

                if skill_name.is_empty() {
                    // Just "/" with no skill name - treat as regular message
                    (message_content.to_string(), None)
                } else {
                    // Look up skill in registry
                    let registry = services.skills.read().await;
                    if let Some(skill) = registry.get(skill_name) {
                        // Check if required tools are available
                        let available_tools = gather_available_tools(services).await;
                        match check_tool_availability(&skill.metadata, &available_tools) {
                            ToolCheckResult::Satisfied => {
                                // All tools available, proceed with skill injection
                            }
                            ToolCheckResult::Missing(missing) => {
                                // Required tools are not available
                                let message = format_missing_tools_message(skill_name, &missing);
                                warn!(
                                    "Skill '{}' requires unavailable tools: {:?}",
                                    skill_name, missing
                                );
                                return serde_json::json!({
                                    "type": "error",
                                    "payload": {
                                        "code": "SKILL_TOOLS_UNAVAILABLE",
                                        "message": message,
                                        "recoverable": true,
                                        "missing_tools": missing
                                    }
                                });
                            }
                        }

                        // Get skill's base directory path for auxiliary file access
                        let base_path = skill
                            .file_path
                            .as_ref()
                            .and_then(|p| p.parent())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();

                        // Enumerate files in skill directory (non-recursive, skip SKILL.md)
                        let available_files = if !base_path.is_empty() {
                            match std::fs::read_dir(&base_path) {
                                Ok(entries) => {
                                    let mut files: Vec<String> = entries
                                        .filter_map(|e| e.ok())
                                        .filter(|e| {
                                            e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                                        })
                                        .filter_map(|e| {
                                            let name = e.file_name().to_string_lossy().to_string();
                                            // Skip the skill definition file itself
                                            if name.to_uppercase() == "SKILL.MD" {
                                                None
                                            } else {
                                                Some(name)
                                            }
                                        })
                                        .collect();
                                    files.sort();
                                    files
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to enumerate skill directory {}: {}",
                                        base_path, e
                                    );
                                    vec![]
                                }
                            }
                        } else {
                            vec![]
                        };

                        info!(
                            "Injecting skill '{}' into system prompt (base_path={}, files={:?})",
                            skill_name, base_path, available_files
                        );

                        // Return user args as message, skill content as context
                        let ctx = nevoflux_builtin_wasm::SkillContext {
                            name: skill.metadata.name.clone(),
                            base_path,
                            content: skill.content.clone(),
                            available_files,
                        };
                        (args, Some(ctx))
                    } else {
                        // Skill not found - return error
                        return serde_json::json!({
                            "type": "error",
                            "payload": {
                                "code": "SKILL_NOT_FOUND",
                                "message": format!("Skill '{}' not found. Type / to see available skills.", skill_name),
                                "recoverable": true
                            }
                        });
                    }
                }
            } else {
                // Regular message without skill
                (message_content.to_string(), None)
            };

            // Extract session_id if provided
            let session_id = payload
                .get("payload")
                .and_then(|p| p.get("session_id"))
                .and_then(|s| s.as_str())
                .unwrap_or("default")
                .to_string();

            // Extract mode if provided (default to Chat)
            let mode = payload
                .get("payload")
                .and_then(|p| p.get("mode"))
                .and_then(|m| m.as_str())
                .map(parse_agent_mode)
                .unwrap_or(AgentMode::Chat);

            // Extract attachments (multimodal: images, files)
            let mut attachments: Vec<Attachment> = payload
                .get("payload")
                .and_then(|p| p.get("attachments"))
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let name = v.get("name")?.as_str()?.to_string();
                            let mime_type = v.get("mime_type")?.as_str()?.to_string();
                            let data = v.get("data")?.as_str()?.to_string();
                            Some(Attachment {
                                name,
                                mime_type,
                                data,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Extract local file references (from file picker)
            let mut local_files: Vec<nevoflux_protocol::FileInfo> = payload
                .get("payload")
                .and_then(|p| p.get("local_files"))
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let path = v.get("path")?.as_str()?.to_string();
                            let is_directory = v.get("is_directory")?.as_bool()?;
                            let size = v.get("size").and_then(|s| s.as_u64());
                            let modified = v.get("modified").and_then(|m| m.as_u64());
                            Some(nevoflux_protocol::FileInfo {
                                path,
                                is_directory,
                                size,
                                modified,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Mirror of the streaming path: turn image local_files into
            // proper attachments so the LLM sees the picture instead of
            // being told to read it via an unsupported tool.
            promote_image_local_files_to_attachments(&mut attachments, &mut local_files);

            // Extract tab_id if provided (from browser sidebar)
            let tab_id = payload
                .get("payload")
                .and_then(|p| p.get("tab_id"))
                .and_then(|t| t.as_i64());

            // Extract tab_ids list if provided (all available tabs)
            let tab_ids: Vec<nevoflux_builtin_wasm::TabInfo> = payload
                .get("payload")
                .and_then(|p| p.get("tab_ids"))
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let space = v
                                .get("space")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let tab_id = v.get("tab_id").and_then(|t| t.as_i64())?;
                            let tab_title = v
                                .get("tab_title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            let url = v
                                .get("url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(nevoflux_builtin_wasm::TabInfo {
                                space,
                                tab_id,
                                tab_title,
                                url,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            debug!(
                "Processing chat message with mode={:?}, session={}, attachments={}, local_files={}, tab_id={:?}, tab_ids={}",
                mode,
                session_id,
                attachments.len(),
                local_files.len(),
                tab_id,
                tab_ids.len()
            );

            // Ensure session exists and save user message
            match session_manager.get_or_create_session(&session_id).await {
                Ok(session) => {
                    info!(
                        "Session ready: id={}, created_at={}",
                        session.id, session.created_at
                    );
                }
                Err(e) => {
                    error!("Failed to get/create session {}: {}", session_id, e);
                }
            }

            // Build attachment metadata for history display (no base64 data stored)
            let attachment_metadata = build_attachment_metadata(&attachments, &local_files);

            // Save user message to database
            let mut generated_title: Option<String> = None;
            match session_manager
                .add_message_with_metadata(
                    &session_id,
                    MessageRole::User,
                    message_content,
                    attachment_metadata,
                )
                .await
            {
                Ok(msg) => {
                    info!("Saved user message: id={}, session={}", msg.id, session_id);

                    // Generate title from first message if session has no title yet
                    match session_manager.generate_title(&session_id).await {
                        Ok(Some(title)) => {
                            info!("Generated session title: {}", title);
                            generated_title = Some(title);
                        }
                        Ok(None) => {
                            // Session already has a title or no messages
                        }
                        Err(e) => {
                            error!("Failed to generate title: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to save user message to {}: {}", session_id, e);
                }
            }

            // Create host functions with config and runtime
            let mut host = DaemonHostFunctions::new(config.clone(), runtime)
                .with_services(services.clone())
                .with_session_id(session_id.clone())
                .with_canvas_video_service(canvas_video_service.clone());

            // Pass skill base path to host for relative path resolution
            if let Some(ref ctx) = skill_context {
                if !ctx.base_path.is_empty() {
                    host = host.with_skill_base_path(&ctx.base_path);
                }
            }

            // Create agent with host functions
            let agent = Agent::new(host);

            // Load MCP server names for system prompt injection
            let mcp_servers: Vec<String> = crate::mcp_config::McpServersConfig::load()
                .map(|c| {
                    c.servers
                        .iter()
                        .filter(|s| s.enabled)
                        .map(|s| s.name.clone())
                        .collect()
                })
                .unwrap_or_default();

            // Build agent input with skill context injected into system prompt
            let input = AgentInput {
                session_id: session_id.clone(),
                mode,
                user_message,
                history: load_session_history(
                    session_manager,
                    &session_id,
                    config.daemon.context.max_history_messages,
                )
                .await,
                attachments,
                local_files,
                custom_system_prompt: None, // Use default mode-based prompt
                tab_id,
                tab_ids,
                skill_context,
                available_models: config.llm.configured_providers(),
                mcp_servers,
                soul_context: build_soul_context(&services),
                tools_config: None,
                os_platform: Some(std::env::consts::OS.to_string()),
            };

            // Run agent
            match agent.run(&input) {
                Ok(output) => {
                    // orchestrate is a tool call handled inside the agent loop.
                    let final_text = output.text.clone();

                    // Save assistant response to database
                    if !final_text.is_empty() {
                        match session_manager
                            .add_message(&session_id, MessageRole::Assistant, &final_text)
                            .await
                        {
                            Ok(msg) => {
                                info!(
                                    "Saved assistant message: id={}, session={}",
                                    msg.id, session_id
                                );
                            }
                            Err(e) => {
                                error!("Failed to save assistant message to {}: {}", session_id, e);
                            }
                        }
                    }

                    // Save tool calls to session history
                    for tool_call in &output.tool_calls {
                        if let Err(e) = session_manager
                            .add_tool_use_message(
                                &session_id,
                                &tool_call.id,
                                &tool_call.name,
                                &tool_call.arguments,
                                None,
                            )
                            .await
                        {
                            error!("Failed to save tool call {}: {}", tool_call.name, e);
                        }
                    }

                    let mut response = serde_json::json!({
                        "type": "stream_chunk",
                        "payload": {
                            "content": final_text,
                            "tool_calls": output.tool_calls,
                            "done": true
                        }
                    });

                    // Include session title if generated
                    if let Some(title) = generated_title {
                        response["payload"]["session_title"] = serde_json::Value::String(title);
                    }

                    response
                }
                Err(e) => {
                    error!("Agent run failed: {}", e);
                    serde_json::json!({
                        "type": "error",
                        "payload": {
                            "code": "AGENT_ERROR",
                            "message": format!("Agent error: {}", e)
                        }
                    })
                }
            }
        }
        "stop_generation" => {
            serde_json::json!({
                "type": "agent_state",
                "payload": {
                    "state": "idle",
                    "message": "Generation stopped"
                }
            })
        }
        "system_command" | "agent:command" => {
            let inner_payload = payload.get("payload");

            let command = inner_payload
                .and_then(|p| p.get("command"))
                .and_then(|c| c.as_str())
                .unwrap_or("");

            let request_id = inner_payload
                .and_then(|p| p.get("request_id"))
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();

            let mut params = inner_payload
                .and_then(|p| p.get("params"))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            // Add request_id to params for handlers
            if let Some(obj) = params.as_object_mut() {
                obj.insert("request_id".to_string(), serde_json::json!(request_id));
            }

            match command {
                "status" => {
                    let config = AgentConfig::load().unwrap_or_default();
                    let has_configured = has_any_configured_provider(&config.llm);

                    // Asset & Stream Plane handshake (bridge:hello.asset_plane).
                    // The extension caches `port` + `bearer_token`; on session
                    // change it invalidates URL caches. If the AssetServer
                    // failed to bind, advertise nothing — extension falls back
                    // to NM-only.
                    let asset_plane_value = match services.asset_server.as_ref() {
                        Some(asset_server) => {
                            // If the extension reported its origin in this
                            // status call, lock the CORS allow-origin to it.
                            if let Some(origin) =
                                params.get("origin").and_then(|v| v.as_str())
                            {
                                if !origin.is_empty() {
                                    asset_server
                                        .set_allowed_origin(Some(origin.to_string()));
                                }
                            }
                            serde_json::to_value(asset_server.asset_plane_info())
                                .unwrap_or(serde_json::Value::Null)
                        }
                        None => serde_json::Value::Null,
                    };

                    serde_json::json!({
                        "type": "system_response",
                        "payload": {
                            "request_id": request_id,
                            "command": "status",
                            "success": true,
                            "data": {
                                "status": "ok",
                                "version": env!("CARGO_PKG_VERSION"),
                                "first_run": !has_configured,
                                "has_configured_provider": has_configured,
                                "asset_plane": asset_plane_value,
                            }
                        }
                    })
                }
                "session.resolve" => handle_session_resolve(session_manager, &params).await,
                "session.list" => handle_session_list(session_manager, &params).await,
                "session.clone" => handle_session_clone(session_manager, &params).await,
                "session.delete" => handle_session_delete(session_manager, &params).await,
                "session.rename" => handle_session_rename(session_manager, &params).await,
                "session.pin" => handle_session_pin(session_manager, &params, true).await,
                "session.unpin" => handle_session_pin(session_manager, &params, false).await,
                // MCP server configuration commands
                "mcp.list" => handle_mcp_list(&params).await,
                "mcp.add" => handle_mcp_add(&params).await,
                "mcp.update" => handle_mcp_update(&params).await,
                "mcp.delete" => handle_mcp_delete(&params).await,
                "mcp.test" => handle_mcp_test(&params).await,
                "mcp.connect" => handle_mcp_connect(&params).await,
                "mcp.disconnect" => handle_mcp_disconnect(&params).await,
                "file.pick" => handle_file_pick(&params).await,
                "skill.list" => handle_skill_list(services, &params).await,
                // LLM provider configuration commands
                "config.llm.list" => handle_config_llm_list(&params).await,
                "config.llm.get" => handle_config_llm_get(&params).await,
                "config.llm.set" => handle_config_llm_set(&params, shared_config).await,
                // OpenClaw model configuration commands
                // Wrap in system_response envelope (handlers return raw data)
                cmd @ ("config.openclaw.model.list"
                | "config.openclaw.model.set"
                | "config.openclaw.model.delete"
                | "config.openclaw.status") => {
                    let data = match cmd {
                        "config.openclaw.model.list" => handle_openclaw_model_list().await,
                        "config.openclaw.model.set" => handle_openclaw_model_set(&params).await,
                        "config.openclaw.model.delete" => {
                            handle_openclaw_model_delete(&params).await
                        }
                        "config.openclaw.status" => handle_openclaw_status().await,
                        _ => unreachable!(),
                    };
                    let success = data
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    serde_json::json!({
                        "type": "system_response",
                        "payload": {
                            "request_id": request_id,
                            "command": cmd,
                            "success": success,
                            "data": data
                        }
                    })
                }
                // Agent config file commands
                "config.file.read" => handle_config_file_read(&params).await,
                "config.file.write" => handle_config_file_write(&params).await,
                // Artifact persistence commands
                "artifact.get" => handle_artifact_get(session_manager, &params).await,
                "artifact.list" => handle_artifact_list(session_manager, &params).await,
                // ContentStore persistence commands
                "content_store.set" => {
                    let key = params.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    if key.is_empty() {
                        serde_json::json!({
                            "type": "system_response",
                            "payload": {
                                "request_id": request_id,
                                "command": "content_store.set",
                                "success": false,
                                "error": {
                                    "code": "MISSING_PARAM",
                                    "message": "Missing key parameter"
                                }
                            }
                        })
                    } else {
                        let value = params
                            .get("value")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        match session_manager.set_config(key, value.clone()) {
                            Ok(()) => {
                                // Mirror canvas ContentStore writes into the
                                // artifacts table so canvas.share picks up the
                                // latest edits instead of the creation-time
                                // snapshot. Best-effort; failures are logged
                                // inside the helper and never fail the write.
                                if key.starts_with("canvas:") {
                                    mirror_canvas_to_artifacts_table(session_manager, key, &value);
                                }
                                serde_json::json!({
                                    "type": "system_response",
                                    "payload": {
                                        "request_id": request_id,
                                        "command": "content_store.set",
                                        "success": true,
                                        "data": { "key": key }
                                    }
                                })
                            }
                            Err(e) => serde_json::json!({
                                "type": "system_response",
                                "payload": {
                                    "request_id": request_id,
                                    "command": "content_store.set",
                                    "success": false,
                                    "error": {
                                        "code": "STORAGE_ERROR",
                                        "message": format!("{}", e)
                                    }
                                }
                            }),
                        }
                    }
                }
                "content_store.delete" => {
                    let key = params.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    if key.is_empty() {
                        serde_json::json!({
                            "type": "system_response",
                            "payload": {
                                "request_id": request_id,
                                "command": "content_store.delete",
                                "success": false,
                                "error": {
                                    "code": "MISSING_PARAM",
                                    "message": "Missing key parameter"
                                }
                            }
                        })
                    } else {
                        match session_manager.delete_config(key) {
                            Ok(deleted) => serde_json::json!({
                                "type": "system_response",
                                "payload": {
                                    "request_id": request_id,
                                    "command": "content_store.delete",
                                    "success": true,
                                    "data": { "key": key, "deleted": deleted }
                                }
                            }),
                            Err(e) => serde_json::json!({
                                "type": "system_response",
                                "payload": {
                                    "request_id": request_id,
                                    "command": "content_store.delete",
                                    "success": false,
                                    "error": {
                                        "code": "STORAGE_ERROR",
                                        "message": format!("{}", e)
                                    }
                                }
                            }),
                        }
                    }
                }
                "content_store.load" => {
                    let prefix = params.get("prefix").and_then(|p| p.as_str()).unwrap_or("");
                    let result = if prefix.is_empty() {
                        session_manager.list_config()
                    } else {
                        session_manager.list_config_by_prefix(prefix)
                    };
                    match result {
                        Ok(entries) => {
                            let count = entries.len();
                            let entries_json: Vec<serde_json::Value> = entries
                                .into_iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "key": e.key,
                                        "value": e.value,
                                        "updated_at": e.updated_at
                                    })
                                })
                                .collect();
                            serde_json::json!({
                                "type": "system_response",
                                "payload": {
                                    "request_id": request_id,
                                    "command": "content_store.load",
                                    "success": true,
                                    "data": {
                                        "entries": entries_json,
                                        "count": count
                                    }
                                }
                            })
                        }
                        Err(e) => serde_json::json!({
                            "type": "system_response",
                            "payload": {
                                "request_id": request_id,
                                "command": "content_store.load",
                                "success": false,
                                "error": {
                                    "code": "STORAGE_ERROR",
                                    "message": format!("{}", e)
                                }
                            }
                        }),
                    }
                }
                _ => {
                    serde_json::json!({
                        "type": "system_response",
                        "payload": {
                            "request_id": request_id,
                            "command": command,
                            "success": false,
                            "error": {
                                "code": "UNKNOWN_COMMAND",
                                "message": format!("Unknown command: {}", command)
                            }
                        }
                    })
                }
            }
        }
        _ => {
            debug!("Unknown chat message type: {}", msg_type);
            serde_json::json!({
                "type": "error",
                "payload": {
                    "code": "UNKNOWN_MESSAGE_TYPE",
                    "message": format!("Unknown message type: {}", msg_type)
                }
            })
        }
    }
}

/// Handle session.resolve command.
///
/// Resolves a session by ID, creating it if it doesn't exist.
/// Returns the session info and its messages.
async fn handle_session_resolve(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    // Extract request_id for response correlation
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let session_id = match params.get("session_id").and_then(|s| s.as_str()) {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.resolve",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing session_id parameter"
                    }
                }
            });
        }
    };

    info!("Resolving session: {}", session_id);

    // Try to get or create the session
    match session_manager.get_or_create_session(session_id).await {
        Ok(session) => {
            // Check if this was a new session by seeing if updated_at equals created_at
            let created = session.created_at == session.updated_at;
            info!("Session resolved: id={}, created={}", session.id, created);

            // Get messages for the session
            let messages = match session_manager.get_messages(&session.id).await {
                Ok(msgs) => {
                    // Debug: log content_type distribution
                    let type_counts: std::collections::HashMap<String, usize> =
                        msgs.iter()
                            .fold(std::collections::HashMap::new(), |mut acc, m| {
                                *acc.entry(m.content_type.as_str().to_string()).or_insert(0) += 1;
                                acc
                            });
                    info!(
                        "Found {} messages for session {} (content_types: {:?})",
                        msgs.len(),
                        session.id,
                        type_counts
                    );
                    msgs.into_iter()
                        .map(|m| {
                            let mut msg = serde_json::json!({
                                "id": m.id,
                                "role": format!("{:?}", m.role).to_lowercase(),
                                "content": m.content,
                                "content_type": m.content_type.as_str(),
                                "created_at": m.created_at
                            });
                            if let Some(metadata) = m.metadata {
                                msg.as_object_mut()
                                    .unwrap()
                                    .insert("metadata".to_string(), serde_json::json!(metadata));
                            }
                            msg
                        })
                        .collect::<Vec<_>>()
                }
                Err(e) => {
                    error!("Failed to get messages for {}: {}", session.id, e);
                    vec![]
                }
            };

            // Get artifacts for the session
            let artifacts: Vec<serde_json::Value> =
                match session_manager.list_artifacts(&session.id) {
                    Ok(arts) => arts
                        .into_iter()
                        .map(|a| {
                            serde_json::json!({
                                "id": a.id,
                                "title": a.title,
                                "description": a.description,
                                "content_type": a.content_type,
                                "created_at": a.created_at
                            })
                        })
                        .collect(),
                    Err(e) => {
                        error!("Failed to get artifacts for {}: {}", session.id, e);
                        vec![]
                    }
                };

            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.resolve",
                    "success": true,
                    "data": {
                        "session": {
                            "id": session.id,
                            "title": session.title,
                            "mode": format!("{:?}", session.mode).to_lowercase(),
                            "created_at": session.created_at,
                            "updated_at": session.updated_at
                        },
                        "messages": messages,
                        "artifacts": artifacts,
                        "created": created
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to resolve session: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.resolve",
                    "success": false,
                    "error": {
                        "code": "RESOLVE_FAILED",
                        "message": format!("Failed to resolve session: {}", e)
                    }
                }
            })
        }
    }
}

/// Handle session.list command.
///
/// Lists sessions with optional pagination.
async fn handle_session_list(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as u32;
    let offset = params.get("offset").and_then(|o| o.as_u64()).unwrap_or(0) as u32;
    let exclude_empty = params
        .get("exclude_empty")
        .and_then(|e| e.as_bool())
        .unwrap_or(true);

    let list_params = ListSessionsParams::new()
        .with_limit(limit)
        .with_offset(offset)
        .exclude_empty(exclude_empty);

    match session_manager.list_sessions(list_params).await {
        Ok(sessions) => {
            info!(
                "session.list: returning {} sessions (exclude_empty={})",
                sessions.len(),
                exclude_empty
            );
            // Get message counts for each session
            let mut session_summaries = Vec::new();
            for session in &sessions {
                let message_count = session_manager
                    .get_message_count(&session.id)
                    .await
                    .unwrap_or(0);

                session_summaries.push(serde_json::json!({
                    "id": session.id,
                    "title": session.title,
                    "updated_at": session.updated_at,
                    "message_count": message_count,
                    "pinned": session.pinned
                }));
            }

            // Get total count (matching the same filter)
            let total = session_manager
                .get_session_count_filtered(false, exclude_empty)
                .await
                .unwrap_or(0);

            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.list",
                    "success": true,
                    "data": {
                        "sessions": session_summaries,
                        "total": total
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to list sessions: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.list",
                    "success": false,
                    "error": {
                        "code": "LIST_FAILED",
                        "message": format!("Failed to list sessions: {}", e)
                    }
                }
            })
        }
    }
}

/// Handle session.clone command.
///
/// Clones messages from a source session to a new target session.
async fn handle_session_clone(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let source_id = match params.get("source_id").and_then(|s| s.as_str()) {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.clone",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing source_id parameter"
                    }
                }
            });
        }
    };

    let target_id = match params.get("target_id").and_then(|s| s.as_str()) {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.clone",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing target_id parameter"
                    }
                }
            });
        }
    };

    // Get source messages
    let source_messages = match session_manager.get_messages(source_id).await {
        Ok(msgs) => msgs,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.clone",
                    "success": false,
                    "error": {
                        "code": "SOURCE_NOT_FOUND",
                        "message": format!("Failed to get source messages: {}", e)
                    }
                }
            });
        }
    };

    // Get source session for title
    let source_title = session_manager
        .get_session(source_id)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.title);

    // Create target session with same title
    let target_session = match session_manager
        .create_session(Some(target_id.to_string()), source_title)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.clone",
                    "success": false,
                    "error": {
                        "code": "CREATE_FAILED",
                        "message": format!("Failed to create target session: {}", e)
                    }
                }
            });
        }
    };

    // Copy messages to target session
    let mut cloned_messages = Vec::new();
    for msg in &source_messages {
        match session_manager
            .add_message(target_id, msg.role, &msg.content)
            .await
        {
            Ok(new_msg) => {
                cloned_messages.push(serde_json::json!({
                    "id": new_msg.id,
                    "role": format!("{:?}", new_msg.role).to_lowercase(),
                    "content": new_msg.content,
                    "created_at": new_msg.created_at
                }));
            }
            Err(e) => {
                error!("Failed to clone message: {}", e);
            }
        }
    }

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "session.clone",
            "success": true,
            "data": {
                "session": {
                    "id": target_session.id,
                    "title": target_session.title,
                    "mode": format!("{:?}", target_session.mode).to_lowercase(),
                    "created_at": target_session.created_at,
                    "updated_at": target_session.updated_at
                },
                "messages": cloned_messages
            }
        }
    })
}

/// Handle session.delete command.
///
/// Deletes a session by ID.
async fn handle_session_delete(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let session_id = match params.get("session_id").and_then(|s| s.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.delete",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing or empty session_id parameter"
                    }
                }
            });
        }
    };

    match session_manager.delete_session(session_id).await {
        Ok(deleted) => {
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.delete",
                    "success": true,
                    "data": {
                        "id": session_id,
                        "deleted": deleted
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to delete session: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.delete",
                    "success": false,
                    "error": {
                        "code": "DELETE_FAILED",
                        "message": format!("Failed to delete session: {}", e)
                    }
                }
            })
        }
    }
}

/// Handle session.rename command.
///
/// Renames a session by setting its title.
async fn handle_session_rename(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let session_id = match params.get("session_id").and_then(|s| s.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.rename",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing or empty session_id parameter"
                    }
                }
            });
        }
    };

    let title = match params.get("title").and_then(|t| t.as_str()) {
        Some(t) if !t.is_empty() => t,
        _ => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.rename",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing or empty title parameter"
                    }
                }
            });
        }
    };

    match session_manager.set_title(session_id, title).await {
        Ok(session) => {
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.rename",
                    "success": true,
                    "data": {
                        "id": session.id,
                        "title": session.title
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to rename session: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "session.rename",
                    "success": false,
                    "error": {
                        "code": "RENAME_FAILED",
                        "message": format!("Failed to rename session: {}", e)
                    }
                }
            })
        }
    }
}

/// Handle session.pin and session.unpin commands.
///
/// Pins or unpins a session.
async fn handle_session_pin(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
    pin: bool,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let command = if pin { "session.pin" } else { "session.unpin" };

    let session_id = match params.get("session_id").and_then(|s| s.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": command,
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing or empty session_id parameter"
                    }
                }
            });
        }
    };

    let result = if pin {
        session_manager.pin_session(session_id).await
    } else {
        session_manager.unpin_session(session_id).await
    };

    match result {
        Ok(session) => {
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": command,
                    "success": true,
                    "data": {
                        "id": session.id,
                        "pinned": session.pinned
                    }
                }
            })
        }
        Err(e) => {
            error!(
                "Failed to {} session: {}",
                if pin { "pin" } else { "unpin" },
                e
            );
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": command,
                    "success": false,
                    "error": {
                        "code": if pin { "PIN_FAILED" } else { "UNPIN_FAILED" },
                        "message": format!("Failed to {} session: {}", if pin { "pin" } else { "unpin" }, e)
                    }
                }
            })
        }
    }
}

/// Handle artifact.get command.
async fn handle_artifact_get(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let artifact_id = match params.get("artifact_id").and_then(|s| s.as_str()) {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "artifact.get",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing artifact_id parameter"
                    }
                }
            });
        }
    };

    info!("artifact.get: looking up artifact_id={}", artifact_id);

    match session_manager.get_artifact(artifact_id) {
        Ok(Some(artifact)) => {
            info!(
                "artifact.get: found artifact {} (title={}, content_len={})",
                artifact.id,
                artifact.title,
                artifact.content.len()
            );
            let files_json = artifact
                .files
                .as_ref()
                .map(|f| serde_json::to_value(f).unwrap_or_default());
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "artifact.get",
                    "success": true,
                    "data": {
                        "id": artifact.id,
                        "session_id": artifact.session_id,
                        "title": artifact.title,
                        "description": artifact.description,
                        "content_type": artifact.content_type,
                        "content": artifact.content,
                        "files": files_json,
                        "entry": artifact.entry,
                        "created_at": artifact.created_at
                    }
                }
            })
        }
        Ok(None) => {
            warn!("artifact.get: artifact not found: {}", artifact_id);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "artifact.get",
                    "success": false,
                    "error": {
                        "code": "NOT_FOUND",
                        "message": format!("Artifact not found: {}", artifact_id)
                    }
                }
            })
        }
        Err(e) => serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "artifact.get",
                "success": false,
                "error": {
                    "code": "STORAGE_ERROR",
                    "message": format!("{}", e)
                }
            }
        }),
    }
}

/// Handle artifact.list command.
async fn handle_artifact_list(
    session_manager: &Arc<SessionManager>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let session_id = match params.get("session_id").and_then(|s| s.as_str()) {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "artifact.list",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing session_id parameter"
                    }
                }
            });
        }
    };

    match session_manager.list_artifacts(session_id) {
        Ok(artifacts) => {
            let artifacts_json: Vec<serde_json::Value> = artifacts
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "title": a.title,
                        "description": a.description,
                        "content_type": a.content_type,
                        "created_at": a.created_at
                    })
                })
                .collect();
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "artifact.list",
                    "success": true,
                    "data": {
                        "artifacts": artifacts_json
                    }
                }
            })
        }
        Err(e) => serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "artifact.list",
                "success": false,
                "error": {
                    "code": "STORAGE_ERROR",
                    "message": format!("{}", e)
                }
            }
        }),
    }
}

/// Handle MCP channel messages
async fn handle_mcp_message(payload: &serde_json::Value) -> serde_json::Value {
    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "mcp_request" => {
            // TODO: Implement MCP request handling
            serde_json::json!({
                "type": "mcp_response",
                "payload": {
                    "jsonrpc": "2.0",
                    "id": payload.get("payload").and_then(|p| p.get("id")).cloned().unwrap_or(serde_json::json!(null)),
                    "error": {
                        "code": -32601,
                        "message": "MCP not yet implemented"
                    }
                }
            })
        }
        _ => {
            serde_json::json!({
                "type": "error",
                "payload": {
                    "code": "UNKNOWN_MCP_TYPE",
                    "message": format!("Unknown MCP message type: {}", msg_type)
                }
            })
        }
    }
}

// ============================================
// MCP Configuration Handlers
// ============================================

use crate::mcp_config::{McpServerConfigFile, McpServersConfig};

/// Extract a McpServerConfigFile from a JSON value.
fn extract_server_config(params: &serde_json::Value) -> McpServerConfigFile {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    let server_type = params
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("stdio");
    let enabled = params
        .get("enabled")
        .and_then(|e| e.as_bool())
        .unwrap_or(true);
    let description = params
        .get("description")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    let command = params
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());
    let args: Vec<String> = params
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let env: HashMap<String, String> = params
        .get("env")
        .and_then(|e| e.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let work_dir = params
        .get("work_dir")
        .and_then(|w| w.as_str())
        .map(|s| s.to_string());

    let url = params
        .get("url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());
    let timeout = params.get("timeout").and_then(|t| t.as_u64());
    let headers: Option<HashMap<String, String>> = params
        .get("headers")
        .and_then(|h| h.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        });
    let reconnect = params.get("reconnect").and_then(|r| r.as_u64());
    let method = params
        .get("method")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());
    let api_key = params
        .get("api_key")
        .and_then(|a| a.as_str())
        .map(|s| s.to_string());

    McpServerConfigFile {
        name: name.to_string(),
        server_type: server_type.to_string(),
        enabled,
        description,
        command,
        args,
        env,
        work_dir,
        url,
        timeout,
        headers,
        reconnect,
        method,
        api_key,
    }
}

/// Handle mcp.list command.
///
/// Returns list of configured MCP servers and their connection status.
async fn handle_mcp_list(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    // Load MCP servers config from file
    match McpServersConfig::load() {
        Ok(config) => {
            let servers: Vec<serde_json::Value> = config
                .servers
                .iter()
                .map(|s| {
                    let mut obj = serde_json::json!({
                        "name": s.name,
                        "type": s.server_type,
                        "enabled": s.enabled,
                    });
                    let m = obj.as_object_mut().unwrap();
                    if let Some(ref desc) = s.description {
                        m.insert("description".into(), serde_json::json!(desc));
                    }
                    if let Some(ref cmd) = s.command {
                        m.insert("command".into(), serde_json::json!(cmd));
                    }
                    if !s.args.is_empty() {
                        m.insert("args".into(), serde_json::json!(s.args));
                    }
                    if !s.env.is_empty() {
                        m.insert("env".into(), serde_json::json!(s.env));
                    }
                    if let Some(ref wd) = s.work_dir {
                        m.insert("work_dir".into(), serde_json::json!(wd));
                    }
                    if let Some(ref url) = s.url {
                        m.insert("url".into(), serde_json::json!(url));
                    }
                    if let Some(t) = s.timeout {
                        m.insert("timeout".into(), serde_json::json!(t));
                    }
                    if let Some(ref h) = s.headers {
                        m.insert("headers".into(), serde_json::json!(h));
                    }
                    if let Some(r) = s.reconnect {
                        m.insert("reconnect".into(), serde_json::json!(r));
                    }
                    if let Some(ref method) = s.method {
                        m.insert("method".into(), serde_json::json!(method));
                    }
                    if let Some(ref ak) = s.api_key {
                        m.insert("api_key".into(), serde_json::json!(ak));
                    }
                    obj
                })
                .collect();

            // TODO: Get connected servers from MCP registry
            let connected: Vec<String> = Vec::new();

            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.list",
                    "success": true,
                    "data": {
                        "servers": servers,
                        "connected": connected
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to load MCP config: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.list",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load MCP config: {}", e)
                    }
                }
            })
        }
    }
}

/// Handle mcp.add command.
///
/// Adds a new MCP server configuration.
async fn handle_mcp_add(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    // Extract server config from params
    let server_params = match params.get("server") {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.add",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing server parameter"
                    }
                }
            });
        }
    };

    let server = extract_server_config(server_params);
    let server_name = server.name.clone();

    // Load existing config, add server, and save
    let mut config = match McpServersConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.add",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load MCP config: {}", e)
                    }
                }
            });
        }
    };

    if let Err(e) = config.add_server(server) {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.add",
                "success": false,
                "error": {
                    "code": "ADD_FAILED",
                    "message": format!("Failed to add server: {}", e)
                }
            }
        });
    }

    if let Err(e) = config.save() {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.add",
                "success": false,
                "error": {
                    "code": "SAVE_FAILED",
                    "message": format!("Failed to save MCP config: {}", e)
                }
            }
        });
    }

    info!("Added MCP server: {}", server_name);
    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "mcp.add",
            "success": true,
            "data": {
                "name": server_name
            }
        }
    })
}

/// Handle mcp.update command.
///
/// Updates an existing MCP server configuration.
async fn handle_mcp_update(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.update",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing name parameter"
                    }
                }
            });
        }
    };

    let server_params = match params.get("server") {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.update",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing server parameter"
                    }
                }
            });
        }
    };

    let server = extract_server_config(server_params);

    let mut config = match McpServersConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.update",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load MCP config: {}", e)
                    }
                }
            });
        }
    };

    if let Err(e) = config.update_server(name, server) {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.update",
                "success": false,
                "error": {
                    "code": "UPDATE_FAILED",
                    "message": format!("Failed to update server: {}", e)
                }
            }
        });
    }

    if let Err(e) = config.save() {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.update",
                "success": false,
                "error": {
                    "code": "SAVE_FAILED",
                    "message": format!("Failed to save MCP config: {}", e)
                }
            }
        });
    }

    info!("Updated MCP server: {}", name);
    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "mcp.update",
            "success": true,
            "data": {
                "name": name
            }
        }
    })
}

/// Handle mcp.delete command.
///
/// Deletes an MCP server configuration.
async fn handle_mcp_delete(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.delete",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing name parameter"
                    }
                }
            });
        }
    };

    let mut config = match McpServersConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.delete",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load MCP config: {}", e)
                    }
                }
            });
        }
    };

    if !config.remove_server(name) {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.delete",
                "success": false,
                "error": {
                    "code": "NOT_FOUND",
                    "message": format!("Server not found: {}", name)
                }
            }
        });
    }

    if let Err(e) = config.save() {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "mcp.delete",
                "success": false,
                "error": {
                    "code": "SAVE_FAILED",
                    "message": format!("Failed to save MCP config: {}", e)
                }
            }
        });
    }

    info!("Deleted MCP server: {}", name);
    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "mcp.delete",
            "success": true,
            "data": {
                "name": name
            }
        }
    })
}

/// Handle mcp.test command.
///
/// Tests connection to an MCP server by temporarily connecting and listing tools.
async fn handle_mcp_test(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing name parameter"
                    }
                }
            });
        }
    };

    // Load config and find server
    let config = match McpServersConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load MCP config: {}", e)
                    }
                }
            });
        }
    };

    let server = match config.get_server(name) {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": false,
                    "error": {
                        "code": "NOT_FOUND",
                        "message": format!("Server not found: {}", name)
                    }
                }
            });
        }
    };

    // Build the command line (only stdio servers can be tested this way)
    let command = match &server.command {
        Some(c) => c,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": false,
                    "error": {
                        "code": "UNSUPPORTED",
                        "message": format!("Only stdio servers can be tested, server '{}' is type '{}'", name, server.server_type)
                    }
                }
            });
        }
    };
    let args = &server.args;

    info!(
        "Testing MCP server '{}': {} {}",
        name,
        command,
        args.join(" ")
    );

    // Try to spawn the process and communicate with it
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Set environment variables
    for (key, value) in &server.env {
        cmd.env(key, value);
    }

    match cmd.spawn() {
        Ok(mut child) => {
            // Send initialize request
            let stdin = child.stdin.as_mut();
            if let Some(stdin) = stdin {
                use tokio::io::AsyncWriteExt;
                let init_request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "nevoflux-agent",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                });
                let request_str = format!("{}\n", serde_json::to_string(&init_request).unwrap());

                if let Err(e) = stdin.write_all(request_str.as_bytes()).await {
                    let _ = child.kill().await;
                    return serde_json::json!({
                        "type": "system_response",
                        "payload": {
                            "request_id": request_id,
                            "command": "mcp.test",
                            "success": true,
                            "data": {
                                "name": name,
                                "success": false,
                                "message": format!("Failed to write to stdin: {}", e),
                                "tools_count": 0
                            }
                        }
                    });
                }
            }

            // Wait a bit and kill the process
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = child.kill().await;

            // Process started successfully
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": true,
                    "data": {
                        "name": name,
                        "success": true,
                        "message": "Server started successfully",
                        "tools_count": 0
                    }
                }
            })
        }
        Err(e) => {
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.test",
                    "success": true,
                    "data": {
                        "name": name,
                        "success": false,
                        "message": format!("Failed to start server: {}", e),
                        "tools_count": 0
                    }
                }
            })
        }
    }
}

/// Handle mcp.connect command.
///
/// Connects to an MCP server.
async fn handle_mcp_connect(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.connect",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing name parameter"
                    }
                }
            });
        }
    };

    // TODO: Implement actual MCP connection via MCP registry
    // For now, just acknowledge the request
    info!("MCP connect requested for: {}", name);

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "mcp.connect",
            "success": true,
            "data": {
                "name": name,
                "connected": false,
                "message": "Connection management not yet implemented"
            }
        }
    })
}

/// Handle mcp.disconnect command.
///
/// Disconnects from an MCP server.
async fn handle_mcp_disconnect(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "mcp.disconnect",
                    "success": false,
                    "error": {
                        "code": "MISSING_PARAM",
                        "message": "Missing name parameter"
                    }
                }
            });
        }
    };

    // TODO: Implement actual MCP disconnection via MCP registry
    // For now, just acknowledge the request
    info!("MCP disconnect requested for: {}", name);

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "mcp.disconnect",
            "success": true,
            "data": {
                "name": name,
                "connected": false
            }
        }
    })
}

/// Handle file.pick command.
///
/// Opens a native file picker dialog and returns selected files.
async fn handle_file_pick(params: &serde_json::Value) -> serde_json::Value {
    use crate::file_picker::pick_files;
    use nevoflux_protocol::{PickFilesRequest, PickerMode};

    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    // Parse picker mode
    let mode = params
        .get("mode")
        .and_then(|m| m.as_str())
        .map(|m| match m {
            "files" => PickerMode::Files,
            "directories" => PickerMode::Directories,
            _ => PickerMode::Both,
        })
        .unwrap_or(PickerMode::Both);

    let multiple = params
        .get("multiple")
        .and_then(|m| m.as_bool())
        .unwrap_or(false);

    let title = params
        .get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    let default_path = params
        .get("default_path")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());

    // On Linux and Windows, "both" mode is not reliably supported in a single dialog.
    // On Linux, rfd cannot select both files and directories simultaneously.
    // On Windows, the PowerShell BrowseForFolder fallback has compatibility issues.
    // Ask the sidebar to let the user choose between files or directories,
    // then re-send file.pick with the specific mode.
    // macOS handles "both" natively via osascript.
    #[cfg(not(target_os = "macos"))]
    if mode == PickerMode::Both {
        info!("File picker: Both mode not natively supported, asking sidebar to choose mode");
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "file.pick",
                "success": true,
                "data": {
                    "choose_mode": true,
                    "options": ["files", "directories"],
                    "message": "Select what to pick"
                }
            }
        });
    }

    let req = PickFilesRequest {
        mode,
        multiple,
        title,
        default_path,
    };

    info!(
        "File picker requested: mode={:?}, multiple={}",
        req.mode, req.multiple
    );

    // Timeout after 120 seconds to prevent the file picker lock from being held forever
    let pick_result =
        tokio::time::timeout(std::time::Duration::from_secs(120), pick_files(req)).await;

    let pick_result = match pick_result {
        Ok(result) => result,
        Err(_) => {
            warn!("File picker timed out after 120 seconds");
            Err(nevoflux_protocol::PickFilesError::DialogFailed(
                "File picker timed out after 120 seconds".to_string(),
            ))
        }
    };

    match pick_result {
        Ok(response) => {
            let files: Vec<serde_json::Value> = response
                .files
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "path": f.path,
                        "is_directory": f.is_directory,
                        "size": f.size,
                        "modified": f.modified
                    })
                })
                .collect();

            info!(
                "File picker completed: {} files selected, cancelled={}",
                files.len(),
                response.cancelled
            );

            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "file.pick",
                    "success": true,
                    "data": {
                        "files": files,
                        "cancelled": response.cancelled
                    }
                }
            })
        }
        Err(e) => {
            error!("File picker failed: {:?}", e);

            let (code, message) = match e {
                nevoflux_protocol::PickFilesError::NoDisplay => {
                    ("NO_DISPLAY", "No graphical display available".to_string())
                }
                nevoflux_protocol::PickFilesError::AlreadyPicking => (
                    "ALREADY_PICKING",
                    "A file picker dialog is already open".to_string(),
                ),
                nevoflux_protocol::PickFilesError::DialogFailed(msg) => ("DIALOG_FAILED", msg),
            };

            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "file.pick",
                    "success": false,
                    "error": {
                        "code": code,
                        "message": message
                    }
                }
            })
        }
    }
}

/// Handle skill.list system command.
///
/// Lists all available skills from the skill registry.
async fn handle_skill_list(
    services: &HostServices,
    params: &serde_json::Value,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let registry = services.skills.read().await;
    let summaries = registry.list();

    let skills: Vec<_> = summaries
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "description": s.description,
                "tags": s.tags,
                "source": s.source,
                "enabled": s.enabled
            })
        })
        .collect();

    info!("skill.list: returning {} skills", skills.len());

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "skill.list",
            "success": true,
            "data": { "skills": skills }
        }
    })
}

// ============================================
// LLM Config Handlers
// ============================================

/// Provider metadata for the LLM provider list.
struct ProviderMeta {
    id: &'static str,
    display_name: &'static str,
    provider_type: &'static str,
    /// Embedded icon bytes (WebP, 128x128)
    icon_bytes: &'static [u8],
}

/// Encode icon bytes as a base64 data URI (image/webp).
fn icon_data_uri(bytes: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:image/webp;base64,{}", b64)
}

const PROVIDER_METAS: &[ProviderMeta] = &[
    ProviderMeta {
        id: "anthropic",
        display_name: "Anthropic",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/anthropic.webp"),
    },
    ProviderMeta {
        id: "openai",
        display_name: "OpenAI",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/openai.webp"),
    },
    ProviderMeta {
        id: "deepseek",
        display_name: "DeepSeek",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/deepseek.webp"),
    },
    ProviderMeta {
        id: "qwen",
        display_name: "Qwen",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/qwen.webp"),
    },
    ProviderMeta {
        id: "gemini",
        display_name: "Google Gemini",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/gemini.webp"),
    },
    ProviderMeta {
        id: "groq",
        display_name: "Groq",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/groq.webp"),
    },
    ProviderMeta {
        id: "openrouter",
        display_name: "OpenRouter",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/openrouter.webp"),
    },
    ProviderMeta {
        id: "mistral",
        display_name: "Mistral",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/mistral.webp"),
    },
    ProviderMeta {
        id: "xai",
        display_name: "XAI (Grok)",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/xai.webp"),
    },
    ProviderMeta {
        id: "cohere",
        display_name: "Cohere",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/cohere.webp"),
    },
    ProviderMeta {
        id: "perplexity",
        display_name: "Perplexity",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/perplexity.webp"),
    },
    ProviderMeta {
        id: "together",
        display_name: "Together AI",
        provider_type: "service",
        icon_bytes: include_bytes!("../../../assets/icons/providers/together.webp"),
    },
    ProviderMeta {
        id: "ollama",
        display_name: "Ollama",
        provider_type: "local",
        icon_bytes: include_bytes!("../../../assets/icons/providers/ollama.webp"),
    },
    ProviderMeta {
        id: "claude-code",
        display_name: "Claude Code",
        provider_type: "cli",
        icon_bytes: include_bytes!("../../../assets/icons/providers/anthropic.webp"),
    },
    ProviderMeta {
        id: "gemini-cli",
        display_name: "Gemini CLI",
        provider_type: "cli",
        icon_bytes: include_bytes!("../../../assets/icons/providers/gemini.webp"),
    },
    ProviderMeta {
        id: "kimi-agent",
        display_name: "Kimi Agent",
        provider_type: "cli",
        icon_bytes: include_bytes!("../../../assets/icons/providers/kimi.webp"),
    },
    ProviderMeta {
        id: "openclaw",
        display_name: "OpenClaw",
        provider_type: "agent",
        icon_bytes: include_bytes!("../../../assets/icons/providers/openclaw.webp"),
    },
];

/// Get the ProviderConfig for a given provider id from the LlmConfig.
fn get_provider_config<'a>(
    llm: &'a crate::config::LlmConfig,
    provider_id: &str,
) -> Option<&'a crate::config::ProviderConfig> {
    match provider_id {
        "anthropic" => Some(&llm.anthropic),
        "openai" => Some(&llm.openai),
        "deepseek" => Some(&llm.deepseek),
        "qwen" => Some(&llm.qwen),
        "gemini" => Some(&llm.gemini),
        "groq" => Some(&llm.groq),
        "openrouter" => Some(&llm.openrouter),
        "mistral" => Some(&llm.mistral),
        "xai" => Some(&llm.xai),
        "cohere" => Some(&llm.cohere),
        "perplexity" => Some(&llm.perplexity),
        "together" => Some(&llm.together),
        "ollama" => Some(&llm.ollama),
        "claude-code" | "claude_code" => Some(&llm.claude_code),
        "gemini-cli" | "gemini_cli" => Some(&llm.gemini_cli),
        "kimi-agent" | "kimi_agent" | "kimi" => Some(&llm.kimi_agent),
        "openclaw" | "open_claw" | "open-claw" => Some(&llm.openclaw),
        _ => None,
    }
}

/// Check if any LLM provider is configured and usable.
/// Providers with API keys are always considered configured.
/// Keyless providers (ollama, claude-code, gemini-cli, kimi-agent) are
/// considered configured when they are the active provider.
fn has_any_configured_provider(llm: &crate::config::LlmConfig) -> bool {
    // Check if any provider has an explicit API key
    let has_key = PROVIDER_METAS.iter().any(|meta| {
        get_provider_config(llm, meta.id)
            .map(|pc| pc.api_key.is_some())
            .unwrap_or(false)
    });
    if has_key {
        return true;
    }

    // Keyless providers are usable simply by being selected as active
    const KEYLESS_PROVIDERS: &[&str] = &[
        "ollama",
        "claude-code",
        "claude_code",
        "gemini-cli",
        "gemini_cli",
        "kimi-agent",
        "kimi_agent",
        "kimi",
    ];
    if let Some(active) = llm.active_provider() {
        if KEYLESS_PROVIDERS.contains(&active) {
            return true;
        }
    }

    false
}

/// Get a mutable reference to the ProviderConfig for a given provider id.
fn get_provider_config_mut<'a>(
    llm: &'a mut crate::config::LlmConfig,
    provider_id: &str,
) -> Option<&'a mut crate::config::ProviderConfig> {
    match provider_id {
        "anthropic" => Some(&mut llm.anthropic),
        "openai" => Some(&mut llm.openai),
        "deepseek" => Some(&mut llm.deepseek),
        "qwen" => Some(&mut llm.qwen),
        "gemini" => Some(&mut llm.gemini),
        "groq" => Some(&mut llm.groq),
        "openrouter" => Some(&mut llm.openrouter),
        "mistral" => Some(&mut llm.mistral),
        "xai" => Some(&mut llm.xai),
        "cohere" => Some(&mut llm.cohere),
        "perplexity" => Some(&mut llm.perplexity),
        "together" => Some(&mut llm.together),
        "ollama" => Some(&mut llm.ollama),
        "claude-code" | "claude_code" => Some(&mut llm.claude_code),
        "gemini-cli" | "gemini_cli" => Some(&mut llm.gemini_cli),
        "kimi-agent" | "kimi_agent" | "kimi" => Some(&mut llm.kimi_agent),
        "openclaw" | "open_claw" | "open-claw" => Some(&mut llm.openclaw),
        _ => None,
    }
}

/// Handle config.llm.list command.
///
/// Returns all supported providers with their metadata and configuration status.
async fn handle_config_llm_list(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let config = AgentConfig::load().unwrap_or_default();
    let active = config.llm.active_provider().map(|s| s.to_string());

    let providers: Vec<serde_json::Value> = PROVIDER_METAS
        .iter()
        .map(|meta| {
            let provider_config = get_provider_config(&config.llm, meta.id);
            let configured = provider_config
                .map(|pc| pc.api_key.is_some())
                .unwrap_or(false);
            let is_active = active.as_deref() == Some(meta.id)
                || (meta.id == "claude-code" && active.as_deref() == Some("claude_code"))
                || (meta.id == "gemini-cli" && active.as_deref() == Some("gemini_cli"))
                || (meta.id == "kimi-agent"
                    && (active.as_deref() == Some("kimi_agent")
                        || active.as_deref() == Some("kimi")));
            let model = provider_config.and_then(|pc| pc.model.clone());

            let default_model = meta
                .id
                .parse::<nevoflux_llm::ProviderType>()
                .ok()
                .map(|pt| nevoflux_llm::default_model_for(pt).to_string());

            serde_json::json!({
                "id": meta.id,
                "display_name": meta.display_name,
                "type": meta.provider_type,
                "icon": icon_data_uri(meta.icon_bytes),
                "configured": configured,
                "active": is_active,
                "model": model,
                "default_model": default_model,
            })
        })
        .collect();

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "config.llm.list",
            "success": true,
            "data": {
                "providers": providers,
                "active_provider": active
            }
        }
    })
}

/// Handle config.llm.get command.
///
/// Returns configuration for a specific provider, with masked API key.
async fn handle_config_llm_get(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let provider_id = params
        .get("provider")
        .and_then(|p| p.as_str())
        .unwrap_or("");

    if provider_id.is_empty() {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.llm.get",
                "success": false,
                "error": {
                    "code": "MISSING_PARAM",
                    "message": "Missing provider parameter"
                }
            }
        });
    }

    let config = AgentConfig::load().unwrap_or_default();
    let provider_config = match get_provider_config(&config.llm, provider_id) {
        Some(pc) => pc,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.llm.get",
                    "success": false,
                    "error": {
                        "code": "UNKNOWN_PROVIDER",
                        "message": format!("Unknown provider: {}", provider_id)
                    }
                }
            });
        }
    };

    // Mask API key: show only last 4 chars
    let masked_key = provider_config.api_key.as_ref().map(|key| {
        if key.len() > 4 {
            format!("{}...{}", &key[..3], &key[key.len() - 4..])
        } else {
            "****".to_string()
        }
    });

    let default_model = provider_id
        .parse::<nevoflux_llm::ProviderType>()
        .ok()
        .map(|pt| nevoflux_llm::default_model_for(pt).to_string());

    let default_context_window = provider_id
        .parse::<nevoflux_llm::ProviderType>()
        .ok()
        .map(|pt| nevoflux_llm::default_context_window_for(pt));

    let is_active = config.llm.active_provider() == Some(provider_id)
        || (provider_id == "claude-code" && config.llm.active_provider() == Some("claude_code"))
        || (provider_id == "gemini-cli" && config.llm.active_provider() == Some("gemini_cli"))
        || (provider_id == "kimi-agent"
            && (config.llm.active_provider() == Some("kimi_agent")
                || config.llm.active_provider() == Some("kimi")));

    serde_json::json!({
        "type": "system_response",
        "payload": {
            "request_id": request_id,
            "command": "config.llm.get",
            "success": true,
            "data": {
                "provider": provider_id,
                "api_key": masked_key,
                "has_api_key": provider_config.api_key.is_some(),
                "model": provider_config.model,
                "base_url": provider_config.base_url,
                "context_window": provider_config.context_window,
                "default_model": default_model,
                "default_context_window": default_context_window,
                "active": is_active
            }
        }
    })
}

/// Handle config.llm.set command.
///
/// Updates configuration for a specific provider and optionally sets it as active.
async fn handle_config_llm_set(
    params: &serde_json::Value,
    shared_config: &SharedAgentConfig,
) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let provider_id = params
        .get("provider")
        .and_then(|p| p.as_str())
        .unwrap_or("");

    if provider_id.is_empty() {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.llm.set",
                "success": false,
                "error": {
                    "code": "MISSING_PARAM",
                    "message": "Missing provider parameter"
                }
            }
        });
    }

    let mut config = match AgentConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.llm.set",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": format!("Failed to load config: {}", e)
                    }
                }
            });
        }
    };

    // Get mutable reference to the provider config
    let provider_config = match get_provider_config_mut(&mut config.llm, provider_id) {
        Some(pc) => pc,
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.llm.set",
                    "success": false,
                    "error": {
                        "code": "UNKNOWN_PROVIDER",
                        "message": format!("Unknown provider: {}", provider_id)
                    }
                }
            });
        }
    };

    // Update API key if provided
    if let Some(api_key) = params.get("api_key").and_then(|k| k.as_str()) {
        if api_key.is_empty() {
            provider_config.api_key = None;
        } else {
            provider_config.api_key = Some(api_key.to_string());
        }
    }

    // Update model if provided
    if let Some(model) = params.get("model").and_then(|m| m.as_str()) {
        if model.is_empty() {
            provider_config.model = None;
        } else {
            provider_config.model = Some(model.to_string());
        }
    }

    // Update base_url if provided
    if let Some(base_url) = params.get("base_url").and_then(|u| u.as_str()) {
        if base_url.is_empty() {
            provider_config.base_url = None;
        } else {
            provider_config.base_url = Some(base_url.to_string());
        }
    }

    // Set as active provider if requested
    if params
        .get("set_active")
        .and_then(|s| s.as_bool())
        .unwrap_or(false)
    {
        config.llm.provider = Some(provider_id.to_string());
    }

    // Save config to disk and update runtime config
    match config.save() {
        Ok(()) => {
            let is_active = config.llm.provider.as_deref() == Some(provider_id);
            // Update the in-memory runtime config so changes take effect immediately
            *shared_config.write().unwrap() = Arc::new(config);
            info!(
                "config.llm.set: updated provider {} (active={}, runtime config updated)",
                provider_id, is_active
            );
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.llm.set",
                    "success": true,
                    "data": {
                        "provider": provider_id,
                        "active": is_active
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to save config: {}", e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.llm.set",
                    "success": false,
                    "error": {
                        "code": "SAVE_ERROR",
                        "message": format!("Failed to save config: {}", e)
                    }
                }
            })
        }
    }
}

/// Allowlist of config filenames that can be read/written via the config.file commands.
const CONFIG_FILE_ALLOWLIST: &[&str] =
    &["IDENTITY.md", "SOUL.md", "USER.md", "TOOLS.md", "AGENTS.md"];

// ============================================================================
// OpenClaw model configuration commands
// ============================================================================

/// Handle config.openclaw.model.list — list configured OpenClaw models/providers.
async fn handle_openclaw_model_list() -> serde_json::Value {
    use std::process::Command;

    if !crate::openclaw_setup::is_openclaw_installed() {
        return serde_json::json!({
            "command": "config.openclaw.model.list",
            "success": false,
            "error": "OpenClaw is not installed"
        });
    }

    // Read providers
    let providers = Command::new(crate::openclaw_setup::resolve_openclaw())
        .args(["config", "get", "models.providers"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            serde_json::from_str::<serde_json::Value>(&s).ok()
        })
        .unwrap_or(serde_json::json!({}));

    // Read primary model
    let primary = Command::new(crate::openclaw_setup::resolve_openclaw())
        .args(["config", "get", "agents.defaults.model.primary"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    serde_json::json!({
        "command": "config.openclaw.model.list",
        "success": true,
        "providers": providers,
        "primary_model": primary
    })
}

/// Handle config.openclaw.model.set — full auto-setup.
///
/// Single entry point: saves model config, writes auth profile, installs plugin,
/// configures permissions, starts/restarts gateway. User just fills the form and
/// clicks save — everything else is automatic.
async fn handle_openclaw_model_set(params: &serde_json::Value) -> serde_json::Value {
    use std::process::Command;

    if !crate::openclaw_setup::is_openclaw_installed() {
        return serde_json::json!({
            "command": "config.openclaw.model.set",
            "success": false,
            "error": "OpenClaw is not installed. Run: npm install -g openclaw@latest && openclaw onboard",
            "setup_step": "install"
        });
    }

    let provider_name = match params.get("provider_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "command": "config.openclaw.model.set",
                "success": false,
                "error": "Missing provider_name"
            });
        }
    };

    let base_url = params
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let api_key = params.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let api_type = params
        .get("api_type")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-completions");
    let model_id = params
        .get("model_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let model_name = params
        .get("model_name")
        .and_then(|v| v.as_str())
        .unwrap_or(model_id);
    let context_window = params
        .get("context_window")
        .and_then(|v| v.as_u64())
        .unwrap_or(200000);
    let max_tokens = params
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(32768);
    let reasoning = params
        .get("reasoning")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let set_as_primary = params
        .get("set_as_primary")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // --- Step 1: Write model provider config ---
    let provider_config = serde_json::json!({
        "baseUrl": base_url,
        "apiKey": api_key,
        "api": api_type,
        "models": [{
            "id": model_id,
            "name": model_name,
            "reasoning": reasoning,
            "input": ["text"],
            "contextWindow": context_window,
            "maxTokens": max_tokens
        }]
    });

    let config_path = format!("models.providers.{}", provider_name);
    let output = Command::new(crate::openclaw_setup::resolve_openclaw())
        .args([
            "config",
            "set",
            &config_path,
            &serde_json::to_string(&provider_config).unwrap(),
            "--strict-json",
        ])
        .output();

    if let Err(e) = output {
        return serde_json::json!({
            "command": "config.openclaw.model.set",
            "success": false,
            "error": format!("Failed to run openclaw config set: {}", e)
        });
    }
    let output = output.unwrap();
    if !output.status.success() {
        return serde_json::json!({
            "command": "config.openclaw.model.set",
            "success": false,
            "error": format!("openclaw config set failed: {}", String::from_utf8_lossy(&output.stderr))
        });
    }

    // --- Step 2: Write auth profile ---
    if let Err(e) = crate::openclaw_setup::write_auth_profile(provider_name, api_key) {
        tracing::warn!("Auth profile write failed: {}", e);
    }

    // --- Step 3: Set as primary model ---
    if set_as_primary {
        let primary = if model_id.contains('/') {
            model_id.to_string()
        } else {
            format!("{}/{}", provider_name, model_id)
        };
        let _ = Command::new(crate::openclaw_setup::resolve_openclaw())
            .args(["config", "set", "agents.defaults.model.primary", &primary])
            .output();

        let alias_path = format!("agents.defaults.models.{}", primary);
        let alias_value = serde_json::json!({"alias": provider_name});
        let _ = Command::new(crate::openclaw_setup::resolve_openclaw())
            .args([
                "config",
                "set",
                &alias_path,
                &serde_json::to_string(&alias_value).unwrap(),
                "--strict-json",
            ])
            .output();
    }

    // --- Step 4: Full auto-setup (plugin, permissions, gateway) ---
    let (needs_browser_restart, setup_message) = crate::openclaw_setup::full_auto_setup();

    info!(
        "OpenClaw model configured: provider={}, model={}, setup={}",
        provider_name, model_id, setup_message
    );

    serde_json::json!({
        "command": "config.openclaw.model.set",
        "success": true,
        "provider_name": provider_name,
        "model_id": model_id,
        "set_as_primary": set_as_primary,
        "needs_browser_restart": needs_browser_restart,
        "setup_message": setup_message
    })
}

/// Handle config.openclaw.model.delete — remove an OpenClaw model/provider.
async fn handle_openclaw_model_delete(params: &serde_json::Value) -> serde_json::Value {
    use std::process::Command;

    let provider_name = match params.get("provider_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "command": "config.openclaw.model.delete",
                "success": false,
                "error": "Missing provider_name"
            });
        }
    };

    let config_path = format!("models.providers.{}", provider_name);
    let output = Command::new(crate::openclaw_setup::resolve_openclaw())
        .args(["config", "unset", &config_path])
        .output();

    match output {
        Ok(o) if o.status.success() => serde_json::json!({
            "command": "config.openclaw.model.delete",
            "success": true,
            "provider_name": provider_name
        }),
        Ok(o) => serde_json::json!({
            "command": "config.openclaw.model.delete",
            "success": false,
            "error": String::from_utf8_lossy(&o.stderr).to_string()
        }),
        Err(e) => serde_json::json!({
            "command": "config.openclaw.model.delete",
            "success": false,
            "error": format!("Failed to run openclaw: {}", e)
        }),
    }
}

/// Handle config.openclaw.status — detailed diagnostics for sidebar display.
async fn handle_openclaw_status() -> serde_json::Value {
    use std::process::Command;

    let installed = crate::openclaw_setup::is_openclaw_installed();

    if !installed {
        return serde_json::json!({
            "command": "config.openclaw.status",
            "success": true,
            "installed": false,
            "setup_step": "install",
            "message": "OpenClaw is not installed. Run: npm install -g openclaw@latest && openclaw onboard"
        });
    }

    let version = Command::new(crate::openclaw_setup::resolve_openclaw())
        .args(["--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let gateway_running = crate::openclaw_setup::is_gateway_running();
    let plugin_installed = crate::openclaw_setup::is_plugin_installed();

    // Determine current setup step
    let (setup_step, message) = if !gateway_running && !plugin_installed {
        (
            "setup",
            "OpenClaw needs initial setup. Save a model configuration to auto-configure.",
        )
    } else if !gateway_running {
        ("gateway", "OpenClaw gateway is not running.")
    } else if !plugin_installed {
        (
            "plugin",
            "NevoFlux tools plugin not installed. Save a model configuration to auto-configure.",
        )
    } else {
        ("ready", "OpenClaw is ready.")
    };

    serde_json::json!({
        "command": "config.openclaw.status",
        "success": true,
        "installed": true,
        "version": version,
        "gateway_running": gateway_running,
        "plugin_installed": plugin_installed,
        "setup_step": setup_step,
        "message": message
    })
}

/// Handle config.file.read command.
///
/// Reads a config file from the nevoflux config directory.
/// Only files in the allowlist can be read.
async fn handle_config_file_read(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let filename = params
        .get("filename")
        .and_then(|f| f.as_str())
        .unwrap_or("");

    if filename.is_empty() || !CONFIG_FILE_ALLOWLIST.contains(&filename) {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.file.read",
                "success": false,
                "error": {
                    "code": "INVALID_FILENAME",
                    "message": format!("Invalid filename: '{}'. Allowed: {:?}", filename, CONFIG_FILE_ALLOWLIST)
                }
            }
        });
    }

    let config_dir = match dirs::config_dir() {
        Some(dir) => dir.join("nevoflux"),
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.file.read",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": "Could not determine config directory"
                    }
                }
            });
        }
    };

    let file_path = config_dir.join(filename);

    if tokio::fs::metadata(&file_path).await.is_ok() {
        match tokio::fs::read_to_string(&file_path).await {
            Ok(content) => {
                serde_json::json!({
                    "type": "system_response",
                    "payload": {
                        "request_id": request_id,
                        "command": "config.file.read",
                        "success": true,
                        "data": {
                            "filename": filename,
                            "content": content,
                            "exists": true
                        }
                    }
                })
            }
            Err(e) => {
                serde_json::json!({
                    "type": "system_response",
                    "payload": {
                        "request_id": request_id,
                        "command": "config.file.read",
                        "success": false,
                        "error": {
                            "code": "READ_ERROR",
                            "message": format!("Failed to read file: {}", e)
                        }
                    }
                })
            }
        }
    } else {
        serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.file.read",
                "success": true,
                "data": {
                    "filename": filename,
                    "content": "",
                    "exists": false
                }
            }
        })
    }
}

/// Handle config.file.write command.
///
/// Writes content to a config file in the nevoflux config directory.
/// Only files in the allowlist can be written.
async fn handle_config_file_write(params: &serde_json::Value) -> serde_json::Value {
    let request_id = params
        .get("request_id")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    let filename = params
        .get("filename")
        .and_then(|f| f.as_str())
        .unwrap_or("");

    let content = params.get("content").and_then(|c| c.as_str()).unwrap_or("");

    if filename.is_empty() || !CONFIG_FILE_ALLOWLIST.contains(&filename) {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.file.write",
                "success": false,
                "error": {
                    "code": "INVALID_FILENAME",
                    "message": format!("Invalid filename: '{}'. Allowed: {:?}", filename, CONFIG_FILE_ALLOWLIST)
                }
            }
        });
    }

    let config_dir = match dirs::config_dir() {
        Some(dir) => dir.join("nevoflux"),
        None => {
            return serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.file.write",
                    "success": false,
                    "error": {
                        "code": "CONFIG_ERROR",
                        "message": "Could not determine config directory"
                    }
                }
            });
        }
    };

    // Ensure config directory exists
    if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
        return serde_json::json!({
            "type": "system_response",
            "payload": {
                "request_id": request_id,
                "command": "config.file.write",
                "success": false,
                "error": {
                    "code": "DIR_ERROR",
                    "message": format!("Failed to create config directory: {}", e)
                }
            }
        });
    }

    let file_path = config_dir.join(filename);
    let bytes_written = content.len();

    match tokio::fs::write(&file_path, content).await {
        Ok(()) => {
            info!(
                "config.file.write: wrote {} bytes to {}",
                bytes_written, filename
            );
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.file.write",
                    "success": true,
                    "data": {
                        "filename": filename,
                        "bytes_written": bytes_written
                    }
                }
            })
        }
        Err(e) => {
            error!("Failed to write config file {}: {}", filename, e);
            serde_json::json!({
                "type": "system_response",
                "payload": {
                    "request_id": request_id,
                    "command": "config.file.write",
                    "success": false,
                    "error": {
                        "code": "WRITE_ERROR",
                        "message": format!("Failed to write file: {}", e)
                    }
                }
            })
        }
    }
}

// ============================================
// Tool Availability Helpers
// ============================================

/// Built-in tools that are always available.
/// These correspond to the core tools provided by the agent runtime.
const BUILTIN_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Bash",
    "Glob",
    "Grep",
    "browser_navigate",
    "browser_click",
    "browser_type",
    "browser_screenshot",
    "browser_scroll",
    "browser_get_content",
    "computer_screenshot",
    "computer_click",
    "computer_type",
    "computer_key",
    "computer_scroll",
];

/// Gather all available tools from MCP servers and built-in tools.
///
/// Returns a list of tool names in the format:
/// - Built-in tools: just the tool name (e.g., "Read", "Write")
/// - MCP tools: "server_name:tool_name" format (e.g., "notion:search")
async fn gather_available_tools(services: &HostServices) -> Vec<String> {
    let mut tools = Vec::new();

    // Add built-in tools
    tools.extend(BUILTIN_TOOLS.iter().map(|s| s.to_string()));

    // Add MCP tools from connected servers
    if let Some(ref mcp_manager) = services.mcp_manager {
        if let Ok(mcp_tools) = mcp_manager.list_all_tools().await {
            for server_tool in mcp_tools {
                // Format: "server_name:tool_name"
                tools.push(format!(
                    "{}:{}",
                    server_tool.server_name, server_tool.tool.name
                ));
            }
        }
    }

    tools
}

/// Build attachment metadata from attachments and local files for persisting in message history.
///
/// Read image-typed `local_files` from disk and ALSO add them as real
/// `Attachment` entries so the LLM can SEE the picture (vision modality)
/// instead of being told to use the `read` tool. The `read` tool calls
/// `fs::read_to_string` which fails on binary PNG/JPEG with a UTF-8
/// error, leaving the agent confused (observed bug:
/// /tmp/nevoflux-debug.log shows round 2 producing 0 text + 0 tool
/// calls after `read` returned an 83-byte error message).
///
/// IMPORTANT — `local_files` entries are KEPT after promotion. The
/// agent needs the path string to call
/// `canvas_attach_asset({ local_path: ... })`, which is the proper
/// way to put bytes into a composition's files map for rendering.
/// Without the path, the agent can't bridge the gap between
/// "vision-only LLM input" and "binary asset for the renderer" — it
/// resorts to globbing and AskUser (observed in
/// /tmp/nevoflux-sidebar.log).
///
/// Promotion rules:
/// - Only image MIME types (`image/*`) are promoted. Documents / archives
///   stay as `local_files` so the agent can still drive `read` on text
///   formats it knows how to handle.
/// - **Downscale to LLM-friendly size before encoding.** A 6.45 MB PNG
///   becomes a 9 MB base64 string in the LLM payload, which most proxies
///   (and even the direct Anthropic 5 MB-per-image cap) reject. Modern
///   vision models work fine on ~1024 px images; we resize via
///   `canvas_video::asset_resize::maybe_resize_bytes` with stage=1024×1024
///   so opaque PNGs become small JPEG q=85 and transparent PNGs stay PNG
///   at the smaller dimensions.
/// - Failure-safe: if `fs::read` or resize fails, the entry stays in
///   `local_files` so the metadata is still surfaced to the agent and
///   other code paths (history display, etc.) keep working.
/// - Cap: skip files > 20 MB advertised; refuse promotion if even the
///   resized output exceeds 5 MB raw (the Anthropic-direct cap and a
///   reasonable upper bound for any proxy).
fn promote_image_local_files_to_attachments(
    attachments: &mut Vec<Attachment>,
    local_files: &mut Vec<nevoflux_protocol::FileInfo>,
) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use crate::canvas_video::asset_resize::{maybe_resize_bytes, ResizeOutcome};

    const MAX_INPUT_BYTES: u64 = 20 * 1024 * 1024;
    const LLM_STAGE_MAX: u32 = 1024;
    // Hard ceiling on the post-resize payload sent to the LLM. Anthropic
    // direct caps individual images at 5 MB; third-party proxies often
    // smaller. After resizing to 1024 px JPEG q=85 we should be well
    // under this — guard rejects pathological cases.
    const MAX_LLM_BYTES: usize = 5 * 1024 * 1024;

    let mut promoted_paths: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for f in local_files.iter() {
        if f.is_directory {
            continue;
        }
        let original_mime = guess_mime_type(&f.path);
        if !original_mime.starts_with("image/") {
            continue;
        }
        if f.size.map(|s| s > MAX_INPUT_BYTES).unwrap_or(false) {
            tracing::warn!(
                path = %f.path,
                size = ?f.size,
                "image local_file too large to promote to attachment; leaving as path-only reference"
            );
            continue;
        }

        let raw_bytes = match std::fs::read(&f.path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    path = %f.path,
                    error = %e,
                    "failed to read image local_file for attachment promotion"
                );
                continue;
            }
        };
        let original_bytes = raw_bytes.len();
        if original_bytes as u64 > MAX_INPUT_BYTES {
            tracing::warn!(
                path = %f.path,
                bytes = original_bytes,
                "image local_file exceeded MAX_INPUT_BYTES post-read; skipping promotion"
            );
            continue;
        }

        // Resize to LLM-friendly dimensions. The 1024×1024 box is what
        // Claude vision tools internally normalise to anyway; sending
        // anything bigger spends bandwidth without improving recognition.
        let (resized_bytes, outcome) =
            maybe_resize_bytes(&raw_bytes, LLM_STAGE_MAX, LLM_STAGE_MAX);
        let (final_bytes, final_mime): (Vec<u8>, String) = match &outcome {
            ResizeOutcome::Resized { format, .. } => {
                let new_mime = match format {
                    image::ImageFormat::Jpeg => "image/jpeg",
                    image::ImageFormat::Png => "image/png",
                    image::ImageFormat::Gif => "image/gif",
                    _ => "application/octet-stream",
                };
                (resized_bytes, new_mime.to_string())
            }
            // No resize / not-an-image / failure → fall back to original
            // bytes with the original mime. We still promote (don't drop
            // tiny images just because they didn't get resized).
            _ => (raw_bytes, original_mime.to_string()),
        };

        if final_bytes.len() > MAX_LLM_BYTES {
            tracing::warn!(
                path = %f.path,
                final_bytes = final_bytes.len(),
                "post-resize image still > {} MB; skipping promotion (LLM proxy will likely reject)",
                MAX_LLM_BYTES / (1024 * 1024)
            );
            continue;
        }

        let name = std::path::Path::new(&f.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| f.path.clone());
        let data = STANDARD.encode(&final_bytes);
        attachments.push(Attachment {
            name,
            mime_type: final_mime.clone(),
            data,
        });
        promoted_paths.insert(f.path.clone());
        tracing::info!(
            path = %f.path,
            mime = %final_mime,
            original_bytes = original_bytes,
            llm_bytes = final_bytes.len(),
            outcome = ?outcome,
            "promoted image local_file to attachment (with resize); local_files entry preserved so agent can call canvas_attach_asset(local_path=...)"
        );
    }

    // Intentionally NOT removing promoted entries from `local_files`.
    // The agent needs the path string later to call
    // `canvas_attach_asset({ local_path: ... })` — that's how bytes get
    // moved from disk into the composition's files map for the
    // renderer. The duplication (vision attachment + path entry) is
    // not wasteful: the agent uses each for a different purpose.
    let _ = promoted_paths;
}

/// Stores only name, mime_type, and path — no base64 data — to keep the database small.
fn build_attachment_metadata(
    attachments: &[Attachment],
    local_files: &[nevoflux_protocol::FileInfo],
) -> Option<HashMap<String, serde_json::Value>> {
    let mut attachment_meta: Vec<serde_json::Value> = Vec::new();

    for att in attachments {
        attachment_meta.push(serde_json::json!({
            "name": att.name,
            "mime_type": att.mime_type,
        }));
    }

    for f in local_files {
        let name = std::path::Path::new(&f.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| f.path.clone());
        let mime_type = if f.is_directory {
            "inode/directory"
        } else {
            guess_mime_type(&f.path)
        };
        attachment_meta.push(serde_json::json!({
            "name": name,
            "mime_type": mime_type,
            "path": f.path,
        }));
    }

    if attachment_meta.is_empty() {
        None
    } else {
        let mut metadata = HashMap::new();
        metadata.insert(
            "attachments".to_string(),
            serde_json::json!(attachment_meta),
        );
        Some(metadata)
    }
}

/// Simple MIME type guessing from file extension.
fn guess_mime_type(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        "md" => "text/markdown",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_protocol::PlanStep;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.port_start, 19500);
        assert_eq!(config.port_end, 19600);
        assert_eq!(config.bind_address, "127.0.0.1");
    }

    #[test]
    fn promote_image_local_files_reads_bytes_and_keeps_entry() {
        // Write a real PNG to a tempfile and verify the helper reads it,
        // base64-encodes it into attachments, and drops the local_files
        // entry so the agent doesn't double-process.
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hero.png");
        // Minimal valid PNG header — magic byte sniffer in the inliner
        // recognizes it; that's all we need for this unit.
        let bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        std::fs::write(&path, &bytes).unwrap();

        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: path.to_string_lossy().to_string(),
            is_directory: false,
            size: Some(bytes.len() as u64),
            modified: None,
        }];

        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);

        assert_eq!(attachments.len(), 1, "image should have been promoted");
        assert_eq!(attachments[0].mime_type, "image/png");
        assert_eq!(attachments[0].name, "hero.png");
        assert!(!attachments[0].data.is_empty(), "data must be base64-encoded");
        // Decode and compare round-trip.
        use base64::{engine::general_purpose::STANDARD, Engine};
        let round_trip = STANDARD.decode(&attachments[0].data).unwrap();
        assert_eq!(round_trip, bytes);
        // local_files entry MUST be preserved so the agent can call
        // canvas_attach_asset({ local_path: ... }) afterwards. Earlier
        // versions dropped the entry and the agent ended up globbing
        // /tmp blindly looking for the path it could no longer see.
        assert_eq!(local_files.len(), 1, "promoted entry must STAY in local_files for canvas_attach_asset(local_path=...)");
        assert_eq!(local_files[0].path, path.to_string_lossy().to_string());

        // Reuse: writing a tmpfile guard
        let _ = &dir;
    }

    #[test]
    fn promote_skips_non_image_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.txt");
        std::fs::write(&path, b"hello").unwrap();
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: path.to_string_lossy().to_string(),
            is_directory: false,
            size: Some(5),
            modified: None,
        }];
        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);
        assert_eq!(attachments.len(), 0, "non-image must NOT be promoted");
        assert_eq!(local_files.len(), 1, "non-image stays in local_files");
        let _ = &dir;
    }

    #[test]
    fn promote_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        // Direct path to the dir itself, marked as directory.
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: dir.path().to_string_lossy().to_string(),
            is_directory: true,
            size: None,
            modified: None,
        }];
        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);
        assert_eq!(attachments.len(), 0, "directories never get promoted");
        assert_eq!(local_files.len(), 1, "directory stays in local_files");
        let _ = &dir;
    }

    #[test]
    fn promote_handles_missing_file_gracefully() {
        // Path that doesn't exist — must not panic, must not add to
        // attachments, must keep the entry in local_files (so the agent
        // can at least see the metadata).
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: "/this/path/does/not/exist/hero.png".to_string(),
            is_directory: false,
            size: Some(1024),
            modified: None,
        }];
        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);
        assert_eq!(attachments.len(), 0);
        assert_eq!(local_files.len(), 1, "missing file kept as path reference");
    }

    #[test]
    fn promote_skips_oversized_image_by_metadata() {
        // size > 20 MB → skip without even trying to read.
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: "/tmp/huge.png".to_string(),
            is_directory: false,
            size: Some(100 * 1024 * 1024), // 100 MB advertised
            modified: None,
        }];
        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);
        assert_eq!(attachments.len(), 0);
        assert_eq!(local_files.len(), 1);
    }

    #[test]
    fn promote_resizes_oversized_image_before_llm_payload() {
        // Reproduces the exact failing scenario from the user's log:
        // a 2816×1536 high-entropy PNG (logged as 6.45 MB raw, 9 MB
        // base64). After my fix, promotion must resize it down so the
        // LLM payload is well under the 5 MB cap.
        use base64::{engine::general_purpose::STANDARD, Engine};
        use image::{codecs::png::PngEncoder, ImageEncoder};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hero.png");

        // High-entropy pixels — PNG predictors can't compress, simulating
        // a real photo encoded as PNG.
        let (w, h) = (2816u32, 1536u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = x.wrapping_mul(2654435761).wrapping_add(y.wrapping_mul(40503)) as u8;
                let g = y.wrapping_mul(2246822519).wrapping_add(x.wrapping_mul(16807)) as u8;
                let b = (x ^ y).wrapping_mul(1597334677) as u8;
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        let mut png_bytes = Vec::new();
        let encoder = PngEncoder::new(&mut png_bytes);
        encoder.write_image(&rgb, w, h, image::ColorType::Rgb8.into()).unwrap();
        std::fs::write(&path, &png_bytes).unwrap();

        let original_size = png_bytes.len();
        // Sanity: the test fixture really exceeds the LLM-payload cap
        // when sent raw. Otherwise the test wouldn't be exercising the
        // resize path.
        assert!(original_size > 5 * 1024 * 1024,
            "fixture only {} bytes — expected > 5 MB to trigger LLM size guard", original_size);

        let mut attachments: Vec<Attachment> = Vec::new();
        let mut local_files = vec![nevoflux_protocol::FileInfo {
            path: path.to_string_lossy().to_string(),
            is_directory: false,
            size: Some(original_size as u64),
            modified: None,
        }];

        promote_image_local_files_to_attachments(&mut attachments, &mut local_files);

        assert_eq!(attachments.len(), 1, "image must be promoted (with resize)");
        assert_eq!(local_files.len(), 1, "promoted entry preserved for canvas_attach_asset(local_path)");

        // Decode the output and verify it's much smaller AND inside the
        // LLM cap. Opaque photo PNG → JPEG q=85 path.
        let llm_bytes = STANDARD.decode(&attachments[0].data).unwrap();
        assert!(llm_bytes.len() < 1 * 1024 * 1024,
            "LLM payload {} bytes; should be < 1 MB after resize", llm_bytes.len());
        assert_eq!(attachments[0].mime_type, "image/jpeg",
            "opaque PNG should convert to JPEG");
        let _ = &dir;
    }

    #[test]
    fn test_parse_agent_mode() {
        assert!(matches!(parse_agent_mode("browser"), AgentMode::Browser));
        assert!(matches!(parse_agent_mode("agent"), AgentMode::Agent));
        // §3.2: "code" deprecated, maps to Agent
        #[allow(deprecated)]
        {
            assert!(matches!(parse_agent_mode("code"), AgentMode::Agent));
        }
        // Unknown defaults to Chat
        assert!(matches!(parse_agent_mode("chat"), AgentMode::Chat));
        assert!(matches!(parse_agent_mode("unknown"), AgentMode::Chat));
        assert!(matches!(parse_agent_mode(""), AgentMode::Chat));
    }

    #[tokio::test]
    async fn test_find_available_port() {
        let config = ServerConfig::default();
        let port = find_available_port(&config).await;
        assert!(port.is_ok());
        let port = port.unwrap();
        assert!(port >= 19500 && port <= 19600);
    }

    #[test]
    fn test_format_plan_as_context() {
        let proposal = PlanProposal {
            summary: "Deploy the application".to_string(),
            steps: vec![
                PlanStep {
                    description: "Build the project".to_string(),
                    model: None,
                },
                PlanStep {
                    description: "Run tests".to_string(),
                    model: Some("gpt-4o".to_string()),
                },
                PlanStep {
                    description: "Deploy to production".to_string(),
                    model: None,
                },
            ],
        };

        let text = format_plan_as_context(&proposal);
        assert!(text.contains("Approved plan: Deploy the application"));
        assert!(text.contains("1. Build the project"));
        assert!(text.contains("2. Run tests [model: gpt-4o]"));
        assert!(text.contains("3. Deploy to production"));
        assert!(text.contains("Execute this plan now."));
    }

    #[test]
    fn test_format_plan_as_context_empty_steps() {
        let proposal = PlanProposal {
            summary: "Empty plan".to_string(),
            steps: vec![],
        };

        let text = format_plan_as_context(&proposal);
        assert!(text.contains("Approved plan: Empty plan"));
        assert!(text.contains("Execute this plan now."));
    }

    #[tokio::test]
    async fn test_server_start_and_shutdown() {
        let config = ServerConfig::default();
        let router = Arc::new(Router::new());
        let session_manager = Arc::new(SessionManager::in_memory().unwrap());

        let server = start_server(config, router, session_manager).await;
        assert!(server.is_ok());

        let mut server = server.unwrap();
        assert!(server.port() >= 19500);

        // Shutdown
        server.shutdown().await;
    }

    // -----------------------------------------------------------------------
    // E2E: hot knowledge → system prompt injection
    // -----------------------------------------------------------------------

    /// Verify that promoted hot knowledge entries are rendered into the
    /// correct markdown format by `build_hot_knowledge_section()`.
    ///
    /// Tests all three categories (site_interaction, tool_optimization,
    /// user_preference) and verifies non-hot entries are excluded.
    #[test]
    fn e2e_hot_knowledge_section_rendering() {
        use nevoflux_storage::{CreateKnowledgeParams, Storage};

        let storage = Storage::open_in_memory().unwrap();

        // Insert hot entries for all 3 categories
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("github.com".into()),
                summary: "Use data-testid for selectors on GitHub".into(),
                details: "GitHub uses data-testid attributes extensively".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                domain: None,
                summary: "click_element times out on SPAs".into(),
                details: "Single-page apps need wait_for_navigation after click".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "user_preference".into(),
                domain: None,
                summary: "User prefers concise responses".into(),
                details: "Keep replies under 3 sentences when possible".into(),
                ..Default::default()
            })
            .unwrap();

        // Also insert a non-hot entry (should NOT appear)
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("hidden.com".into()),
                summary: "This should not appear in hot section".into(),
                details: "Not promoted".into(),
                ..Default::default()
            })
            .unwrap();

        // Mark the first 3 entries as hot via SQL (simulating promotion)
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET status = 'promoted', hot = 1, \
                     hot_summary = '[github.com] Use data-testid for selectors' \
                     WHERE summary LIKE '%data-testid%'",
                    [],
                )?;
                conn.execute(
                    "UPDATE knowledge SET status = 'promoted', hot = 1, \
                     hot_summary = 'click_element needs wait_for_navigation on SPAs' \
                     WHERE summary LIKE '%click_element%'",
                    [],
                )?;
                conn.execute(
                    "UPDATE knowledge SET status = 'promoted', hot = 1, \
                     hot_summary = 'User prefers concise responses' \
                     WHERE summary LIKE '%concise responses%'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        // Call build_hot_knowledge_section
        let section = build_hot_knowledge_section(storage.database())
            .expect("Should produce a section when hot entries exist");

        // Verify section header
        assert!(
            section.contains("## Learned Knowledge / 已学习的知识"),
            "Should have main header. Got:\n{}",
            section
        );

        // Verify all 3 category subsections
        assert!(
            section.contains("### Site Interactions / 网站交互"),
            "Should have Site Interactions section"
        );
        assert!(
            section.contains("### Tool Optimizations / 工具优化"),
            "Should have Tool Optimizations section"
        );
        assert!(
            section.contains("### User Preferences / 用户偏好"),
            "Should have User Preferences section"
        );

        // Verify hot_summary content appears
        assert!(
            section.contains("[github.com] Use data-testid for selectors"),
            "Should contain site interaction hot_summary"
        );
        assert!(
            section.contains("click_element needs wait_for_navigation on SPAs"),
            "Should contain tool optimization hot_summary"
        );
        assert!(
            section.contains("User prefers concise responses"),
            "Should contain user preference hot_summary"
        );

        // Verify non-hot entry does NOT appear
        assert!(
            !section.contains("hidden.com"),
            "Non-hot entries must not appear in the section"
        );
        assert!(
            !section.contains("This should not appear"),
            "Non-hot entry summary must not appear"
        );
    }

    /// Verify that `build_hot_knowledge_section()` returns `None` when
    /// there are no hot entries.
    #[test]
    fn e2e_hot_knowledge_section_empty_when_no_hot() {
        use nevoflux_storage::{CreateKnowledgeParams, Storage};

        let storage = Storage::open_in_memory().unwrap();

        // Insert a non-hot entry
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "Some knowledge".into(),
                details: "Details".into(),
                ..Default::default()
            })
            .unwrap();

        let section = build_hot_knowledge_section(storage.database());
        assert!(
            section.is_none(),
            "Should return None when no hot entries exist"
        );
    }

    #[test]
    fn test_freshness_warning_old_entry() {
        use nevoflux_storage::{CreateKnowledgeParams, Storage};

        let storage = Storage::open_in_memory().unwrap();

        let entry = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "user_preference".to_string(),
                summary: "Old preference".to_string(),
                details: "Old details".to_string(),
                ..Default::default()
            })
            .unwrap();
        storage
            .knowledge()
            .update_status(&entry.id, "validated")
            .unwrap();
        storage
            .knowledge()
            .mark_hot(&entry.id, "Old preference")
            .unwrap();

        // Set updated_at to 5 days ago
        storage
            .database()
            .with_connection(|conn| {
                let five_days_ago = (chrono::Utc::now() - chrono::Duration::days(5)).to_rfc3339();
                conn.execute(
                    "UPDATE knowledge SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![five_days_ago, entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let section = build_hot_knowledge_section(storage.database()).unwrap();
        assert!(
            section.contains("old, verify before acting]"),
            "Expected freshness warning, got: {}",
            section
        );
    }

    #[test]
    fn test_freshness_no_warning_recent() {
        use nevoflux_storage::{CreateKnowledgeParams, Storage};

        let storage = Storage::open_in_memory().unwrap();

        let entry = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "user_preference".to_string(),
                summary: "Fresh preference".to_string(),
                details: "Fresh details".to_string(),
                ..Default::default()
            })
            .unwrap();
        storage
            .knowledge()
            .update_status(&entry.id, "validated")
            .unwrap();
        storage
            .knowledge()
            .mark_hot(&entry.id, "Fresh preference")
            .unwrap();

        let section = build_hot_knowledge_section(storage.database()).unwrap();
        assert!(
            !section.contains("verify before acting"),
            "Should not have freshness warning, got: {}",
            section
        );
    }
}
