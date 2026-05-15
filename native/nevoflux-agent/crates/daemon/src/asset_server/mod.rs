// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Asset & Stream Plane HTTP server.
//!
//! Companion to native messaging: this localhost-only server handles
//! bulk byte streams (uploads, downloads, asset GET, render frame upload,
//! OAuth callbacks). See
//! `docs/superpowers/specs/2026-04-29-asset-stream-plane-design.md` for
//! the full design and §1-§4 for the boundary rules.
//!
//! Phase 1 (Step A) lights:
//! - `GET /file/:token` (legacy, bearer-less, `download_tokens`)
//! - `GET /v1/health`
//! - `GET /v1/capabilities`
//! - `POST /v1/upload/screenshot/:request_id` (writes to upload_inbox)
//! - all Phase 2-5 routes return `501 Not Implemented` with
//!   `X-NF-Future-Phase: phase-N`

pub mod auth;
pub mod handlers;
pub mod inbox;
pub mod oauth;
mod port;
pub mod state;
pub mod token_store;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use thiserror::Error;
use tokio::sync::oneshot;

pub use inbox::InboxError;
pub use state::{AssetPlaneInfo, AssetServerState};

/// Wire format version for `bridge:hello.asset_plane.version`.
pub const ASSET_SERVER_VERSION: u32 = 1;

/// Default cap for POST request bodies.  64 MiB lets a 4K PNG screenshot
/// (~20 MiB worst case) and an MP4 frame batch through comfortably.
pub const DEFAULT_MAX_BODY_SIZE: usize = 64 * 1024 * 1024;

/// `download_tokens` TTL — single-use, browser_upload `/file/:token`.
pub const DOWNLOAD_TOKEN_TTL: Duration = Duration::from_secs(60);
/// `composition_tokens` TTL — multi-use, `/v1/asset/composition/...`.
pub const COMPOSITION_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);
/// `blob_tokens` TTL — single-or-multi, `/v1/blob/<id>`.
#[allow(dead_code)] // lit in Phase 5
pub const BLOB_TOKEN_TTL: Duration = Duration::from_secs(60 * 60);
/// Tool-side wait deadline for the screenshot inbox.
pub const SCREENSHOT_INBOX_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum BindError {
    #[error("no free port found in range {start}..{end}")]
    NoFreePortInRange { start: u16, end: u16 },
    #[error("axum serve failed: {0}")]
    Serve(String),
}

/// Boot-time configuration for the AssetServer.
#[derive(Clone)]
pub struct AssetServerConfig {
    /// Half-open port range `[start, end)`. Must match the bridge's range
    /// so co-existing daemons each get distinct AssetServer ports.
    pub port_range: std::ops::Range<u16>,
    /// Daemon-wide bearer; rotates on daemon restart.
    pub bearer_token: String,
    /// Increments on token rotation. Extension URL caches use this to
    /// invalidate stale URLs.
    pub session_id: String,
    /// Max body size for /v1/* POST requests.
    pub max_body_size: usize,
    /// CORS allow-origin (the moz-extension://... URL). May be `None` at
    /// boot — populated when the extension first connects.
    pub allowed_origin: Option<String>,
    /// Artifact storage backend — required by Phase 2 composition / asset
    /// GET handlers. Daemon wires this from `HostServices::database`;
    /// unit-test boots leave it `None` and the corresponding routes
    /// return 503.
    pub storage: Option<Arc<nevoflux_storage::Database>>,
    /// Canvas video service — used by Phase 3 asset upload handler to
    /// reuse `attach_asset` (resize + magic-byte sniff + dual-write
    /// `files`/`content` invariant). `None` in unit-test boots that
    /// don't exercise the asset upload route.
    pub canvas_video_service: Option<Arc<crate::canvas_video::CanvasVideoService>>,
}

impl std::fmt::Debug for AssetServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetServerConfig")
            .field("port_range", &self.port_range)
            .field("max_body_size", &self.max_body_size)
            .field("allowed_origin", &self.allowed_origin)
            .field("storage", &self.storage.as_ref().map(|_| "Some(Database)"))
            .field(
                "canvas_video_service",
                &self.canvas_video_service.as_ref().map(|_| "Some(...)"),
            )
            .finish_non_exhaustive()
    }
}

impl Default for AssetServerConfig {
    fn default() -> Self {
        Self {
            // Default range matches `ServerConfig` in `server.rs`. The
            // bridge has already taken its slot by the time AssetServer
            // starts, so AssetServer will pick the next free.
            port_range: 19500..19601,
            bearer_token: token_store::random_token(),
            session_id: token_store::random_token(),
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            allowed_origin: None,
            storage: None,
            canvas_video_service: None,
        }
    }
}

/// Running AssetServer. Cheaply cloneable handle that exposes the bound
/// port, bearer token, and the typed API for feature modules to call
/// (`register_download`, `await_screenshot`, etc.).
#[derive(Clone)]
pub struct AssetServer {
    state: Arc<AssetServerState>,
    bound_port: u16,
    /// Held in an `Arc` so that cloning the handle does not steal the
    /// shutdown sender — only the original holder can shut the server
    /// down (typically: nobody, since the server lives until process
    /// exit).
    _shutdown_tx: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
}

impl std::fmt::Debug for AssetServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetServer")
            .field("bound_port", &self.bound_port)
            .finish()
    }
}

impl AssetServer {
    /// Start the server. Returns once the listener is bound and the
    /// background serve task is spawned. Eviction loops for all three
    /// TokenStores and the upload inbox are also spawned.
    pub async fn start(config: AssetServerConfig) -> Result<Self, BindError> {
        let listener = port::bind_in_range(&config.port_range).await?;
        let bound_port = listener
            .local_addr()
            .map_err(|e| BindError::Serve(format!("local_addr: {e}")))?
            .port();

        let state = Arc::new(AssetServerState::new(&config));
        state.set_bound_port(bound_port);

        token_store::spawn_eviction_loop(state.download_tokens.clone(), Duration::from_secs(30));
        token_store::spawn_eviction_loop(state.composition_tokens.clone(), Duration::from_secs(60));
        token_store::spawn_eviction_loop(state.blob_tokens.clone(), Duration::from_secs(60));
        inbox::spawn_inbox_eviction_loop(state.upload_inbox.clone(), Duration::from_secs(30));
        oauth::spawn_oauth_eviction_loop(state.oauth_registry.clone(), Duration::from_secs(60));

        let app = build_router(state.clone(), config.max_body_size);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = serve.await {
                tracing::error!("asset_server: serve error: {e}");
            }
        });

        tracing::info!(
            port = bound_port,
            "asset_server: started on 127.0.0.1:{bound_port}"
        );

        Ok(Self {
            state,
            bound_port,
            _shutdown_tx: Arc::new(std::sync::Mutex::new(Some(shutdown_tx))),
        })
    }

    pub fn bound_port(&self) -> u16 {
        self.bound_port
    }

    pub fn bearer_token(&self) -> &str {
        &self.state.bearer_token
    }

    pub fn session_id(&self) -> &str {
        &self.state.session_id
    }

    pub fn state(&self) -> &Arc<AssetServerState> {
        &self.state
    }

    /// Build the `asset_plane` info struct delivered to the extension via
    /// the `system_command status` SystemResponse.
    pub fn asset_plane_info(&self) -> AssetPlaneInfo {
        AssetPlaneInfo {
            port: self.bound_port,
            bearer_token: self.bearer_token().to_string(),
            session_id: self.session_id().to_string(),
            version: ASSET_SERVER_VERSION,
            max_body_size: self.state.max_body_size,
            phases: vec!["screenshot-upload".to_string()],
        }
    }

    /// Update the CORS allow-origin once the extension has identified
    /// itself. Subsequent preflights echo the new origin.
    pub fn set_allowed_origin(&self, origin: Option<String>) {
        self.state.set_allowed_origin(origin);
    }

    /// Park a screenshot upload slot under `request_id`, then await the
    /// extension's POST. Returns the captured bytes or a timeout/cancel
    /// error.
    pub async fn await_screenshot(
        &self,
        request_id: &str,
        timeout: Duration,
    ) -> Result<Bytes, InboxError> {
        self.state
            .upload_inbox
            .await_request(request_id, timeout)
            .await
    }

    /// Step B caller: register a path for one-shot download via
    /// `/file/:token`. Returns the full URL the consumer (browser actor,
    /// ffmpeg subprocess) should fetch.
    pub fn register_download(
        &self,
        path: std::path::PathBuf,
        mime_type: String,
        file_name: String,
        ttl: Duration,
    ) -> String {
        use crate::agent::browser_input::upload::TokenEntry;
        use std::time::Instant;

        let entry = TokenEntry {
            path,
            mime_type,
            file_name,
            size: 0, // size is informational only — the GET reads from disk
            expires_at: Instant::now() + ttl,
        };
        let token = self.state.download_tokens.insert(entry);
        format!("http://127.0.0.1:{}/file/{}", self.bound_port, token)
    }

    /// Phase 5 caller: park a pending OAuth flow keyed by `state`.
    /// Returns the absolute callback URL the originator should hand to
    /// the OAuth provider as `redirect_uri`, plus the `oneshot::Receiver`
    /// that resolves when the provider redirects back to the callback
    /// route.
    ///
    /// `state` MUST be a high-entropy random string the originator
    /// generated and embedded in the OAuth authorize URL — the registry
    /// uses it as a single-use key, and the callback handler verifies
    /// the URL-path provider matches what was registered here.
    ///
    /// Default TTL of 5 minutes is enough for most OAuth flows; pass
    /// a custom `ttl` for slow-fixed flows.
    pub fn register_oauth_flow(
        &self,
        provider: &str,
        state: &str,
        ttl: Duration,
    ) -> (String, tokio::sync::oneshot::Receiver<oauth::OAuthCallbackResult>) {
        let rx = self.state.oauth_registry.register(provider, state, ttl);
        let url = format!(
            "http://127.0.0.1:{}/v1/oauth/{}/callback",
            self.bound_port, provider
        );
        (url, rx)
    }

    /// Phase 5 caller: register raw bytes for `/v1/blob/:id` GET.
    /// Returns the absolute blob URL containing the freshly-issued
    /// single-use token. Default TTL is [`BLOB_TOKEN_TTL`] (1 h) —
    /// callers can pass a shorter `ttl` for time-sensitive flows.
    ///
    /// Use case: tool dispatch result > 100 KB. The daemon parks the
    /// bytes here and returns a `BlobRef { blob_id, content_type, bytes }`
    /// over native messaging; the consumer (LLM client / sidebar) GETs
    /// the URL out-of-band to retrieve the bytes.
    pub fn register_blob(
        &self,
        bytes: Bytes,
        content_type: String,
        ttl: Duration,
    ) -> String {
        use std::time::Instant;
        let entry = token_store::BlobEntry {
            bytes,
            content_type,
            expires_at: Instant::now() + ttl,
        };
        let token = self.state.blob_tokens.insert(entry);
        format!("http://127.0.0.1:{}/v1/blob/{}", self.bound_port, token)
    }

    /// Phase 2 caller: register a composition for asset GET. Issues ONE
    /// short token covering all `asset_names`, returns a map of
    /// (asset_name → absolute URL). The token is multi-use within `ttl`;
    /// caller is responsible for choosing a sensible TTL — typically
    /// `COMPOSITION_TOKEN_TTL` (5 min, per design D12).
    ///
    /// The composition handler (`/v1/composition/:id`) calls this inline
    /// during the GET; non-handler callers (e.g. `load_composition` once
    /// it's wired through `HostServices::asset_server` in Step B) call it
    /// to embed URLs into a `GetCompositionResponse`.
    pub fn register_composition_assets(
        &self,
        composition_id: &str,
        asset_names: &[String],
        ttl: Duration,
    ) -> HashMap<String, String> {
        let token = self.state.issue_composition_token(composition_id, ttl);
        asset_names
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    format!(
                        "http://127.0.0.1:{}/v1/asset/composition/{}/{}?t={}",
                        self.bound_port, composition_id, name, token
                    ),
                )
            })
            .collect()
    }
}

fn build_router(state: Arc<AssetServerState>, max_body: usize) -> Router {
    // /v1/* routes that REQUIRE bearer (no per-resource short-token alternative).
    let v1_bearer = Router::new()
        .route("/v1/health", get(handlers::health::handle))
        .route("/v1/capabilities", get(handlers::capabilities::handle))
        .route(
            "/v1/upload/screenshot/:request_id",
            post(handlers::upload::handle_screenshot),
        )
        // Phase 3 — generic byte sink + asset drop (lit). Auth: bearer.
        .route(
            "/v1/upload/asset/:composition_id/:name",
            post(handlers::upload::handle_asset),
        )
        .route(
            "/v1/upload/generic/:inbox_id",
            post(handlers::upload::handle_generic),
        )
        // Phase 4 — render frame POST + SSE control channel.
        .route(
            "/v1/render/:job_id/frame",
            post(handlers::render::handle_frame),
        )
        .route(
            "/v1/render/:job_id/sse",
            get(handlers::render::handle_sse),
        )
        // Phase 5 — URL-as-handle blob registry.
        .route("/v1/blob/:id", get(handlers::blob::handle))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::bearer_middleware,
        ))
        .with_state(state.clone());

    // Phase 5 — OAuth callback. Bearer-LESS: the OAuth provider hits
    // this URL via browser redirect and has no daemon bearer token.
    // Auth is the `state` query parameter (one-shot CSRF nonce
    // generated by the originator + verified inside the handler against
    // OAuthRegistry).
    let v1_oauth = Router::new()
        .route(
            "/v1/oauth/:provider/callback",
            get(handlers::oauth::handle),
        )
        .with_state(state.clone());

    // /v1/* routes with TWO-TIER auth (bearer header OR per-composition
    // `?t=<short_token>` query param). Auth is enforced inside the handler
    // via `auth::check_composition_request_auth`, so these routes sit
    // outside the bearer middleware layer.
    let v1_composition = Router::new()
        .route("/v1/composition/:id", get(handlers::composition::handle))
        .route(
            "/v1/asset/composition/:id/:name",
            get(handlers::asset::handle),
        )
        .with_state(state.clone());

    // Legacy /file/:token — bearer-LESS (single-use UUID is the auth).
    let legacy = Router::new()
        .route("/file/:token", get(handlers::legacy_file::handle))
        .with_state(state.clone());

    Router::new()
        .merge(legacy)
        .merge(v1_composition)
        .merge(v1_oauth)
        .merge(v1_bearer)
        .layer(DefaultBodyLimit::max(max_body))
        // CORS layer must be outermost so OPTIONS preflights are answered
        // before the bearer layer rejects them.
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::cors_middleware,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::upload::{TokenEntry, TOKEN_TTL};
    use std::time::Instant;

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    async fn boot() -> AssetServer {
        AssetServer::start(AssetServerConfig {
            bearer_token: "test-bearer".into(),
            session_id: "test-session".into(),
            ..Default::default()
        })
        .await
        .expect("AssetServer should boot in tests")
    }

    // -----------------------------------------------------------------------
    // §13.1 — bearer auth
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn missing_bearer_returns_401() {
        let server = boot().await;
        let url = format!("http://127.0.0.1:{}/v1/health", server.bound_port());
        let resp = test_client().get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn wrong_bearer_returns_401() {
        let server = boot().await;
        let url = format!("http://127.0.0.1:{}/v1/health", server.bound_port());
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer nope")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn valid_bearer_returns_200_health() {
        let server = boot().await;
        let url = format!("http://127.0.0.1:{}/v1/health", server.bound_port());
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    // -----------------------------------------------------------------------
    // §13.4 — /v1/capabilities returns phases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn capabilities_lists_screenshot_upload_phase() {
        let server = boot().await;
        let url = format!("http://127.0.0.1:{}/v1/capabilities", server.bound_port());
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let phases = body["phases"].as_array().expect("phases must be array");
        assert!(phases
            .iter()
            .any(|v| v.as_str() == Some("screenshot-upload")));
        assert_eq!(body["version"], 1);
    }

    // -----------------------------------------------------------------------
    // §13.1 — CORS preflight
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn cors_preflight_returns_204_with_headers() {
        let server = boot().await;
        server.set_allowed_origin(Some("moz-extension://test-id".into()));
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/screenshot/abc",
            server.bound_port()
        );
        let resp = test_client()
            .request(reqwest::Method::OPTIONS, &url)
            .header("origin", "moz-extension://test-id")
            .header("access-control-request-method", "POST")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 204);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("moz-extension://test-id")
        );
    }

    // -----------------------------------------------------------------------
    // §13.1 — screenshot inbox roundtrip
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn screenshot_post_then_await_roundtrip() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/screenshot/req-roundtrip",
            server.bound_port()
        );
        // Producer first: extension POSTs.
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .body(b"PNG-bytes".to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        // Consumer then awaits — inbox should already have the bytes.
        let bytes = server
            .await_screenshot("req-roundtrip", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"PNG-bytes");
    }

    #[tokio::test]
    async fn screenshot_await_then_post_roundtrip() {
        // Consumer-first ordering: tool dispatch awaits before extension
        // POSTs. This is the production path.
        let server = boot().await;
        let server_clone = server.clone();
        let waiter = tokio::spawn(async move {
            server_clone
                .await_screenshot("req-await-first", Duration::from_secs(2))
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let url = format!(
            "http://127.0.0.1:{}/v1/upload/screenshot/req-await-first",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .body(b"capture".to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let bytes = waiter.await.unwrap().unwrap();
        assert_eq!(bytes.as_ref(), b"capture");
    }

    // -----------------------------------------------------------------------
    // §13.1 — 501 stubs carry X-NF-Future-Phase
    // -----------------------------------------------------------------------

    /// Phase 2 is now lit, so the previous "stub returns 501" test would
    /// false-pass. Replace with the graceful-degradation contract: when
    /// the AssetServer has no storage wired (test boot, or a daemon where
    /// Phase 2 hasn't been wired through start_server yet), the route
    /// returns 503 so callers can fall back to NM-only transport.
    #[tokio::test]
    async fn phase2_composition_route_returns_503_when_storage_not_wired() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/composition/abc",
            server.bound_port()
        );
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 503);
    }

    // Phase 4 render frame route is now lit; the previous "stub returns
    // 501" test was replaced by the round-trip + SSE tests below.

    // Phase 5 blob route is now lit; the previous "stub returns 501"
    // expectation was replaced by the round-trip + TTL tests below.

    // -----------------------------------------------------------------------
    // Legacy /file/:token wire-format spec — Step A asserted byte-for-byte
    // parity against the old file_server.rs; in Step B file_server.rs has
    // been deleted, so we now pin the wire shape directly.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn legacy_file_route_wire_format_is_stable() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let server = boot().await;

        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(b"hello legacy file route").unwrap();
        tf.flush().unwrap();
        let entry = TokenEntry {
            path: tf.path().to_path_buf(),
            mime_type: "text/plain".into(),
            file_name: "name with \"quotes\".txt".into(),
            size: 23,
            expires_at: Instant::now() + TOKEN_TTL,
        };
        let token = server.state().download_tokens.insert(entry);
        let url = format!("http://127.0.0.1:{}/file/{}", server.bound_port(), token);

        let resp = test_client().get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/plain")
        );
        assert_eq!(
            resp.headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok()),
            // Quote inside the filename must be escaped — same shape as
            // the original file_server.rs::handle_file_download.
            Some("attachment; filename=\"name with \\\"quotes\\\".txt\"")
        );
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"hello legacy file route");
    }

    #[tokio::test]
    async fn legacy_file_token_is_single_use() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let server = boot().await;
        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(b"once").unwrap();
        tf.flush().unwrap();
        let entry = TokenEntry {
            path: tf.path().to_path_buf(),
            mime_type: "text/plain".into(),
            file_name: "once.txt".into(),
            size: 4,
            expires_at: Instant::now() + TOKEN_TTL,
        };
        let token = server.state().download_tokens.insert(entry);
        let url = format!("http://127.0.0.1:{}/file/{}", server.bound_port(), token);

        let r1 = test_client().get(&url).send().await.unwrap();
        assert_eq!(r1.status(), 200);
        let r2 = test_client().get(&url).send().await.unwrap();
        assert_eq!(r2.status(), 404);
    }

    // -----------------------------------------------------------------------
    // §13.3 — bridge:hello asset_plane shape
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn asset_plane_info_advertises_correct_fields() {
        let server = boot().await;
        let info = server.asset_plane_info();
        assert_eq!(info.port, server.bound_port());
        assert_eq!(info.bearer_token, "test-bearer");
        assert_eq!(info.session_id, "test-session");
        assert_eq!(info.version, ASSET_SERVER_VERSION);
        assert_eq!(info.max_body_size, DEFAULT_MAX_BODY_SIZE);
        assert!(info.phases.contains(&"screenshot-upload".to_string()));

        let json = serde_json::to_value(&info).unwrap();
        assert!(json.get("port").is_some());
        assert!(json.get("bearer_token").is_some());
        assert!(json.get("session_id").is_some());
    }

    // -----------------------------------------------------------------------
    // §13.5 — two daemons co-exist with distinct asset_plane.port
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn two_servers_pick_distinct_ports() {
        let s1 = boot().await;
        let s2 = boot().await;
        assert_ne!(
            s1.bound_port(),
            s2.bound_port(),
            "two AssetServers must end up on different ports"
        );
        let r = AssetServerConfig::default().port_range;
        assert!(r.contains(&s1.bound_port()));
        assert!(r.contains(&s2.bound_port()));
    }

    // -----------------------------------------------------------------------
    // Step B caller: register_download returns a working URL
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn register_download_returns_fetchable_url() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let server = boot().await;
        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(b"register_download payload").unwrap();
        tf.flush().unwrap();

        let url = server.register_download(
            tf.path().to_path_buf(),
            "text/plain".into(),
            "registered.txt".into(),
            DOWNLOAD_TOKEN_TTL,
        );
        assert!(url.starts_with(&format!("http://127.0.0.1:{}/file/", server.bound_port())));

        let resp = test_client().get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"register_download payload");

        // Single-use: second GET 404s.
        let r2 = test_client().get(&url).send().await.unwrap();
        assert_eq!(r2.status(), 404);
    }

    // -----------------------------------------------------------------------
    // §7.1 — Phase 2 composition + asset GET routes
    // -----------------------------------------------------------------------

    /// 1×1 transparent PNG, base64-encoded — used as a stand-in binary asset.
    const PNG_1X1_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    fn sample_meta() -> String {
        serde_json::json!({
            "kind": "composition",
            "version": 1,
            "spec": {"width": 640, "height": 360, "duration_sec": 5.0, "fps": 30},
            "origin": {"created_with": "test", "created_at": 0}
        })
        .to_string()
    }

    /// Boot an AssetServer backed by a real in-memory artifact store with a
    /// fixture composition: HTML referencing two assets, plus the assets
    /// themselves.
    async fn boot_with_fixture() -> (
        AssetServer,
        std::sync::Arc<nevoflux_storage::Database>,
        String,
    ) {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use nevoflux_storage::repositories::{ArtifactRepository, CompositionAssetRepository};
        use nevoflux_storage::Database;

        let db = std::sync::Arc::new(
            Database::open_in_memory().expect("in-memory Database for asset_server tests"),
        );

        // Post-migration-016 shape: text files in artifacts.files,
        // binary assets in the composition_assets table.
        let mut files = std::collections::HashMap::new();
        files.insert(
            "index.html".into(),
            r#"<html><body>
                <img src="assets/hero.png">
                <video src="assets/clip.mp4"></video>
            </body></html>"#
                .to_string(),
        );
        files.insert("composition.meta.json".into(), sample_meta());

        let repo = ArtifactRepository::new(&db);
        let id = "comp-fixture".to_string();
        repo.create(nevoflux_storage::CreateArtifactParams {
            id: id.clone(),
            session_id: None,
            title: "fixture".into(),
            description: None,
            content_type: "text/html".into(),
            content: files["index.html"].clone(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .expect("create fixture artifact");

        // Fixture assets — go into the dedicated table.
        let asset_repo = CompositionAssetRepository::new(&db);
        let png_bytes = STANDARD.decode(PNG_1X1_B64.as_bytes()).unwrap();
        asset_repo
            .upsert(&id, "hero.png", &png_bytes, Some("image/png"))
            .unwrap();
        // mp4 stub — `ftyp` magic at bytes 4-7 drives video/mp4 sniff.
        let mp4_bytes = vec![
            0u8, 0, 0, 0x18, 0x66, 0x74, 0x79, 0x70, b'i', b's', b'o', b'm',
        ];
        asset_repo
            .upsert(&id, "clip.mp4", &mp4_bytes, Some("video/mp4"))
            .unwrap();
        // Text asset — drives the no-magic-match branch in the asset
        // GET handler (length 17 → drives the Range test's
        // Content-Range total).
        asset_repo
            .upsert(&id, "note.txt", b"hello asset plane", Some("text/plain"))
            .unwrap();

        let server = AssetServer::start(AssetServerConfig {
            bearer_token: "test-bearer".into(),
            session_id: "test-session".into(),
            storage: Some(std::sync::Arc::clone(&db)),
            ..Default::default()
        })
        .await
        .expect("AssetServer should boot with storage");

        (server, db, id)
    }

    #[tokio::test]
    async fn composition_route_rewrites_relative_assets_to_absolute_urls() {
        let (server, _storage, id) = boot_with_fixture().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/composition/{}",
            server.bound_port(),
            id
        );
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(
            !body.contains("data:image"),
            "response must NOT inline data: URIs (Phase 2 contract). got: {body}"
        );
        // hero.png and clip.mp4 must both be rewritten to v1/asset/composition/...
        // URLs that include the composition id and a token query param.
        assert!(
            body.contains("/v1/asset/composition/comp-fixture/hero.png?t="),
            "missing rewritten hero.png URL: {body}"
        );
        assert!(
            body.contains("/v1/asset/composition/comp-fixture/clip.mp4?t="),
            "missing rewritten clip.mp4 URL: {body}"
        );
        assert!(!body.contains(r#"src="assets/hero.png""#));
    }

    #[tokio::test]
    async fn composition_route_returns_etag_and_cache_control() {
        let (server, _storage, id) = boot_with_fixture().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/composition/{}",
            server.bound_port(),
            id
        );
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("cache-control")
                .and_then(|v| v.to_str().ok()),
            Some("private, max-age=300")
        );
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .expect("etag header present")
            .to_string();
        assert!(etag.starts_with('"') && etag.ends_with('"'));

        // If-None-Match revalidation returns 304.
        let resp304 = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .header("if-none-match", &etag)
            .send()
            .await
            .unwrap();
        assert_eq!(resp304.status(), 304);
    }

    #[tokio::test]
    async fn composition_route_token_query_param_bypasses_bearer() {
        let (server, _storage, id) = boot_with_fixture().await;
        // Pre-issue a composition token via the typed API so we have one
        // we can present without going through the bearer-protected GET.
        let urls = server.register_composition_assets(
            &id,
            &[String::from("hero.png")],
            std::time::Duration::from_secs(60),
        );
        let issued_url = &urls["hero.png"];
        let token = issued_url
            .rsplit_once("t=")
            .expect("issued url has token")
            .1
            .to_string();

        // Hitting /v1/composition/<id> with ?t=<token> and NO bearer must succeed.
        let url = format!(
            "http://127.0.0.1:{}/v1/composition/{}?t={}",
            server.bound_port(),
            id,
            token
        );
        let resp = test_client().get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);

        // No bearer + no/wrong token → 401.
        let bare_url = format!(
            "http://127.0.0.1:{}/v1/composition/{}",
            server.bound_port(),
            id
        );
        let resp_no_auth = test_client().get(&bare_url).send().await.unwrap();
        assert_eq!(resp_no_auth.status(), 401);
    }

    #[tokio::test]
    async fn composition_route_returns_404_for_unknown_artifact_id() {
        let (server, _storage, _id) = boot_with_fixture().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/composition/does-not-exist",
            server.bound_port()
        );
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn asset_route_streams_bytes_with_correct_mime() {
        let (server, _storage, id) = boot_with_fixture().await;
        // Issue a composition token; use it via ?t= (the iframe / <img>
        // path consumers can't add headers).
        let urls = server.register_composition_assets(
            &id,
            &[String::from("hero.png"), String::from("note.txt")],
            std::time::Duration::from_secs(60),
        );

        // Binary asset: PNG, magic-bytes sniffed.
        let resp = test_client().get(&urls["hero.png"]).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("image/png")
        );
        let body = resp.bytes().await.unwrap();
        // First 8 bytes must be the PNG signature.
        assert_eq!(
            &body[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );

        // Text asset: stored as raw UTF-8, mime resolved by extension.
        let resp_txt = test_client().get(&urls["note.txt"]).send().await.unwrap();
        assert_eq!(resp_txt.status(), 200);
        assert_eq!(
            resp_txt
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/plain")
        );
        let txt = resp_txt.text().await.unwrap();
        assert_eq!(txt, "hello asset plane");
    }

    #[tokio::test]
    async fn asset_route_supports_range_requests() {
        let (server, _storage, id) = boot_with_fixture().await;
        let urls = server.register_composition_assets(
            &id,
            &[String::from("note.txt")],
            std::time::Duration::from_secs(60),
        );
        let resp = test_client()
            .get(&urls["note.txt"])
            .header("range", "bytes=0-4")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 206);
        assert_eq!(
            resp.headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok()),
            Some("bytes 0-4/17")
        );
        let body = resp.text().await.unwrap();
        assert_eq!(body, "hello");
    }

    #[tokio::test]
    async fn asset_route_short_token_multi_use_within_ttl() {
        let (server, _storage, id) = boot_with_fixture().await;
        let urls = server.register_composition_assets(
            &id,
            &[String::from("hero.png")],
            std::time::Duration::from_secs(60),
        );
        let url = &urls["hero.png"];

        // Multi-use: same URL/token must serve at least twice within TTL.
        let r1 = test_client().get(url).send().await.unwrap();
        assert_eq!(r1.status(), 200);
        let r2 = test_client().get(url).send().await.unwrap();
        assert_eq!(r2.status(), 200);
    }

    #[tokio::test]
    async fn asset_route_rejects_token_for_different_composition() {
        let (server, _storage, id) = boot_with_fixture().await;
        // Token issued for the fixture composition must NOT auth a request
        // against a different composition id (defense-in-depth: handler
        // verifies token's stored composition_id matches the URL path).
        let urls = server.register_composition_assets(
            &id,
            &[String::from("hero.png")],
            std::time::Duration::from_secs(60),
        );
        let issued = &urls["hero.png"];
        let token = issued.rsplit_once("t=").unwrap().1;
        let cross_url = format!(
            "http://127.0.0.1:{}/v1/asset/composition/SOME-OTHER-ID/hero.png?t={}",
            server.bound_port(),
            token
        );
        let resp = test_client().get(&cross_url).send().await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    // -----------------------------------------------------------------------
    // §7.3 — Phase 3 generic upload + asset drop
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn generic_upload_returns_201_with_sha_and_metadata() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/generic/inbox-abc",
            server.bound_port()
        );
        let body = b"hello generic plane".to_vec();
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .header("content-type", "application/octet-stream")
            .header("x-nf-filename", "note.bin")
            .body(body.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["id"], "inbox-abc");
        assert_eq!(json["bytes"], body.len());
        assert_eq!(json["filename"], "note.bin");
        assert_eq!(json["content_type"], "application/octet-stream");
        // SHA-256 of "hello generic plane"
        let sha = json["sha256"].as_str().unwrap();
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&body);
        let expected = format!("{:x}", h.finalize());
        assert_eq!(sha, expected);
    }

    #[tokio::test]
    async fn generic_upload_then_await_inbox_returns_bytes() {
        // Upload-then-consume round-trip — proves the generic route shares
        // the same `upload_inbox` plumbing as the screenshot route, so a
        // tool dispatcher waiting on `await_inbox(inbox_id)` sees the
        // bytes a content-script POST delivered.
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/generic/inbox-roundtrip",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .body(b"pasted data".to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);

        // The screenshot helper uses upload_inbox under the hood — the
        // same store backs the generic route, so reusing await_screenshot
        // here is intentional (and verifies the contract).
        let bytes = server
            .await_screenshot("inbox-roundtrip", std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"pasted data");
    }

    #[tokio::test]
    async fn generic_upload_missing_bearer_returns_401() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/generic/inbox-noauth",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .body(b"x".to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn asset_upload_returns_503_when_canvas_video_service_not_wired() {
        // Test boot has neither storage nor canvas_video_service. Asset
        // upload should fail gracefully so callers fall back to NM
        // (matches the composition GET 503 contract).
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/upload/asset/comp-x/hero.png",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .header("content-type", "image/png")
            .body(vec![0u8; 8])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 503);
    }

    #[tokio::test]
    async fn asset_upload_persists_into_artifact_files_and_serves_via_get() {
        // End-to-end: POST raw PNG bytes → daemon attach_asset writes
        // base64 into artifacts.files['assets/hero.png'] → subsequent
        // /v1/asset/composition/<id>/hero.png GET returns the same bytes.
        use crate::canvas_video::CanvasVideoService;
        use nevoflux_storage::repositories::ArtifactRepository;

        // 1. Build a CanvasVideoService with a fixture composition (no
        //    asset yet — the upload will add it).
        let svc = std::sync::Arc::new(CanvasVideoService::new_for_tests());
        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let id = "comp-phase3-upload";
        let mut files = std::collections::HashMap::new();
        files.insert(
            "index.html".into(),
            r#"<html><body><img src="assets/hero.png"></body></html>"#.to_string(),
        );
        files.insert(
            "composition.meta.json".into(),
            serde_json::json!({
                "kind": "composition", "version": 1,
                "spec": {"width": 640, "height": 360, "duration_sec": 5.0, "fps": 30},
                "origin": {"created_with": "phase3-test", "created_at": 0}
            })
            .to_string(),
        );
        repo.create(nevoflux_storage::CreateArtifactParams {
            id: id.into(),
            session_id: None,
            title: "phase3 fixture".into(),
            description: None,
            content_type: "text/html".into(),
            content: files["index.html"].clone(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();

        let db_arc = std::sync::Arc::new(svc.storage().unwrap().database().clone());
        let server = AssetServer::start(AssetServerConfig {
            bearer_token: "test-bearer".into(),
            session_id: "test-session".into(),
            storage: Some(db_arc),
            canvas_video_service: Some(svc.clone()),
            ..Default::default()
        })
        .await
        .expect("AssetServer should boot for phase3 upload test");

        // 2. POST raw PNG bytes (1×1 transparent PNG, 67 bytes — small
        //    enough that resize is a no-op).
        const PNG_1X1: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // sig
            0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T',
            0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05,
            0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4,
            0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D',
            0xAE, 0x42, 0x60, 0x82,
        ];
        let upload_url = format!(
            "http://127.0.0.1:{}/v1/upload/asset/{}/hero.png",
            server.bound_port(),
            id
        );
        let resp = test_client()
            .post(&upload_url)
            .header("authorization", "Bearer test-bearer")
            .header("content-type", "image/png")
            .body(PNG_1X1.to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["composition_id"], id);
        assert_eq!(json["path"], "assets/hero.png");

        // 3. Confirm bytes round-trip via the Phase 2 asset GET route.
        let urls = server.register_composition_assets(
            id,
            &[String::from("hero.png")],
            std::time::Duration::from_secs(60),
        );
        let resp = test_client().get(&urls["hero.png"]).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.bytes().await.unwrap();
        // First 8 bytes match the PNG signature — proves the upload's
        // bytes survived the base64 round-trip and the magic-byte sniff
        // selected `image/png`.
        assert_eq!(&body[..8], &PNG_1X1[..8]);
    }

    // -----------------------------------------------------------------------
    // §7.4 — Phase 5 URL-as-handle blob registry
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn blob_register_then_fetch_round_trips_bytes_and_content_type() {
        let server = boot().await;
        let url = server.register_blob(
            bytes::Bytes::from_static(b"large blob payload"),
            "text/plain".into(),
            BLOB_TOKEN_TTL,
        );
        assert!(url.starts_with(&format!("http://127.0.0.1:{}/v1/blob/", server.bound_port())));
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/plain")
        );
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"large blob payload");
    }

    #[tokio::test]
    async fn blob_token_is_single_use() {
        let server = boot().await;
        let url = server.register_blob(
            bytes::Bytes::from_static(b"once"),
            "application/octet-stream".into(),
            BLOB_TOKEN_TTL,
        );
        let r1 = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(r1.status(), 200);
        let r2 = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(r2.status(), 404);
    }

    #[tokio::test]
    async fn blob_route_unknown_id_returns_404() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/blob/never-registered",
            server.bound_port()
        );
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn blob_expired_returns_404() {
        let server = boot().await;
        let url = server.register_blob(
            bytes::Bytes::from_static(b"expired"),
            "text/plain".into(),
            // negative-ish TTL: 1 ms in the future, sleep past it.
            std::time::Duration::from_millis(1),
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn blob_route_requires_bearer() {
        let server = boot().await;
        let url = server.register_blob(
            bytes::Bytes::from_static(b"x"),
            "text/plain".into(),
            BLOB_TOKEN_TTL,
        );
        let resp = test_client().get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    // -----------------------------------------------------------------------
    // §7.5 — Phase 5 OAuth callback consolidation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn oauth_callback_dispatches_code_to_originator() {
        let server = boot().await;
        let state_nonce = "nonce-success-001";
        let (cb_url, rx) = server.register_oauth_flow(
            "anthropic",
            state_nonce,
            std::time::Duration::from_secs(30),
        );
        // Simulate the OAuth provider hitting our callback (no bearer).
        let resp = test_client()
            .get(format!("{cb_url}?code=auth-code-XYZ&state={state_nonce}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        // HTML response carries the success card.
        let body = resp.text().await.unwrap();
        assert!(body.contains("OAuth flow complete"));
        // Originator side: the oneshot now has the result.
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.provider, "anthropic");
        assert_eq!(result.state, state_nonce);
        assert_eq!(result.code.as_deref(), Some("auth-code-XYZ"));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn oauth_callback_dispatches_provider_error_to_originator() {
        let server = boot().await;
        let state_nonce = "nonce-error-001";
        let (cb_url, rx) = server.register_oauth_flow(
            "anthropic",
            state_nonce,
            std::time::Duration::from_secs(30),
        );
        let resp = test_client()
            .get(format!(
                "{cb_url}?error=access_denied&error_description=user_declined&state={state_nonce}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .unwrap()
            .unwrap();
        assert!(result.code.is_none());
        assert_eq!(result.error.as_deref(), Some("access_denied"));
        assert_eq!(result.error_description.as_deref(), Some("user_declined"));
    }

    #[tokio::test]
    async fn oauth_callback_unknown_state_returns_200_html_but_does_not_dispatch() {
        // Provider redirect with a state we never registered. We still
        // return 200 (the user's browser is staring at the page; throwing
        // 404 would just confuse them). No oneshot is dispatched.
        let server = boot().await;
        let cb_url = format!(
            "http://127.0.0.1:{}/v1/oauth/anthropic/callback",
            server.bound_port()
        );
        let resp = test_client()
            .get(format!("{cb_url}?code=x&state=ghost"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("no matching pending flow"),
            "expected fallback message, got: {body}"
        );
    }

    #[tokio::test]
    async fn oauth_callback_provider_mismatch_does_not_leak_to_other_provider_originator() {
        // Originator registered for `anthropic`, but the callback URL
        // path says `github`. Defense-in-depth: don't dispatch — and
        // (per OAuthRegistry::resolve) the registered entry IS consumed
        // (single-use even on mismatch) so a subsequent legit callback
        // won't double-dispatch either.
        let server = boot().await;
        let state_nonce = "nonce-cross-001";
        let (_anthropic_cb, rx) = server.register_oauth_flow(
            "anthropic",
            state_nonce,
            std::time::Duration::from_secs(30),
        );
        let github_cb_url = format!(
            "http://127.0.0.1:{}/v1/oauth/github/callback",
            server.bound_port()
        );
        let resp = test_client()
            .get(format!("{github_cb_url}?code=x&state={state_nonce}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        // Originator's oneshot must NOT have received a value. The
        // provider-mismatch path inside OAuthRegistry::resolve consumes
        // the entry (single-use even on mismatch — defense against
        // replay) which drops the Sender; the Receiver therefore
        // resolves to `Err(_)` (channel closed) rather than getting a
        // bogus value. Either way, no `Ok(callback)` is delivered.
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .expect("rx must resolve quickly");
        assert!(
            result.is_err(),
            "originator must NOT receive a callback for cross-provider hits, got Ok({:?})",
            result.ok()
        );
    }

    #[tokio::test]
    async fn oauth_callback_missing_state_param_returns_html_no_dispatch() {
        let server = boot().await;
        let cb_url = format!(
            "http://127.0.0.1:{}/v1/oauth/anthropic/callback?code=x",
            server.bound_port()
        );
        let resp = test_client().get(&cb_url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.contains("Missing"));
    }

    #[tokio::test]
    async fn oauth_callback_route_does_not_require_bearer() {
        // OAuth providers don't have our bearer. The state query param
        // is the auth.
        let server = boot().await;
        let state_nonce = "nonce-nobearer-001";
        let (cb_url, _rx) = server.register_oauth_flow(
            "anthropic",
            state_nonce,
            std::time::Duration::from_secs(30),
        );
        let resp = test_client()
            .get(format!("{cb_url}?code=x&state={state_nonce}"))
            .send() // no Authorization header
            .await
            .unwrap();
        // 200, NOT 401.
        assert_eq!(resp.status(), 200);
    }

    // -----------------------------------------------------------------------
    // §7.2 — Phase 4 render frame POST + SSE control channel
    // -----------------------------------------------------------------------

    /// Boot an AssetServer wired to a real CanvasVideoService so the
    /// frame POST handler can resolve a job snapshot + push frames.
    async fn boot_with_canvas_video() -> (AssetServer, std::sync::Arc<crate::canvas_video::CanvasVideoService>) {
        let svc = std::sync::Arc::new(crate::canvas_video::CanvasVideoService::new_for_tests());
        let db_arc = std::sync::Arc::new(svc.storage().unwrap().database().clone());
        let server = AssetServer::start(AssetServerConfig {
            bearer_token: "test-bearer".into(),
            session_id: "test-session".into(),
            storage: Some(db_arc),
            canvas_video_service: Some(svc.clone()),
            ..Default::default()
        })
        .await
        .expect("AssetServer should boot for phase4 test");
        (server, svc)
    }

    #[tokio::test]
    async fn render_frame_post_returns_503_when_canvas_video_not_wired() {
        let server = boot().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/render/job-x/frame",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .header("x-nf-frame-index", "0")
            .body(vec![0u8; 8])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 503);
    }

    #[tokio::test]
    async fn render_frame_post_409s_for_unknown_job() {
        let (server, _svc) = boot_with_canvas_video().await;
        let url = format!(
            "http://127.0.0.1:{}/v1/render/never-registered/frame",
            server.bound_port()
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .header("x-nf-frame-index", "0")
            .body(vec![0u8; 4])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 409);
    }

    #[tokio::test]
    async fn render_frame_post_400s_when_index_header_missing() {
        let (server, svc) = boot_with_canvas_video().await;
        // Make a Queued job so we get past the snapshot check before
        // the header parse.
        let job_id = svc.jobs().create("comp-x".into(), 320, 180, 1.0, 30).await;
        let url = format!(
            "http://127.0.0.1:{}/v1/render/{}/frame",
            server.bound_port(),
            job_id
        );
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .body(vec![0u8; 4])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn render_frame_post_delivers_to_signal_channel() {
        let (server, svc) = boot_with_canvas_video().await;
        // Set up a job AND register the render-loop signal channel
        // (mirrors `render_start`'s setup). The receiver is what the
        // render loop drains in production; here we just await one frame.
        let job_id = svc.jobs().create("comp-x".into(), 320, 180, 1.0, 30).await;
        let mut rx = svc.register_job_signal_channel(&job_id).await;

        let url = format!(
            "http://127.0.0.1:{}/v1/render/{}/frame",
            server.bound_port(),
            job_id
        );
        let png_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xDE, 0xAD];
        let resp = test_client()
            .post(&url)
            .header("authorization", "Bearer test-bearer")
            .header("x-nf-frame-index", "42")
            .header("content-type", "image/png")
            .body(png_bytes.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 204);

        // Render loop sees the frame on its signal channel.
        let signal = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("frame must arrive on signal channel")
            .expect("channel must yield Some");
        match signal {
            crate::canvas_video::service::FrameSignal::Frame { frame_idx, png } => {
                assert_eq!(frame_idx, 42);
                assert_eq!(png, png_bytes);
            }
            other => panic!("expected Frame variant, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn render_sse_emits_cancel_event_when_broadcast() {
        // Subscribe via SSE → broadcast Cancel → reading the SSE stream
        // sees `data: {"type":"cancel"}`. Using reqwest's chunked body
        // reader directly so we don't need a full SSE client lib.
        let (server, svc) = boot_with_canvas_video().await;
        let job_id = svc.jobs().create("comp-x".into(), 320, 180, 1.0, 30).await;

        let url = format!(
            "http://127.0.0.1:{}/v1/render/{}/sse",
            server.bound_port(),
            job_id
        );
        let mut resp = test_client()
            .get(&url)
            .header("authorization", "Bearer test-bearer")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/event-stream")
        );

        // Subscriber must be registered before broadcast or it misses
        // the event. The handler subscribes before responding, so by
        // the time we have a 200 here we know the broadcast channel
        // exists.
        svc.broadcast_render_control(
            &job_id,
            crate::canvas_video::RenderControlEvent::Cancel,
        )
        .await;

        // Read the next chunk that contains a `data:` line carrying our
        // cancel event. Bound by 2 s — keep-alive comments may slip in
        // first, so loop until we see a data line.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut found = false;
        while tokio::time::Instant::now() < deadline {
            let chunk = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                resp.chunk(),
            )
            .await;
            match chunk {
                Ok(Ok(Some(bytes))) => {
                    let s = String::from_utf8_lossy(&bytes).to_string();
                    if s.contains(r#"data: {"type":"cancel"}"#) {
                        found = true;
                        break;
                    }
                }
                _ => continue,
            }
        }
        assert!(found, "expected cancel event in SSE stream");
    }
}
