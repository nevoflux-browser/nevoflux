// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Render-tab routes (Phase 4):
//!
//! - `POST /v1/render/:job_id/frame` — render tab uploads a captured
//!   frame as a single PNG body. Replaces the legacy NM frame-chunk
//!   path (which paged a 30 fps × 10 s render through hundreds of
//!   sub-1 MB messages with reassembly bookkeeping).
//! - `GET /v1/render/:job_id/sse` — render tab subscribes to control
//!   signals (cancel / seek_to). Daemon broadcasts these when the user
//!   pauses or cancels from the sidebar; render tab listens and stops
//!   capturing accordingly.
//!
//! Both bearer-protected. POST returns 204 on success, 409 if the job
//! is no longer in a Queued/Running state. SSE streams text/event-stream
//! until the daemon closes the channel (job ended) or the client drops.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use futures::stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::asset_server::state::AssetServerState;
use crate::canvas_video::service::FrameSignal;
use crate::canvas_video::RenderControlEvent;

pub async fn handle_frame(
    State(state): State<Arc<AssetServerState>>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let svc = match state.canvas_video_service.as_ref() {
        Some(s) => s.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "asset server: canvas_video_service not wired",
            )
                .into_response();
        }
    };

    let frame_idx: u32 = match headers
        .get("x-nf-frame-index")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
    {
        Some(n) => n,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "missing or malformed X-NF-Frame-Index header",
            )
                .into_response();
        }
    };

    // Reject if the job is no longer accepting frames (already done /
    // failed / cancelled). 409 mirrors the behavior the design spec
    // pins, and means "the loop has finalized; drop this frame".
    let snapshot = svc.job_snapshot(&job_id).await;
    let accept = match snapshot.as_ref() {
        Some(s) => matches!(
            s.state,
            crate::canvas_video::job::JobState::Queued
                | crate::canvas_video::job::JobState::Running
        ),
        None => false,
    };
    if !accept {
        return (StatusCode::CONFLICT, "job is not accepting frames").into_response();
    }

    // Push the frame straight into the render loop's signal channel.
    // Same FrameSignal variant the legacy chunk-reassembly path produces
    // — the render loop can't tell the transport apart.
    svc.deliver_render_frame(
        &job_id,
        FrameSignal::Frame {
            frame_idx,
            png: body.to_vec(),
        },
    )
    .await;

    StatusCode::NO_CONTENT.into_response()
}

pub async fn handle_sse(
    State(state): State<Arc<AssetServerState>>,
    Path(job_id): Path<String>,
) -> Response {
    let svc = match state.canvas_video_service.as_ref() {
        Some(s) => s.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "asset server: canvas_video_service not wired",
            )
                .into_response();
        }
    };

    let rx = svc.subscribe_render_control(&job_id).await;
    // BroadcastStream errors when the receiver lags. We swallow lag
    // errors and emit comments so the connection stays open; a render
    // tab that misses a cancel signal will see the next one.
    let stream = BroadcastStream::new(rx).map(
        |item| -> Result<Event, Infallible> {
            match item {
                Ok(RenderControlEvent::Cancel) => Ok(Event::default()
                    .json_data(serde_json::json!({"type": "cancel"}))
                    .unwrap_or_else(|_| Event::default().comment("serialize-failed"))),
                Ok(RenderControlEvent::SeekTo(frame)) => Ok(Event::default()
                    .json_data(serde_json::json!({"type": "seek_to", "frame": frame}))
                    .unwrap_or_else(|_| Event::default().comment("serialize-failed"))),
                Err(_lagged) => Ok(Event::default().comment("lagged")),
            }
        },
    );

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
