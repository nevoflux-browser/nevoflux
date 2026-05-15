//! Wire types for `canvas.video.*` namespace.

use serde::{Deserialize, Serialize};

/// `canvas.video.create_composition` request.
///
/// `deny_unknown_fields` is critical for the LLM-facing path: without it,
/// serde silently accepts hallucinated fields (e.g., the LLM remembering
/// `html` from earlier conversation history even after it was removed
/// from the JSON Schema). Strict deserialization causes such payloads to
/// fail loudly with InvalidRequest, forcing the agent to retry with a
/// schema-compliant call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCompositionRequest {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    #[serde(default)]
    pub bg: Option<String>,
    /// Raw HTML override. Either `html` or `template` must be supplied;
    /// when both are present, `html` wins.
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    /// Caller-supplied DESIGN.md content (Google design.md + video extension
    /// frontmatter). Drives the brand identity layer: colors, typography,
    /// spacing, motion. Daemon parses the YAML frontmatter and injects a
    /// `<style data-nf-design-tokens>:root { ... }</style>` block at the top
    /// of the composition's `<head>`. When absent, the daemon falls back to
    /// the template-specific default DESIGN.md (`templates/<name>.design.md`)
    /// and finally to `reference/DESIGN-template.md` for `html`-only requests.
    #[serde(default)]
    pub design_md: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCompositionResponse {
    pub artifact_id: String,
}

/// `canvas.video.render.start` — initiates a render job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderStartRequest {
    pub composition_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderStartResponse {
    pub job_id: String,
}

/// `canvas.video.render.cancel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCancelRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCancelResponse {
    pub cancelled: bool,
}

/// Extension -> daemon chunk push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFrameChunk {
    pub job_id: String,
    pub frame_idx: u32,
    pub chunk_idx: u32,
    pub total_chunks: u32,
    pub is_last: bool,
    pub bytes: Vec<u8>,
}

/// Ready ack from render page once composition is loaded + patched.
///
/// Retained for backward compat; the page-driven render loop (post-actor
/// rework) ignores this message — the first frame chunk implicitly signals
/// that rendering is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderReady {
    pub job_id: String,
}

/// Page -> daemon: "all frames sent, close the pipe and finalize."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderDone {
    pub job_id: String,
    /// Total frames the page actually emitted (for sanity checks).
    #[serde(default)]
    pub frames_emitted: u32,
}

/// Page -> daemon: "give me the composition HTML + spec for this job."
/// Served synchronously via `bridge:request`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCompositionRequest {
    pub job_id: String,
}

/// Asset plane endpoint advertisement carried inside
/// `GetCompositionResponse` so the render page can POST captured PNG
/// frames straight to `/v1/render/:job_id/frame` without going through
/// native messaging. `None` when the daemon has no `AssetServer` wired
/// (test mode, partial bring-up); render page MUST then fall back to
/// the legacy `canvas_video_frame_chunk` NM path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPlaneEndpoint {
    pub port: u16,
    pub bearer_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCompositionResponse {
    pub html: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub asset_plane: Option<AssetPlaneEndpoint>,
}

/// Canvas Editor / preview consumer -> daemon: "give me the URL-rewritten
/// composition HTML by id, no render job needed".
///
/// Phase 2 of the asset-stream-plane design routes binary asset bytes
/// over HTTP via `/v1/asset/composition/<id>/<name>?t=<token>`. The
/// daemon-side `load_composition` handles the rewrite when an
/// `AssetServer` is wired; this request is the bridge entry point so a
/// chrome:// page (Canvas Editor) can ask for the same HTML as the
/// render tab without needing to know the bearer token / asset plane
/// port itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadCompositionHtmlRequest {
    pub composition_id: String,
}

/// Progress event (pushed on EventBus).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProgress {
    pub job_id: String,
    pub step: String,
    pub current: u32,
    pub total: u32,
}

/// Terminal event: success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSucceeded {
    pub job_id: String,
    pub composition_id: String,
    pub output_path: String,
    pub size_bytes: u64,
    pub duration_ms: u64,
}

/// Terminal event: failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFailed {
    pub job_id: String,
    pub error: String,
}

// ─────────────────────────────────────────────────────────────────────────
// Composition metadata (P3)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompositionKind {
    Composition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionMeta {
    pub kind: CompositionKind,
    pub version: u32,
    pub spec: CompositionSpec,
    pub origin: CompositionOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionSpec {
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    #[serde(default)]
    pub bg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionOrigin {
    #[serde(default)]
    pub template: Option<String>,
    pub created_with: String,
    pub created_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum CompositionMetaError {
    #[error("invalid fps {0}: must be 24, 25, or 30")]
    InvalidFps(u32),
    #[error("invalid duration {0}: must be in [0.5, 60]")]
    InvalidDuration(f32),
    #[error("invalid dimensions {0}x{1}: both must be in [1, 1920]")]
    InvalidDimensions(u32, u32),
    #[error("version {0} not supported; expected 1")]
    UnsupportedVersion(u32),
}

impl CompositionMeta {
    pub fn validate_hard_limits(&self) -> Result<(), CompositionMetaError> {
        if self.version != 1 {
            return Err(CompositionMetaError::UnsupportedVersion(self.version));
        }
        if !matches!(self.spec.fps, 24 | 25 | 30) {
            return Err(CompositionMetaError::InvalidFps(self.spec.fps));
        }
        if !(0.5..=60.0).contains(&self.spec.duration_sec) {
            return Err(CompositionMetaError::InvalidDuration(
                self.spec.duration_sec,
            ));
        }
        if self.spec.width == 0
            || self.spec.width > 1920
            || self.spec.height == 0
            || self.spec.height > 1920
        {
            return Err(CompositionMetaError::InvalidDimensions(
                self.spec.width,
                self.spec.height,
            ));
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Lint protocol (P3)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub rule_id: String,
    pub message: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
    #[serde(default)]
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LintReport {
    #[serde(default)]
    pub errors: Vec<LintIssue>,
    #[serde(default)]
    pub warnings: Vec<LintIssue>,
    #[serde(default)]
    pub infos: Vec<LintIssue>,
    #[serde(default)]
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintCompositionRequest {
    pub composition_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintCompositionResponse {
    #[serde(flatten)]
    pub report: LintReport,
}

// ─────────────────────────────────────────────────────────────────────────
// Inspect protocol (visual layout + WCAG audit)
// ─────────────────────────────────────────────────────────────────────────

/// `canvas.video.inspect_layout` — runs the composition in an offscreen
/// iframe, samples N timestamps across the timeline, and reports visual
/// issues that the static linter cannot catch: text overflow,
/// off-stage elements, zero-size visible elements, and WCAG contrast
/// violations.
///
/// The bridge model mirrors `lint_composition`: daemon publishes a
/// request event with a correlator, extension responds with the result
/// via `canvas_video_inspect_result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectLayoutRequest {
    pub composition_id: String,
    /// How many evenly-spaced timestamps to sample across the
    /// composition. Defaults to 8. Bump to 15 for dense videos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    /// Optional explicit timestamps to additionally check (hero
    /// frames the agent suspects). Merged with the evenly-spaced set.
    #[serde(default)]
    pub at: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectIssueKind {
    /// Element extends past the stage's left/right edge.
    OverflowX,
    /// Element extends past the stage's top/bottom edge.
    OverflowY,
    /// Element entirely outside the stage rect at this timestamp.
    OffStage,
    /// `[data-track-index]` element has zero bbox during its
    /// declared `data-start..+data-duration` window.
    ZeroSize,
    /// WCAG AA contrast ratio below 4.5:1 (or 3:1 for large text).
    Contrast,
    /// Internal — couldn't measure (selector resolution failed,
    /// computed style read errored, etc.). Carries the message in
    /// `fix_hint`.
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectBbox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectIssue {
    /// Timestamp (seconds) at which the issue was observed.
    pub t: f32,
    pub kind: InspectIssueKind,
    pub selector: String,
    /// Element bounding box at the sampled timestamp; absent for
    /// `Contrast` issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<InspectBbox>,
    /// Stage size echoed back so the agent can compute relative
    /// percentages without a second tool call.
    pub stage_w: u32,
    pub stage_h: u32,
    /// For `Contrast` issues only: foreground / background hex,
    /// computed ratio, required ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<f32>,
    /// Suggested fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InspectReport {
    pub frames_checked: u32,
    pub stage_w: u32,
    pub stage_h: u32,
    #[serde(default)]
    pub issues: Vec<InspectIssue>,
    #[serde(default)]
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectLayoutResponse {
    #[serde(flatten)]
    pub report: InspectReport,
}

/// `canvas.video.apply_design_md` — re-inject DESIGN.md tokens into the
/// composition's `index.html`. Non-destructive (only the marked
/// `<style data-nf-design-tokens>` block changes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDesignMdRequest {
    pub composition_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDesignMdResponse {
    pub composition_id: String,
}

/// `canvas.video.create_from_visual_identity` — Mode 3 entry: take a
/// `VisualIdentity` (typically the output of `canvas_extract_visual_identity`)
/// + a template + composition spec, and produce a composition whose
/// DESIGN.md is auto-derived from the VI.
///
/// Equivalent to: caller renders DESIGN.md from VI in their head, then
/// calls `canvas_create_composition({ template, design_md: <rendered>, ...})`.
/// We do the rendering deterministically in the daemon so the LLM doesn't
/// have to hand-translate VI → YAML (which it does poorly: drops fields,
/// hallucinates names, mis-formats weights).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFromVisualIdentityRequest {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    /// Visual identity blob — `nevoflux_protocol::extract::VisualIdentity`.
    /// Embedded as JSON so this protocol module doesn't need to depend on
    /// the extract module's Rust types directly. The daemon deserializes
    /// it via `serde_json::from_value::<VisualIdentity>` at the dispatch
    /// boundary.
    pub visual_identity: serde_json::Value,
    pub template: String,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────
// Reveal path (UX2 — "Play" / "Open folder" buttons)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealPathRequest {
    pub path: String,
    /// "play" → open with default app; "reveal" → open containing folder.
    pub action: RevealAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RevealAction {
    Play,
    Reveal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealPathResponse {
    pub success: bool,
    /// Human-readable error summary on failure; empty on success.
    #[serde(default)]
    pub error: Option<String>,
}

/// `canvas_attach_asset` request — write an image / audio / video / font /
/// arbitrary file into the composition's files map under
/// `assets/<name>.<ext>`. The render pipeline auto-inlines those references
/// (see `canvas_video::asset_inline`), so the agent can put `<img
/// src="assets/hero.png">` in the composition HTML and trust it'll render.
///
/// Exactly one of the four source variants must be supplied:
/// - `data_b64`   — caller provides the file bytes inline (after a fetch /
///   user upload). Pair with `mime_type` (otherwise inferred from `name`).
///   AVOID for files >~1 MB: tool args have practical size limits.
/// - `url`        — daemon fetches the URL with reqwest (10 s timeout) and
///   stores the bytes. Honors http/https only; file:/data: rejected.
/// - `local_path` — daemon reads the file directly from disk. Use this
///   when the user attached a local file to the chat (its path appears
///   in the agent's local_files context). Bypasses the tool-args size
///   limit, so it's the right path for multi-megabyte hero images. The
///   daemon applies the same magic-byte MIME sniff and asset_resize
///   pipeline as the other variants.
/// - `from_tab`   — daemon takes a screenshot of the given browser tab
///   and stores it as PNG. (Browser-tool integration; falls back to
///   error if the daemon isn't bridged to a tab capture surface.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachAssetRequest {
    /// Composition artifact id.
    pub composition_id: String,
    /// Optional explicit name with extension (e.g. `hero.png`). If omitted,
    /// derived from URL basename / local_path basename / content-type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional explicit MIME type. Useful when `data_b64` is supplied for
    /// a file the agent doesn't have an extension for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Inline base64 payload. Mutually exclusive with the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_b64: Option<String>,
    /// Public URL the daemon fetches. Mutually exclusive with the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Filesystem path the daemon reads directly. Mutually exclusive with
    /// the others. Use this when the user attached a local file to the
    /// chat — the path appears in the agent's local_files context, and
    /// large images can be moved to the artifact without serialising the
    /// bytes through tool arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Tab id to screenshot. Mutually exclusive with the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_tab: Option<i64>,
    /// Optional advisory role hint. Currently informational only — no
    /// behavioural difference; future versions may use it to route the
    /// asset into a specific scene slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// `canvas_attach_asset` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachAssetResponse {
    /// Path inside the composition's files map (e.g. `assets/hero.png`).
    /// The agent should reference this path directly in the composition
    /// HTML; the renderer will inline it as a `data:` URI at render time.
    pub path: String,
    /// MIME type that was stored alongside the asset.
    pub mime_type: String,
    /// Original byte length (NOT the base64-encoded length).
    pub size_bytes: u64,
}

#[cfg(test)]
mod meta_tests {
    use super::*;

    #[test]
    fn test_composition_meta_roundtrip() {
        let m = CompositionMeta {
            kind: CompositionKind::Composition,
            version: 1,
            spec: CompositionSpec {
                width: 1920,
                height: 1080,
                duration_sec: 30.0,
                fps: 30,
                bg: Some("#000000".into()),
            },
            origin: CompositionOrigin {
                template: Some("product-intro-16x9".into()),
                created_with: "canvas_create_composition".into(),
                created_at: 1745260800,
            },
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: CompositionMeta = serde_json::from_str(&s).unwrap();
        assert_eq!(back.kind, CompositionKind::Composition);
        assert_eq!(back.spec.width, 1920);
        assert_eq!(back.spec.fps, 30);
        assert_eq!(back.origin.template.as_deref(), Some("product-intro-16x9"));
    }

    #[test]
    fn test_composition_meta_rejects_wrong_kind() {
        let bad = serde_json::json!({
            "kind": "not-a-composition",
            "version": 1,
            "spec": {"width":100,"height":100,"duration_sec":1.0,"fps":30},
            "origin": {"created_with":"x","created_at":0}
        });
        let r: Result<CompositionMeta, _> = serde_json::from_value(bad);
        assert!(r.is_err(), "should reject kind != composition");
    }

    #[test]
    fn test_composition_meta_validate_hard_limits() {
        let mut m = CompositionMeta {
            kind: CompositionKind::Composition,
            version: 1,
            spec: CompositionSpec {
                width: 1920,
                height: 1080,
                duration_sec: 30.0,
                fps: 30,
                bg: None,
            },
            origin: CompositionOrigin {
                template: None,
                created_with: "t".into(),
                created_at: 0,
            },
        };
        assert!(m.validate_hard_limits().is_ok());
        m.spec.fps = 60;
        assert!(m.validate_hard_limits().is_err());
        m.spec.fps = 30;
        m.spec.duration_sec = 90.0;
        assert!(m.validate_hard_limits().is_err());
        m.spec.duration_sec = 30.0;
        m.spec.width = 3000;
        assert!(m.validate_hard_limits().is_err());
    }

    #[test]
    fn test_lint_issue_roundtrip() {
        let i = LintIssue {
            severity: LintSeverity::Warning,
            rule_id: "nf/cdn-whitelist".into(),
            message: "bad cdn".into(),
            line: Some(14),
            col: Some(3),
            fix_hint: Some("use esm.sh".into()),
        };
        let s = serde_json::to_string(&i).unwrap();
        let back: LintIssue = serde_json::from_str(&s).unwrap();
        assert_eq!(back.severity, LintSeverity::Warning);
        assert_eq!(back.rule_id, "nf/cdn-whitelist");
    }

    #[test]
    fn test_create_request_accepts_template_and_session_id() {
        let v = serde_json::json!({
            "title": "t",
            "width": 640, "height": 360,
            "duration_sec": 5.0, "fps": 30,
            "template": "product-intro-16x9",
            "session_id": "sess-abc"
        });
        let r: CreateCompositionRequest = serde_json::from_value(v).unwrap();
        assert_eq!(r.template.as_deref(), Some("product-intro-16x9"));
        assert_eq!(r.session_id.as_deref(), Some("sess-abc"));
    }

    #[test]
    fn test_create_request_backward_compat_without_new_fields() {
        let v = serde_json::json!({
            "title": "t",
            "width": 640, "height": 360,
            "duration_sec": 5.0, "fps": 30
        });
        let r: CreateCompositionRequest = serde_json::from_value(v).unwrap();
        assert!(r.template.is_none());
        assert!(r.session_id.is_none());
    }

    #[test]
    fn test_reveal_action_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&RevealAction::Play).unwrap(),
            "\"play\""
        );
        assert_eq!(
            serde_json::to_string(&RevealAction::Reveal).unwrap(),
            "\"reveal\""
        );
    }

    #[test]
    fn test_reveal_path_request_roundtrip() {
        let req = RevealPathRequest {
            path: "/tmp/x.mp4".into(),
            action: RevealAction::Play,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: RevealPathRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.path, "/tmp/x.mp4");
        assert_eq!(back.action, RevealAction::Play);
    }
}
