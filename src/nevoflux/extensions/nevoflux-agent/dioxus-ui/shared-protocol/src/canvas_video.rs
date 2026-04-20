//! Wire types for `canvas.video.*` namespace.

use serde::{Deserialize, Serialize};

/// `canvas.video.create_composition` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCompositionRequest {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
    #[serde(default)]
    pub bg: Option<String>,
    /// Raw HTML override. Phase B uses this for end-to-end tests;
    /// P2 adds template-driven authoring.
    #[serde(default)]
    pub html: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCompositionResponse {
    pub html: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f32,
    pub fps: u32,
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
