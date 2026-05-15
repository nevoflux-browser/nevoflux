//! Bridge dispatch for canvas.video.* messages.

use std::sync::Arc;

use nevoflux_protocol::canvas_video::{
    AssetPlaneEndpoint, CreateCompositionRequest, GetCompositionRequest, GetCompositionResponse,
    LintCompositionRequest, LoadCompositionHtmlRequest, RenderCancelRequest, RenderDone,
    RenderFailed, RenderFrameChunk, RenderReady, RenderStartRequest,
};
use serde_json::Value;

use crate::canvas_video::CanvasVideoService;
use crate::error::{DaemonError, Result};

pub async fn handle(
    svc: &Arc<CanvasVideoService>,
    message_type: &str,
    payload: Value,
) -> Result<Value> {
    match message_type {
        "canvas_video_create_composition" => {
            let req: CreateCompositionRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            let resp = svc.create_composition(req).await?;
            serde_json::to_value(resp).map_err(|e| DaemonError::InternalError(format!("{}", e)))
        }
        "canvas_video_render_start" => {
            let req: RenderStartRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            let resp = svc.render_start(req).await?;
            serde_json::to_value(resp).map_err(|e| DaemonError::InternalError(format!("{}", e)))
        }
        "canvas_video_render_cancel" => {
            let req: RenderCancelRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            let cancelled = svc.jobs().cancel(&req.job_id).await;
            // Phase 4: broadcast cancel on the SSE control channel so a
            // render tab listening via /v1/render/:job/sse can stop
            // capturing immediately. No-op if no SSE subscriber has
            // connected yet — the legacy NM cancellation flag still works.
            if cancelled {
                svc.broadcast_render_control(
                    &req.job_id,
                    crate::canvas_video::RenderControlEvent::Cancel,
                )
                .await;
            }
            Ok(serde_json::json!({ "cancelled": cancelled }))
        }
        "canvas_video_ready" => {
            // Retained for backward compat with the old push-model page; the
            // current page-driven loop doesn't require a ready handshake
            // (the first frame chunk implicitly signals "in progress").
            let _m: RenderReady = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            Ok(Value::Null)
        }
        "canvas_video_frame_chunk" => {
            let chunk: RenderFrameChunk = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            svc.on_frame_chunk(chunk).await?;
            Ok(Value::Null)
        }
        "canvas_video_render_done" => {
            let m: RenderDone = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            svc.on_render_done(&m.job_id, m.frames_emitted).await;
            Ok(Value::Null)
        }
        "canvas_video_render_failed" => {
            let m: RenderFailed = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            svc.on_render_failed(&m.job_id, &m.error).await;
            Ok(Value::Null)
        }
        "canvas_video_lint_composition" => {
            let req: LintCompositionRequest = serde_json::from_value(payload).map_err(|e| {
                DaemonError::InvalidRequest(format!("canvas_video_lint_composition: {e}"))
            })?;
            let report = svc.lint_composition(&req.composition_id).await?;
            serde_json::to_value(&report).map_err(|e| DaemonError::InternalError(format!("{e}")))
        }
        "canvas_video_get_composition" => {
            let req: GetCompositionRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            let (html, width, height, duration_sec, fps) =
                svc.get_composition_for_job(&req.job_id).await?;
            // Phase 4 wiring: hand the render page port + bearer so it can
            // POST captured PNG frames to /v1/render/:job_id/frame instead
            // of paging them through native messaging (which silently drops
            // anything > NM 1 MB once background.js's chunkMessage envelope
            // wraps it). `None` only when AssetServer isn't wired (test mode);
            // render page falls back to the legacy NM frame_chunk path then.
            let asset_plane = svc.asset_server().map(|s| AssetPlaneEndpoint {
                port: s.bound_port(),
                bearer_token: s.bearer_token().to_string(),
            });
            let resp = GetCompositionResponse {
                html,
                width,
                height,
                duration_sec,
                fps,
                asset_plane,
            };
            serde_json::to_value(resp).map_err(|e| DaemonError::InternalError(format!("{}", e)))
        }
        "canvas_video_load_composition_html" => {
            let req: LoadCompositionHtmlRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("parse: {}", e)))?;
            // Direct call to load_composition by composition_id — no
            // render-job snapshot indirection. The asset_server, if
            // wired, will rewrite `assets/X` to /v1/asset/... URLs;
            // otherwise refs stay relative.
            let (html, width, height, duration_sec, fps) =
                svc.load_composition(&req.composition_id).await?;
            // Canvas Editor / preview path doesn't need asset_plane (it
            // doesn't capture frames), but populating keeps the wire shape
            // stable for downstream consumers.
            let asset_plane = svc.asset_server().map(|s| AssetPlaneEndpoint {
                port: s.bound_port(),
                bearer_token: s.bearer_token().to_string(),
            });
            let resp = GetCompositionResponse {
                html,
                width,
                height,
                duration_sec,
                fps,
                asset_plane,
            };
            serde_json::to_value(resp).map_err(|e| DaemonError::InternalError(format!("{}", e)))
        }
        other => Err(DaemonError::InvalidRequest(format!(
            "unknown canvas.video.* message: {}",
            other
        ))),
    }
}
