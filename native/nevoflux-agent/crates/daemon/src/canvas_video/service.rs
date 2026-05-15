//! CanvasVideoService — dependency bag + method surface.
//!
//! The actor-rework (2026-04-20) moved `canvas.video.*` transport from the
//! WebExtension background script onto the `NevofluxParent`/`NevofluxChild`
//! JSActor pair. The daemon no longer pushes seek commands to the page: the
//! render page drives the loop itself (single-threaded JS `for` over frame
//! indices) and streams chunks back. Accordingly, `BridgeSender` and the
//! `push_canvas_video_*` helpers have been removed.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};

use crate::canvas_video::{
    create,
    frame_chunks::ChunkBuffer,
    job::{JobRegistry, JobSnapshot},
    render,
};
use crate::error::{DaemonError, Result};
use crate::event_bus::{BusEvent, EventBus, PublisherIdentity};
use nevoflux_protocol::canvas_video::{
    CreateCompositionRequest, CreateCompositionResponse, InspectReport, LintReport,
    RenderFrameChunk, RenderStartRequest, RenderStartResponse,
};
use nevoflux_skills::SkillRegistry;
use nevoflux_storage::Storage;

/// Single-stream signal the render loop consumes for a given job.
#[derive(Debug)]
pub enum FrameSignal {
    /// A complete PNG frame arrived from the page.
    Frame { frame_idx: u32, png: Vec<u8> },
    /// Page reports it has emitted the final frame; finalize the encode.
    Done { frames_emitted: u32 },
    /// Page reports an unrecoverable error; abort the job.
    Failed(String),
}

/// Daemon-to-render-tab control signal (Phase 4 SSE channel).
///
/// The render tab opens an SSE connection on entry and listens for
/// these events; the daemon broadcasts them when the user pauses /
/// cancels from the sidebar (or the tool dispatcher decides to abort).
#[derive(Debug, Clone)]
pub enum RenderControlEvent {
    /// User asked to cancel — render tab should stop capturing and
    /// allow the loop to finalize/abort cleanly.
    Cancel,
    /// Seek the loop to the given frame index. Reserved for future
    /// scrubbing support; the legacy NM path doesn't carry this either.
    SeekTo(u32),
}

pub struct CanvasVideoService {
    jobs: JobRegistry,
    /// If true, render_start returns immediately without page-bridge interaction.
    bridge_stub: bool,

    // --- bridge-side async coordination ---
    /// job_id -> ChunkBuffer accumulating incoming frame chunk pieces.
    chunk_buffers: Mutex<HashMap<String, ChunkBuffer>>,

    /// job_id -> unbounded sender into the render loop. One channel per active
    /// job, fed by `on_frame_chunk` / `on_render_done` / `on_render_failed`.
    signal_senders: Mutex<HashMap<String, mpsc::UnboundedSender<FrameSignal>>>,

    /// EventBus handle for jobs.render.{id} progress + terminal events.
    /// None in stub / test mode; emits become no-ops.
    event_bus: Option<Arc<EventBus>>,

    /// Persistent artifact storage. Set by server.rs via `with_storage`; used
    /// by T6 to persist compositions to the artifact repo.
    storage: Option<Arc<Storage>>,

    /// Skill registry for reading auxiliary files (templates, DESIGN-template).
    /// Shared with HostServices so both see the same loaded registry.
    skills: Option<Arc<RwLock<SkillRegistry>>>,

    /// Pending lint correlators: correlator -> oneshot sender for the lint result.
    /// Inserted by `lint_composition`, removed by `on_lint_result` or on timeout.
    lint_correlators: Mutex<HashMap<String, oneshot::Sender<LintReport>>>,

    /// Pending inspect correlators — same pattern as `lint_correlators`.
    /// Inserted by `inspect_layout`, removed by `on_inspect_result` or on timeout.
    inspect_correlators: Mutex<HashMap<String, oneshot::Sender<InspectReport>>>,

    /// Per-job daemon→render-tab control broadcast (Phase 4 SSE).
    /// `subscribe_render_control` lazily creates the broadcast channel
    /// on first subscription; `broadcast_render_control` looks it up
    /// and pushes events. Capacity is small (32) — control events are
    /// rare (cancel, seek) and slow consumers can tolerate lag.
    render_controls: Mutex<HashMap<String, tokio::sync::broadcast::Sender<RenderControlEvent>>>,

    /// Asset & Stream Plane HTTP server, set by `set_asset_server` after
    /// the daemon boots the server (it lives on `HostServices` and is
    /// constructed AFTER this service in `start_server`). When set,
    /// `load_composition` rewrites `assets/X` references in the entry HTML
    /// to absolute `/v1/asset/composition/...` URLs instead of inlining
    /// data URIs (Phase 2 of the asset-stream-plane design).
    asset_server: std::sync::OnceLock<crate::asset_server::AssetServer>,
}

/// Pure throttle decision for progress deliveries. Emits when `current`
/// lands on a multiple of `throttle = max(1, total/20)`, OR when
/// `current == total` (so the final frame always lands on the bus).
///
/// P2 design §4.3: reduces 1080p × 30 s (900 frames) from 900 events to
/// ~21. At small totals (< 20) throttle is 1 so every frame emits —
/// acceptable since the total cost is bounded.
pub(crate) fn should_emit_progress(current: u32, total: u32) -> bool {
    let throttle = (total / 20).max(1);
    current % throttle == 0 || current == total
}

/// Detect a binary asset's true file extension from the first few bytes
/// of its base64-encoded payload. Returns `None` if the payload is text /
/// non-base64 / unrecognized.
///
/// We decode just the first ~16 bytes (24 base64 chars) — enough to read
/// every magic number we care about without paying for a full decode of
/// a multi-MB asset.
fn magic_bytes_extension(payload_b64: &str) -> Option<&'static str> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    // Skip leading whitespace; `decode` is strict about that.
    let head: String = payload_b64
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(24)
        .collect();
    if head.len() < 8 {
        return None;
    }
    // Pad to a multiple of 4 chars so STANDARD.decode accepts it.
    let padded = match head.len() % 4 {
        0 => head,
        n => format!("{head}{}", "=".repeat(4 - n)),
    };
    let bytes = STANDARD.decode(padded.as_bytes()).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(match bytes.as_slice() {
        // Image magics
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => "png",
        [0xFF, 0xD8, 0xFF, ..] => "jpg",
        [0x47, 0x49, 0x46, 0x38, ..] => "gif",
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => "webp",
        // SVG sometimes lacks XML decl — text-like start; skip detection.
        // Video magics (mp4 ftyp box)
        [_, _, _, _, 0x66, 0x74, 0x79, 0x70, ..] => "mp4",
        // Audio
        [0x49, 0x44, 0x33, ..] => "mp3", // ID3v2
        [0xFF, 0xFB, ..] | [0xFF, 0xF3, ..] | [0xFF, 0xF2, ..] => "mp3", // raw MPEG audio
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x41, 0x56, 0x45, ..] => "wav",
        [0x4F, 0x67, 0x67, 0x53, ..] => "ogg",
        // Fonts
        [b'w', b'O', b'F', b'2', ..] => "woff2",
        [b'w', b'O', b'F', b'F', ..] => "woff",
        [0x00, 0x01, 0x00, 0x00, ..] => "ttf",
        [b'O', b'T', b'T', b'O', ..] => "otf",
        _ => return None,
    })
}

/// Replace the extension of `name` with `new_ext`. If `name` has no
/// extension, append `.<new_ext>`.
fn override_extension(name: &str, new_ext: &str) -> String {
    if let Some(dot) = name.rfind('.') {
        let stem = &name[..dot];
        format!("{stem}.{new_ext}")
    } else {
        format!("{name}.{new_ext}")
    }
}

/// Pull `(spec.width, spec.height)` out of the composition's
/// `composition.meta.json` if available. Returns None on any parse
/// failure — caller defaults to a generous bounding box.
fn read_stage_dims(files: &std::collections::HashMap<String, String>) -> Option<(u32, u32)> {
    let raw = files.get("composition.meta.json")?;
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let w = v.get("spec")?.get("width")?.as_u64()? as u32;
    let h = v.get("spec")?.get("height")?.as_u64()? as u32;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

/// Sanitize an asset filename: strip path traversal, collapse to a single
/// `<basename>.<ext>` form. Drops `..`, leading `/`, and disallowed chars.
/// Empty / weird input falls back to `asset.bin`.
fn sanitize_asset_name(name: &str) -> String {
    // Take the basename only (no directory traversal allowed).
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(*c, '.' | '-' | '_'))
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "asset.bin".into();
    }
    cleaned
}

impl CanvasVideoService {
    pub fn new() -> Self {
        Self {
            jobs: JobRegistry::new(),
            bridge_stub: false,
            chunk_buffers: Mutex::new(Default::default()),
            signal_senders: Mutex::new(Default::default()),
            event_bus: None,
            storage: None,
            skills: None,
            lint_correlators: Mutex::new(Default::default()),
            inspect_correlators: Mutex::new(Default::default()),
            render_controls: Mutex::new(Default::default()),
            asset_server: std::sync::OnceLock::new(),
        }
    }

    pub fn new_for_tests() -> Self {
        let storage = Storage::open_in_memory().expect("new_for_tests: in-memory Storage");
        Self {
            jobs: JobRegistry::new(),
            bridge_stub: true,
            chunk_buffers: Mutex::new(Default::default()),
            signal_senders: Mutex::new(Default::default()),
            event_bus: None,
            storage: Some(Arc::new(storage)),
            skills: Some(Arc::new(RwLock::new(SkillRegistry::new()))),
            lint_correlators: Mutex::new(Default::default()),
            inspect_correlators: Mutex::new(Default::default()),
            render_controls: Mutex::new(Default::default()),
            asset_server: std::sync::OnceLock::new(),
        }
    }

    /// Late-bind the AssetServer. Called from `start_server` once the
    /// HTTP listener wins a port slot (which happens AFTER this service
    /// is constructed and Arc-wrapped). Subsequent calls are no-ops —
    /// the service holds at most one AssetServer instance for its
    /// lifetime.
    pub fn set_asset_server(&self, server: crate::asset_server::AssetServer) {
        let _ = self.asset_server.set(server);
    }

    pub fn asset_server(&self) -> Option<&crate::asset_server::AssetServer> {
        self.asset_server.get()
    }

    /// Builder: attach an EventBus so emit_* methods publish progress and
    /// terminal events on `jobs.render.{job_id}`.
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Builder: attach the shared artifact storage (used by T6 to persist compositions).
    pub fn with_storage(mut self, storage: Arc<Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Builder: attach the shared skill registry (used by T7 to read templates/DESIGN-template).
    pub fn with_skills(mut self, skills: Arc<RwLock<SkillRegistry>>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// Return a reference to the artifact storage, if configured.
    pub fn storage(&self) -> Option<&Arc<Storage>> {
        self.storage.as_ref()
    }

    /// Return a reference to the skill registry, if configured.
    pub fn skills(&self) -> Option<&Arc<RwLock<SkillRegistry>>> {
        self.skills.as_ref()
    }

    pub fn bridge_is_stub(&self) -> bool {
        self.bridge_stub
    }

    pub fn jobs(&self) -> &JobRegistry {
        &self.jobs
    }

    // --- Composition management ---

    pub async fn create_composition(
        self: &Arc<Self>,
        req: CreateCompositionRequest,
    ) -> Result<CreateCompositionResponse> {
        create::create(self, req).await
    }

    /// Mode-3 entry: derive a DESIGN.md from a `VisualIdentity` blob (the
    /// output of `canvas_extract_visual_identity`) and create the
    /// composition with that design baseline. Composes
    /// `vi_to_design::vi_to_design_md` + `create_composition` so the LLM
    /// gets a deterministic VI → DESIGN.md path instead of having to
    /// hand-translate. Caller can still `browser_edit_artifact` afterward
    /// to apply user-specific tweaks; the deterministic baseline is the
    /// fast path.
    pub async fn create_from_visual_identity(
        self: &Arc<Self>,
        req: nevoflux_protocol::canvas_video::CreateFromVisualIdentityRequest,
    ) -> Result<CreateCompositionResponse> {
        use nevoflux_protocol::extract::VisualIdentity;

        // Deserialize the embedded VI blob. We expose it as serde_json::Value
        // in the protocol so this module doesn't have to leak `extract`
        // types into every CRUD call site, but parse strictly here.
        let vi: VisualIdentity =
            serde_json::from_value(req.visual_identity.clone()).map_err(|e| {
                crate::error::DaemonError::InvalidRequest(format!(
                    "create_from_visual_identity: visual_identity is not a valid VisualIdentity: {e}"
                ))
            })?;

        let design_md = crate::canvas_video::vi_to_design::vi_to_design_md(&vi);

        let create_req = CreateCompositionRequest {
            title: req.title,
            width: req.width,
            height: req.height,
            duration_sec: req.duration_sec,
            fps: req.fps,
            bg: req.bg,
            html: None,
            template: Some(req.template),
            design_md: Some(design_md),
            session_id: req.session_id,
        };
        create::create(self, create_req).await
    }

    /// Re-inject the composition's stored DESIGN.md tokens into its
    /// `index.html`, replacing only the `<style data-nf-design-tokens>`
    /// marked block. Use when the user has edited DESIGN.md (in the Canvas
    /// Editor or via a separate tool) and wants the brand layer refreshed
    /// without losing content/copy edits to the composition body.
    ///
    /// Idempotent: running it twice with no DESIGN.md change leaves the
    /// artifact byte-identical. Returns `Ok(())` on success;
    /// `InvalidRequest` if the composition is missing or has no
    /// `DESIGN.md` / `index.html` entries in its multi-file artifact.
    pub async fn apply_design_md(&self, composition_id: &str) -> Result<()> {
        use nevoflux_storage::repositories::ArtifactRepository;

        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| DaemonError::InternalError("canvas_video: storage not wired".into()))?;
        let repo = ArtifactRepository::new(storage.database());
        let record = repo
            .get(composition_id)
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!("composition not found: {composition_id}"))
            })?;
        let mut files = record.files.ok_or_else(|| {
            DaemonError::InvalidRequest(format!(
                "composition has no multi-file payload: {composition_id}"
            ))
        })?;
        let design_md = files.get("DESIGN.md").cloned().ok_or_else(|| {
            DaemonError::InvalidRequest(format!("composition has no DESIGN.md: {composition_id}"))
        })?;
        let index_html = files.get("index.html").cloned().ok_or_else(|| {
            DaemonError::InvalidRequest(format!("composition has no index.html: {composition_id}"))
        })?;
        // Diagnostic: log what apply_design_md actually read from SQLite so
        // we can tell whether ContentStore mirror writes are visible here.
        let in_idx_has_brand = index_html.contains("全新 GPT 体验");
        let in_design_has_orange = design_md.contains("#ff6600");
        tracing::info!(
            "apply_design_md READ: id={}, index_html_len={}, design_md_len={}, idx_has_new_brand={}, design_has_ff6600={}",
            composition_id, index_html.len(), design_md.len(), in_idx_has_brand, in_design_has_orange,
        );
        let updated_html =
            crate::canvas_video::design::inject_design_tokens(&index_html, &design_md)?;
        let out_idx_has_brand = updated_html.contains("全新 GPT 体验");
        let out_has_orange_token = updated_html.contains("--color-primary: #ff6600");
        tracing::info!(
            "apply_design_md WROTE: id={}, updated_html_len={}, idx_has_new_brand={}, has_orange_token={}",
            composition_id, updated_html.len(), out_idx_has_brand, out_has_orange_token,
        );
        files.insert("index.html".to_string(), updated_html.clone());
        repo.update_files(composition_id, &files, &updated_html)
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?;
        Ok(())
    }

    /// Write an asset blob into the composition's dedicated
    /// `composition_assets` table. Returns the canonical path the agent
    /// should reference in HTML (`assets/<sanitized-name>`).
    ///
    /// Storage moved out of `artifacts.files` (migration 016): assets are
    /// raw BLOBs in their own table now, so `files` is text-only and
    /// ContentStore mirroring no longer needs a defensive `assets/*`
    /// merge. See migration 016 / commit "feat(storage): composition_assets".
    pub async fn attach_asset(
        &self,
        composition_id: &str,
        name: &str,
        mime_type: &str,
        payload_b64: &str,
        size_bytes: u64,
    ) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use nevoflux_storage::repositories::{ArtifactRepository, CompositionAssetRepository};

        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| DaemonError::InternalError("canvas_video: storage not wired".into()))?;
        let repo = ArtifactRepository::new(storage.database());
        let record = repo
            .get(composition_id)
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!("composition not found: {composition_id}"))
            })?;
        let files = record.files.unwrap_or_default();

        // Read stage dims from composition.meta.json so we know how big
        // a bitmap actually has to be. Falls back to a generous 1920×1920
        // box if the meta is missing/malformed — that's the v1 hard limit
        // upper bound, so resize is still effective.
        let (stage_w, stage_h) = read_stage_dims(&files).unwrap_or((1920, 1920));

        // Decode incoming base64 to raw bytes for the resize path. The
        // bytes branch of asset_resize avoids the encode/decode round-trip
        // we used to pay before assets moved out of artifacts.files.
        let raw_bytes = STANDARD.decode(payload_b64.as_bytes()).map_err(|e| {
            DaemonError::InvalidRequest(format!("canvas_attach_asset: payload not valid base64: {e}"))
        })?;

        // Downscale oversized images BEFORE storage. Without this, a
        // 4000×6000 hero JPEG would burn render-time wall clock for no
        // visual gain. The resize is best-effort: any non-image branch
        // returns the original bytes verbatim.
        let (resized_bytes, resize_outcome) =
            super::asset_resize::maybe_resize_bytes(&raw_bytes, stage_w, stage_h);
        let final_bytes: Vec<u8> = match &resize_outcome {
            super::asset_resize::ResizeOutcome::Resized { .. } => resized_bytes,
            // Non-Resized outcomes return an empty bytes vec by contract;
            // the caller's original bytes are still authoritative.
            _ => raw_bytes,
        };
        let final_size_bytes = match &resize_outcome {
            super::asset_resize::ResizeOutcome::Resized { new_bytes, .. } => *new_bytes as u64,
            _ => size_bytes,
        };

        // Preserve the caller-supplied basename verbatim. Even when the
        // agent mis-extensions (saves a JPEG as foo.png), we keep the
        // basename so any `<img src="assets/foo.png">` references the
        // agent already wrote into HTML still resolve. Magic-byte sniff
        // at HTTP serve time selects the correct `Content-Type` regardless.
        let asset_name = sanitize_asset_name(name);
        let path = format!("assets/{asset_name}");

        // Persist into the dedicated table. Idempotent — repeat attaches
        // overwrite (matches the previous `files.insert` semantic).
        let asset_repo = CompositionAssetRepository::new(storage.database());
        asset_repo
            .upsert(composition_id, &asset_name, &final_bytes, Some(mime_type))
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?;

        tracing::info!(
            "canvas_attach_asset: id={composition_id} path={path} mime={mime_type} \
             original_bytes={size_bytes} stored_bytes={final_size_bytes} \
             stage={stage_w}x{stage_h} resize_outcome={resize_outcome:?}"
        );
        Ok(path)
    }

    /// Load a composition from the artifact repository and return (html, w, h, d, fps).
    pub async fn load_composition(
        &self,
        composition_id: &str,
    ) -> Result<(String, u32, u32, f32, u32)> {
        use nevoflux_protocol::canvas_video::CompositionMeta;
        use nevoflux_storage::repositories::ArtifactRepository;

        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| DaemonError::InternalError("canvas_video: storage not wired".into()))?;
        let repo = ArtifactRepository::new(storage.database());
        let rec = repo
            .get(composition_id)
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!("composition not found: {composition_id}"))
            })?;

        let files = rec
            .files
            .as_ref()
            .ok_or_else(|| DaemonError::InvalidComposition {
                reason: "no files map (not a composition artifact)".into(),
            })?;

        let meta_raw =
            files
                .get("composition.meta.json")
                .ok_or_else(|| DaemonError::InvalidComposition {
                    reason: "missing composition.meta.json".into(),
                })?;
        let meta: CompositionMeta =
            serde_json::from_str(meta_raw).map_err(|e| DaemonError::InvalidComposition {
                reason: format!("malformed meta: {e}"),
            })?;
        meta.validate_hard_limits()
            .map_err(|e| DaemonError::InvalidComposition {
                reason: format!("{e}"),
            })?;

        let entry = rec.entry.as_deref().unwrap_or("index.html");
        let html_raw =
            files
                .get(entry)
                .cloned()
                .ok_or_else(|| DaemonError::InvalidComposition {
                    reason: format!("entry file missing: {entry}"),
                })?;

        // Phase 2: rewrite `assets/X` to absolute Asset Plane URLs when an
        // AssetServer is wired (production path). Asset names come from
        // the dedicated `composition_assets` table (migration 016) — no
        // longer interleaved with text files in the JSON map.
        //
        // When no AssetServer is wired (most unit tests), refs stay
        // relative — callers of `load_composition` in those contexts
        // (lint structural checks, fixture round-trips) don't need
        // assets to actually resolve.
        let html = match self.asset_server.get() {
            Some(asset_server) => {
                use nevoflux_storage::repositories::CompositionAssetRepository;
                let asset_repo = CompositionAssetRepository::new(storage.database());
                let asset_names = asset_repo
                    .list_names(composition_id)
                    .map_err(|e| DaemonError::InternalError(format!("{e}")))?;
                if asset_names.is_empty() {
                    html_raw
                } else {
                    let urls = asset_server.register_composition_assets(
                        composition_id,
                        &asset_names,
                        crate::asset_server::COMPOSITION_TOKEN_TTL,
                    );
                    super::asset_inline::rewrite_assets_to_urls(&html_raw, &urls)
                }
            }
            None => html_raw,
        };
        Ok((
            html,
            meta.spec.width,
            meta.spec.height,
            meta.spec.duration_sec,
            meta.spec.fps,
        ))
    }

    /// Fetch just the title of a composition artifact (without loading the HTML).
    /// Returns "composition" as a generic fallback if the row is missing or the
    /// title is empty — the caller uses this for filename construction and
    /// should never fail hard on a missing title.
    pub async fn load_composition_title(&self, composition_id: &str) -> Result<String> {
        use nevoflux_storage::repositories::ArtifactRepository;
        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| DaemonError::InternalError("canvas_video: storage not wired".into()))?;
        let repo = ArtifactRepository::new(storage.database());
        let rec = repo
            .get(composition_id)
            .map_err(|e| DaemonError::InternalError(format!("{e}")))?
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!("composition not found: {composition_id}"))
            })?;
        let t = rec.title.trim();
        Ok(if t.is_empty() {
            "composition".to_string()
        } else {
            t.to_string()
        })
    }

    pub async fn render_start(
        self: &Arc<Self>,
        req: RenderStartRequest,
    ) -> Result<RenderStartResponse> {
        render::render_start(self, req).await
    }

    pub async fn job_snapshot(&self, job_id: &str) -> Option<JobSnapshot> {
        self.jobs.snapshot(job_id).await
    }

    // --- Bridge coordination helpers ---

    /// Register the render loop's signal channel for this job. Returns the
    /// receiver for the loop to consume. Overwrites any previous sender for
    /// the same job_id (callers must not double-register).
    pub async fn register_job_signal_channel(
        &self,
        job_id: &str,
    ) -> mpsc::UnboundedReceiver<FrameSignal> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.signal_senders
            .lock()
            .await
            .insert(job_id.to_string(), tx);
        // Pre-create the chunk buffer too so chunks arriving before the
        // render loop finishes setup don't race.
        self.chunk_buffers
            .lock()
            .await
            .entry(job_id.to_string())
            .or_insert_with(ChunkBuffer::new);
        rx
    }

    /// Called when the render loop exits, to free per-job state.
    pub async fn cleanup_job_channels(&self, job_id: &str) {
        self.signal_senders.lock().await.remove(job_id);
        self.chunk_buffers.lock().await.remove(job_id);
        // Drop the SSE broadcast sender too — its receivers (any open
        // SSE streams) will then see RecvError::Closed and end cleanly.
        self.render_controls.lock().await.remove(job_id);
    }

    /// Called when canvas_video_frame_chunk arrives from extension.
    /// Accumulates chunks; pushes a `FrameSignal::Frame` into the job's signal
    /// channel once a frame is fully assembled.
    pub async fn on_frame_chunk(&self, chunk: RenderFrameChunk) -> Result<()> {
        let complete = {
            let mut bufs = self.chunk_buffers.lock().await;
            let buf = bufs
                .entry(chunk.job_id.clone())
                .or_insert_with(ChunkBuffer::new);
            buf.add_chunk(
                chunk.frame_idx,
                chunk.chunk_idx,
                chunk.total_chunks,
                chunk.is_last,
                chunk.bytes,
            )
        };
        if let Some(png) = complete {
            self.send_signal(
                &chunk.job_id,
                FrameSignal::Frame {
                    frame_idx: chunk.frame_idx,
                    png,
                },
            )
            .await;
        }
        Ok(())
    }

    /// Called when canvas_video_render_done arrives from extension (page-driven
    /// completion signal).
    pub async fn on_render_done(&self, job_id: &str, frames_emitted: u32) {
        self.send_signal(job_id, FrameSignal::Done { frames_emitted })
            .await;
    }

    /// Called when canvas_video_render_failed arrives from extension.
    pub async fn on_render_failed(&self, job_id: &str, error: &str) {
        self.send_signal(job_id, FrameSignal::Failed(error.to_string()))
            .await;
    }

    /// Phase 4 entry point: HTTP frame POST handler delivers a captured
    /// frame straight to the render loop without going through the
    /// chunk-reassembly path. The render loop can't tell the transport
    /// apart — same `FrameSignal::Frame` variant either way.
    pub async fn deliver_render_frame(&self, job_id: &str, signal: FrameSignal) {
        self.send_signal(job_id, signal).await;
    }

    /// Phase 4 SSE handler subscribes here on connect. The broadcast
    /// channel is lazily created on first subscription and lives until
    /// the job is cleaned up via `cleanup_job_channels`. Capacity 32
    /// is generous given control events are rare (cancel + seek only).
    pub async fn subscribe_render_control(
        &self,
        job_id: &str,
    ) -> tokio::sync::broadcast::Receiver<RenderControlEvent> {
        let mut guard = self.render_controls.lock().await;
        let tx = guard.entry(job_id.to_string()).or_insert_with(|| {
            tokio::sync::broadcast::channel::<RenderControlEvent>(32).0
        });
        tx.subscribe()
    }

    /// Broadcast a control event to whatever render-tab SSE receivers
    /// are subscribed for `job_id`. No-op if the channel doesn't exist
    /// yet (no SSE subscriber has connected) — the render loop's NM
    /// fallback for cancel still works via `JobRegistry::cancel`.
    pub async fn broadcast_render_control(&self, job_id: &str, event: RenderControlEvent) {
        if let Some(tx) = self.render_controls.lock().await.get(job_id) {
            // Send is best-effort — `Err` only fires when no receivers
            // are alive, which is OK (drop the event).
            let _ = tx.send(event);
        }
    }


    async fn send_signal(&self, job_id: &str, sig: FrameSignal) {
        if let Some(tx) = self.signal_senders.lock().await.get(job_id) {
            // Channel closed means the render loop already exited; dropping
            // the signal is the correct behavior.
            let _ = tx.send(sig);
        }
    }

    /// Return composition_id for a job (used by callers that want metadata).
    pub async fn composition_id_for(&self, job_id: &str) -> String {
        self.jobs
            .snapshot(job_id)
            .await
            .map(|s| s.composition_id)
            .unwrap_or_default()
    }

    /// Fetch the composition HTML + spec that the render page needs to
    /// draw the given job. Returns InvalidRequest if the job or its
    /// composition is unknown.
    pub async fn get_composition_for_job(
        &self,
        job_id: &str,
    ) -> Result<(String, u32, u32, f32, u32)> {
        let snap = self
            .jobs
            .snapshot(job_id)
            .await
            .ok_or_else(|| DaemonError::InvalidRequest(format!("job not found: {job_id}")))?;
        self.load_composition(&snap.composition_id).await
    }

    // --- Lint correlator plumbing ---

    /// Request a lint pass for the given composition.
    ///
    /// Publishes a lint request event on the EventBus (if wired) and awaits
    /// a `LintReport` from the extension via `on_lint_result`. Times out
    /// after 5 seconds if no resolver calls `on_lint_result`.
    pub async fn lint_composition(self: &Arc<Self>, composition_id: &str) -> Result<LintReport> {
        let (html, _w, _h, _d, _fps) = self.load_composition(composition_id).await?;
        let correlator = uuid::Uuid::new_v4().simple().to_string();
        let (tx, rx) = oneshot::channel();
        self.lint_correlators
            .lock()
            .await
            .insert(correlator.clone(), tx);

        // Publish the lint request on the EventBus so Task 12 (TCP server glue)
        // can pick it up and forward to the extension. Skip in test mode where
        // event_bus is None; tests resolve correlators directly via on_lint_result.
        if let Some(bus) = &self.event_bus {
            let topic = format!("jobs:lint:request:{correlator}");
            let payload = serde_json::json!({
                "event": "lint_request",
                "job_correlator": correlator,
                "composition_id": composition_id,
                "composition_html": html,
            });
            let event = BusEvent::ephemeral(topic, payload, PublisherIdentity::Internal);
            let _ = bus.publish(event).await;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(report)) => Ok(report),
            Ok(Err(_)) => {
                // Sender was dropped before we received a result.
                self.lint_correlators.lock().await.remove(&correlator);
                Err(DaemonError::InternalError(
                    "lint oneshot dropped before resolve".into(),
                ))
            }
            Err(_) => {
                // Timeout: remove the dangling correlator entry.
                self.lint_correlators.lock().await.remove(&correlator);
                Err(DaemonError::LintTimeout {
                    composition_id: composition_id.to_string(),
                })
            }
        }
    }

    /// Called when the extension delivers a lint result for a pending correlator.
    ///
    /// Resolves the oneshot receiver held by `lint_composition`. If the
    /// correlator is unknown (e.g. the caller already timed out), logs a
    /// warning and does nothing.
    pub async fn on_lint_result(&self, correlator: &str, report: LintReport) {
        if let Some(tx) = self.lint_correlators.lock().await.remove(correlator) {
            let _ = tx.send(report);
        } else {
            tracing::warn!(
                correlator,
                "lint result for unknown correlator — already timed out or duplicate"
            );
        }
    }

    /// Returns the first pending lint correlator key, for use in tests that
    /// need to simulate an extension response without a real EventBus.
    #[cfg(test)]
    pub async fn peek_pending_lint_correlator(&self) -> Option<String> {
        self.lint_correlators.lock().await.keys().next().cloned()
    }

    // --- Inspect correlator plumbing ---

    /// Request a layout/contrast audit for the given composition.
    ///
    /// Mirrors `lint_composition` exactly: publishes a request event on
    /// the EventBus, awaits an `InspectReport` from the extension via
    /// `on_inspect_result`. Times out after 15 seconds (longer than lint
    /// because the extension has to drive the timeline through every
    /// sample frame).
    pub async fn inspect_layout(
        self: &Arc<Self>,
        composition_id: &str,
        frames: u32,
        at: &[f32],
    ) -> Result<InspectReport> {
        let (html, w, h, _d, _fps) = self.load_composition(composition_id).await?;
        let correlator = uuid::Uuid::new_v4().simple().to_string();
        let (tx, rx) = oneshot::channel();
        self.inspect_correlators
            .lock()
            .await
            .insert(correlator.clone(), tx);

        if let Some(bus) = &self.event_bus {
            let topic = format!("jobs:inspect:request:{correlator}");
            let payload = serde_json::json!({
                "event": "inspect_request",
                "job_correlator": correlator,
                "composition_id": composition_id,
                "composition_html": html,
                "stage_w": w,
                "stage_h": h,
                "frames": frames,
                "at": at,
            });
            let event = BusEvent::ephemeral(topic, payload, PublisherIdentity::Internal);
            let _ = bus.publish(event).await;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
            Ok(Ok(report)) => Ok(report),
            Ok(Err(_)) => {
                self.inspect_correlators.lock().await.remove(&correlator);
                Err(DaemonError::InternalError(
                    "inspect oneshot dropped before resolve".into(),
                ))
            }
            Err(_) => {
                self.inspect_correlators.lock().await.remove(&correlator);
                Err(DaemonError::InternalError(format!(
                    "inspect timeout for composition {composition_id}"
                )))
            }
        }
    }

    /// Called when the extension delivers an inspect result.
    pub async fn on_inspect_result(&self, correlator: &str, report: InspectReport) {
        if let Some(tx) = self.inspect_correlators.lock().await.remove(correlator) {
            let _ = tx.send(report);
        } else {
            tracing::warn!(
                correlator,
                "inspect result for unknown correlator — already timed out or duplicate"
            );
        }
    }

    #[cfg(test)]
    pub async fn peek_pending_inspect_correlator(&self) -> Option<String> {
        self.inspect_correlators.lock().await.keys().next().cloned()
    }

    // --- EventBus emitters ---

    async fn emit(&self, job_id: &str, payload: serde_json::Value) {
        if let Some(bus) = &self.event_bus {
            let topic = format!("jobs:render:{}", job_id);
            let event_kind = payload
                .get("event")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let event = BusEvent::ephemeral(topic.clone(), payload, PublisherIdentity::Internal);
            match bus.publish(event).await {
                Ok(_) => tracing::info!(
                    topic = %topic,
                    event = %event_kind,
                    "canvas_video emit publish ok"
                ),
                Err(e) => tracing::warn!(
                    topic = %topic,
                    event = %event_kind,
                    error = %e,
                    "canvas_video emit publish FAILED"
                ),
            }
        } else {
            tracing::warn!(
                job_id = %job_id,
                "canvas_video emit called but event_bus is None — service not wired"
            );
        }
    }

    pub async fn emit_progress(&self, job_id: &str, current: u32, total: u32) {
        if !should_emit_progress(current, total) {
            return;
        }
        self.emit(
            job_id,
            serde_json::json!({
                "event": "progress",
                "job_id": job_id,
                "current": current,
                "total": total,
            }),
        )
        .await;
    }

    pub async fn emit_succeeded(&self, job_id: &str, path: &str, size_bytes: u64) {
        self.emit(
            job_id,
            serde_json::json!({
                "event": "succeeded",
                "job_id": job_id,
                "output_path": path,
                "size_bytes": size_bytes,
            }),
        )
        .await;
    }

    pub async fn emit_failed(&self, job_id: &str, error: &str) {
        self.emit(
            job_id,
            serde_json::json!({
                "event": "failed",
                "job_id": job_id,
                "error": error,
            }),
        )
        .await;
    }

    pub async fn emit_cancelled(&self, job_id: &str, current: u32, total: u32) {
        self.emit(
            job_id,
            serde_json::json!({
                "event": "cancelled",
                "job_id": job_id,
                "current": current,
                "total": total,
            }),
        )
        .await;
    }
}

impl Default for CanvasVideoService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod p3_load_tests {
    use super::*;
    use nevoflux_protocol::canvas_video::CreateCompositionRequest;
    use nevoflux_storage::{repositories::ArtifactRepository, CreateArtifactParams};

    fn req() -> CreateCompositionRequest {
        CreateCompositionRequest {
            title: "t".into(),
            width: 640,
            height: 360,
            duration_sec: 5.0,
            fps: 30,
            bg: None,
            // resolve_index_html now rejects creates with neither template nor
            // html, so seed a minimal html that's still detectable by the
            // load test below (contains the stage marker).
            html: Some(
                "<html><body><div id=\"stage\" data-width=\"640\" data-height=\"360\" \
                 data-duration=\"5\" data-fps=\"30\"></div></body></html>"
                    .into(),
            ),
            template: None,
            design_md: None,
            session_id: None,
        }
    }

    #[tokio::test]
    async fn load_reads_composition_written_by_create() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let resp = svc.create_composition(req()).await.unwrap();
        let (html, w, h, d, fps) = svc.load_composition(&resp.artifact_id).await.unwrap();
        assert!(html.contains("stage"));
        assert_eq!((w, h, fps), (640, 360, 30));
        assert!((d - 5.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn load_rejects_non_composition_artifact() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        repo.create(CreateArtifactParams {
            id: "art-plain".into(),
            session_id: None,
            title: "plain".into(),
            description: None,
            content_type: "text/html".into(),
            content: "<p>hi</p>".into(),
            files: None,
            entry: None,
        })
        .unwrap();
        let err = svc.load_composition("art-plain").await.unwrap_err();
        assert!(
            format!("{err}").contains("invalid composition"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn load_rejects_malformed_meta() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let mut files = std::collections::HashMap::new();
        files.insert("index.html".into(), "<body>ok</body>".into());
        files.insert("composition.meta.json".into(), "not-json".into());
        repo.create(CreateArtifactParams {
            id: "comp-bad".into(),
            session_id: None,
            title: "bad".into(),
            description: None,
            content_type: "text/html".into(),
            content: "<body>ok</body>".into(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();
        let err = svc.load_composition("comp-bad").await.unwrap_err();
        assert!(format!("{err}").contains("malformed"), "got {err}");
    }

    #[tokio::test]
    async fn load_rejects_spec_out_of_bounds() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let bad_meta = serde_json::json!({
            "kind": "composition", "version": 1,
            "spec": {"width": 640, "height": 360, "duration_sec": 5.0, "fps": 60},
            "origin": {"created_with": "t", "created_at": 0}
        });
        let mut files = std::collections::HashMap::new();
        files.insert("index.html".into(), "<body>ok</body>".into());
        files.insert("composition.meta.json".into(), bad_meta.to_string());
        repo.create(CreateArtifactParams {
            id: "comp-bad-fps".into(),
            session_id: None,
            title: "bad".into(),
            description: None,
            content_type: "text/html".into(),
            content: "<body>ok</body>".into(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();
        let err = svc.load_composition("comp-bad-fps").await.unwrap_err();
        assert!(
            format!("{err}").contains("60") || format!("{err}").contains("fps"),
            "got {err}"
        );
    }

    // Post-migration 015: the storage layer guarantees `files[entry]`
    // exists at create time — when the caller passes a `files` map missing
    // the entry key, `ArtifactRepository::create` synthesizes the entry
    // from `params.content` (often empty for tests). The "missing entry"
    // failure path tested previously is no longer reachable through the
    // storage API; the corresponding load_rejects_missing_entry_file test
    // was removed when this invariant landed. If you reintroduce the
    // failure mode (e.g. via raw SQL bypass), reinstate the test.

    // ---------------------------------------------------------------------
    // Phase 2: load_composition emits URL-rewritten HTML (no data: URIs)
    // when an AssetServer is wired. With no AssetServer, refs stay relative.
    // ---------------------------------------------------------------------

    /// 1×1 transparent PNG, base64-encoded — matches what the agent stores
    /// for binary assets (canvas_attach_asset path).
    const PNG_1X1_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    fn composition_meta_json() -> String {
        serde_json::json!({
            "kind": "composition", "version": 1,
            "spec": {"width": 640, "height": 360, "duration_sec": 5.0, "fps": 30},
            "origin": {"created_with": "phase2-test", "created_at": 0}
        })
        .to_string()
    }

    fn write_fixture(svc: &CanvasVideoService, id: &str) {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use nevoflux_storage::repositories::CompositionAssetRepository;

        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        // Post-migration-016 shape: text in artifacts.files, binary in
        // composition_assets.
        let mut files = std::collections::HashMap::new();
        files.insert(
            "index.html".into(),
            r#"<html><body><img src="assets/hero.png"></body></html>"#.to_string(),
        );
        files.insert("composition.meta.json".into(), composition_meta_json());
        repo.create(CreateArtifactParams {
            id: id.into(),
            session_id: None,
            title: "phase2-fixture".into(),
            description: None,
            content_type: "text/html".into(),
            content: files["index.html"].clone(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();

        let png_bytes = STANDARD.decode(PNG_1X1_B64.as_bytes()).unwrap();
        CompositionAssetRepository::new(storage.database())
            .upsert(id, "hero.png", &png_bytes, Some("image/png"))
            .unwrap();
    }

    #[tokio::test]
    async fn get_composition_returns_rewritten_urls_not_data_uris() {
        // Boot a real AssetServer backed by the same Storage the
        // CanvasVideoService writes to, then assert the response HTML
        // never contains an inlined data: URI.
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let id = "comp-phase2-rewrite";
        write_fixture(&svc, id);

        // `Database` is `#[derive(Clone)]` and shares the same
        // `Arc<Mutex<Connection>>` internally — cloning gives the
        // AssetServer a handle to the same in-memory SQLite the
        // CanvasVideoService writes to.
        let db_arc = std::sync::Arc::new(svc.storage().unwrap().database().clone());
        let server = crate::asset_server::AssetServer::start(
            crate::asset_server::AssetServerConfig {
                bearer_token: "phase2-bearer".into(),
                session_id: "phase2-session".into(),
                storage: Some(db_arc),
                ..Default::default()
            },
        )
        .await
        .expect("AssetServer should boot for phase2 test");
        svc.set_asset_server(server.clone());

        let (html, _w, _h, _d, _fps) = svc.load_composition(id).await.unwrap();
        assert!(
            !html.contains("data:image"),
            "Phase 2 contract: load_composition MUST NOT inline data: URIs.\n got: {html}"
        );
        assert!(
            html.contains("/v1/asset/composition/comp-phase2-rewrite/hero.png?t="),
            "expected rewritten Asset Plane URL.\n got: {html}"
        );
    }

    #[tokio::test]
    async fn stored_index_html_keeps_relative_refs() {
        // C1 invariant: SQLite always holds the agent-friendly relative
        // form, never the rewritten URL form. (The rewriting is a
        // GET-time transform on the way out of `load_composition`.)
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let id = "comp-phase2-storage";
        write_fixture(&svc, id);

        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let rec = repo.get(id).unwrap().unwrap();

        let stored_html = &rec.files.unwrap()["index.html"];
        assert!(
            stored_html.contains(r#"src="assets/hero.png""#),
            "stored HTML must keep `assets/X` relative refs.\n got: {stored_html}"
        );
        assert!(
            !stored_html.contains("data:image"),
            "stored HTML must NEVER contain inlined data URIs.\n got: {stored_html}"
        );
        assert!(
            !stored_html.contains("/v1/asset/composition/"),
            "stored HTML must NEVER contain rewritten asset-plane URLs.\n got: {stored_html}"
        );
    }

    #[tokio::test]
    async fn load_composition_without_asset_server_keeps_relative_refs() {
        // Test-mode boot has no AssetServer; load_composition returns the
        // raw stored HTML untouched (no inlining, no rewriting). This is
        // the unit-test path — production wires set_asset_server during
        // start_server.
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let id = "comp-phase2-no-server";
        write_fixture(&svc, id);

        let (html, _w, _h, _d, _fps) = svc.load_composition(id).await.unwrap();
        assert!(
            html.contains(r#"src="assets/hero.png""#),
            "without AssetServer, refs must stay relative.\n got: {html}"
        );
        assert!(!html.contains("data:image"), "no inlining either: {html}");
    }

    /// The motivating defect for Phase 2 was that compositions with
    /// realistic binary assets blew the NM 1 MB cap when `load_composition`
    /// inlined them as data URIs. Plant a 2 MB asset and assert the
    /// returned HTML is now KB-class (URL rewriting only) instead of MB-class.
    #[tokio::test]
    async fn load_composition_response_size_is_kb_not_mb_with_large_asset() {
        use nevoflux_storage::repositories::CompositionAssetRepository;

        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let id = "comp-phase2-bigasset";

        // 2 MiB raw bytes — large enough that the OLD inlining path
        // would have produced ~2.7 MiB HTML (4/3 base64 expansion).
        let mut blob = Vec::with_capacity(2 * 1024 * 1024);
        blob.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        for i in 0..(2 * 1024 * 1024) {
            blob.push((i as u8).wrapping_mul(31));
        }

        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let mut files = std::collections::HashMap::new();
        files.insert(
            "index.html".into(),
            r#"<html><body><img src="assets/big.png"></body></html>"#.to_string(),
        );
        files.insert("composition.meta.json".into(), composition_meta_json());
        repo.create(CreateArtifactParams {
            id: id.into(),
            session_id: None,
            title: "big".into(),
            description: None,
            content_type: "text/html".into(),
            content: files["index.html"].clone(),
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();
        // Asset goes in the dedicated table, raw bytes (no base64 in the
        // store now).
        CompositionAssetRepository::new(storage.database())
            .upsert(id, "big.png", &blob, Some("image/png"))
            .unwrap();

        let db_arc = std::sync::Arc::new(svc.storage().unwrap().database().clone());
        let server = crate::asset_server::AssetServer::start(
            crate::asset_server::AssetServerConfig {
                bearer_token: "phase2-bearer".into(),
                session_id: "phase2-session".into(),
                storage: Some(db_arc),
                ..Default::default()
            },
        )
        .await
        .expect("AssetServer should boot");
        svc.set_asset_server(server.clone());

        let (html, _w, _h, _d, _fps) = svc.load_composition(id).await.unwrap();

        // Phase 2 contract: response is URL-rewritten — its size is
        // bounded by the entry HTML + a per-asset URL (~150 B). 2 KB is
        // a generous ceiling for this fixture; the OLD inlining path
        // would have produced ~2.7 MiB (4/3 base64 expansion of 2 MiB).
        assert!(
            html.len() < 2_000,
            "response must stay KB-class (got {} bytes)",
            html.len()
        );
        assert!(!html.contains("data:image"));
        assert!(html.contains("/v1/asset/composition/"));
    }

    /// End-to-end: take the URL `load_composition` rewrote into the HTML,
    /// fetch it via reqwest from the running AssetServer, and assert the
    /// bytes round-trip back to the stored asset. Proves the GET route
    /// resolves with a real composition_token without needing a browser
    /// in the loop.
    #[tokio::test]
    async fn rewritten_asset_url_round_trips_via_real_http_fetch() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let id = "comp-phase2-roundtrip";
        write_fixture(&svc, id);

        let db_arc = std::sync::Arc::new(svc.storage().unwrap().database().clone());
        let server = crate::asset_server::AssetServer::start(
            crate::asset_server::AssetServerConfig {
                bearer_token: "phase2-bearer".into(),
                session_id: "phase2-session".into(),
                storage: Some(db_arc),
                ..Default::default()
            },
        )
        .await
        .expect("AssetServer should boot");
        svc.set_asset_server(server.clone());

        let (html, _w, _h, _d, _fps) = svc.load_composition(id).await.unwrap();

        // Pull the rewritten URL out of the HTML and fetch it. The token
        // in `?t=...` is what `register_composition_assets` issued.
        let prefix = "http://127.0.0.1:";
        let start = html
            .find(prefix)
            .expect("rewritten URL must appear in HTML");
        let end = html[start..].find('"').expect("URL must end at the quote");
        let url = &html[start..start + end];

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client.get(url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let bytes = resp.bytes().await.unwrap();
        // First 8 bytes match the PNG signature — proves the asset GET
        // handler decoded the base64 entry into raw bytes.
        assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}

#[cfg(test)]
mod attach_helper_tests {
    use super::{magic_bytes_extension, override_extension, sanitize_asset_name};
    use base64::{engine::general_purpose::STANDARD, Engine};

    #[test]
    fn jpg_magic_detected() {
        // FF D8 FF E0 — JPEG/JFIF
        let bytes = [0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(magic_bytes_extension(&b64), Some("jpg"));
    }

    #[test]
    fn png_magic_detected() {
        let bytes = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(magic_bytes_extension(&b64), Some("png"));
    }

    #[test]
    fn gif_magic_detected() {
        let bytes = [0x47u8, 0x49, 0x46, 0x38, 0x39, b'a'];
        let b64 = STANDARD.encode(bytes);
        assert_eq!(magic_bytes_extension(&b64), Some("gif"));
    }

    #[test]
    fn unknown_magic_returns_none() {
        let bytes = b"hello world XYZ";
        let b64 = STANDARD.encode(bytes);
        assert_eq!(magic_bytes_extension(&b64), None);
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(magic_bytes_extension(""), None);
        assert_eq!(magic_bytes_extension("AA"), None);
    }

    #[test]
    fn override_extension_replaces() {
        assert_eq!(override_extension("foo.png", "jpg"), "foo.jpg");
        assert_eq!(override_extension("kitty.bin", "png"), "kitty.png");
    }

    #[test]
    fn override_extension_appends_when_no_dot() {
        assert_eq!(override_extension("foo", "jpg"), "foo.jpg");
    }

    #[test]
    fn sanitize_drops_traversal() {
        assert_eq!(sanitize_asset_name("../etc/passwd"), "passwd");
        assert_eq!(sanitize_asset_name("/abs/path/foo.png"), "foo.png");
    }
}

#[cfg(test)]
mod p3_deps_tests {
    use super::*;
    use nevoflux_skills::SkillRegistry;
    use nevoflux_storage::Storage;
    use std::sync::Arc;
    use tokio::sync::RwLock as TokioRwLock;

    #[tokio::test]
    async fn new_for_tests_has_storage_and_skills() {
        let svc = CanvasVideoService::new_for_tests();
        assert!(svc.storage().is_some(), "test service must carry Storage");
        assert!(
            svc.skills().is_some(),
            "test service must carry SkillRegistry"
        );
    }

    #[tokio::test]
    async fn builders_install_storage_and_skills() {
        let storage = Arc::new(Storage::open_in_memory().expect("new Storage"));
        let skills = Arc::new(TokioRwLock::new(SkillRegistry::new()));
        let svc = CanvasVideoService::new()
            .with_storage(storage.clone())
            .with_skills(skills.clone());
        assert!(Arc::ptr_eq(svc.storage().unwrap(), &storage));
        assert!(Arc::ptr_eq(svc.skills().unwrap(), &skills));
    }
}

#[cfg(test)]
mod p2_emit_tests {
    use super::*;
    use crate::event_bus::{BackpressurePolicy, EventBus, SubscriberIdentity};
    use std::sync::Arc;

    #[tokio::test]
    async fn emit_cancelled_publishes_event_with_frame_counts() {
        let bus = Arc::new(EventBus::new());
        let svc = Arc::new(CanvasVideoService::new_for_tests().with_event_bus(bus.clone()));

        // Subscribe BEFORE emitting so we capture the delivery.
        let pattern = crate::event_bus::types::TopicPattern::wildcard("jobs:render:*");
        let mut sub = bus
            .subscribe(
                pattern,
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropOldest,
                64,
            )
            .expect("subscribe");

        svc.emit_cancelled("job-abc", 42, 150).await;

        let delivered = tokio::time::timeout(std::time::Duration::from_millis(200), sub.rx.recv())
            .await
            .expect("timeout")
            .expect("delivery");

        assert_eq!(delivered.topic, "jobs:render:job-abc");
        let body = &delivered.payload;
        assert_eq!(body["event"], "cancelled");
        assert_eq!(body["job_id"], "job-abc");
        assert_eq!(body["current"], 42);
        assert_eq!(body["total"], 150);
    }

    /// Pure throttle decision: kept separate from the async emit path so
    /// we can exhaustively test the gating math without an EventBus.
    #[test]
    fn should_emit_progress_throttles_to_about_20_per_render() {
        // 900 frames → throttle = 45 → emits at 0, 45, …, 900 (inclusive).
        let mut n = 0;
        for current in 0..=900 {
            if super::should_emit_progress(current, 900) {
                n += 1;
            }
        }
        assert!((18..=25).contains(&n), "900→{}", n);

        // 150 frames → throttle = 7 → ~22 emits.
        let mut n = 0;
        for current in 0..=150 {
            if super::should_emit_progress(current, 150) {
                n += 1;
            }
        }
        assert!((18..=25).contains(&n), "150→{}", n);

        // total below divisor (10): throttle = 1 → emit every frame.
        let mut n = 0;
        for current in 0..=10 {
            if super::should_emit_progress(current, 10) {
                n += 1;
            }
        }
        assert_eq!(n, 11, "10→{}", n);

        // Terminal (current == total) always fires even if not on the divisor.
        assert!(super::should_emit_progress(900, 900));
        assert!(super::should_emit_progress(7, 150));
    }
}
