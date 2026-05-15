// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Upload handlers:
//!
//! - `POST /v1/upload/screenshot/:request_id` (Phase 1) — content script
//!   delivers a captured PNG to the matching `browser_screenshot` tool.
//! - `POST /v1/upload/generic/:inbox_id` (Phase 3) — generic byte sink
//!   for drag-and-drop, clipboard paste, computer-use screenshots, audio
//!   chunks, etc. Bytes are parked in `upload_inbox` keyed by the
//!   `inbox_id`; the awaiting consumer (whatever tool dispatched the
//!   request) picks them up via `AssetServer::await_inbox`.
//! - `POST /v1/upload/asset/:composition_id/:name` (Phase 3) — drop
//!   binary directly into a composition's `assets/<name>`. Reuses
//!   `CanvasVideoService::attach_asset` for resize + magic-byte MIME
//!   sniff + the dual-write `files`/`content` invariant.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};

use crate::asset_server::state::AssetServerState;

/// 30s is plenty: tool handler typically calls `await_screenshot` BEFORE
/// dispatching the browser request, so by the time the POST arrives a
/// waiter is already parked. The TTL only matters when the producer-first
/// branch fires (typical in tests).
const ORPHAN_BYTES_TTL: Duration = Duration::from_secs(30);

pub async fn handle_screenshot(
    State(state): State<Arc<AssetServerState>>,
    Path(request_id): Path<String>,
    body: Bytes,
) -> Response {
    let bytes_len = body.len();
    match state
        .upload_inbox
        .deliver(&request_id, body, ORPHAN_BYTES_TTL)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "request_id": request_id,
                "bytes": bytes_len,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(request_id, error = %e, "asset_server: screenshot upload rejected");
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": e.to_string(),
                })),
            )
                .into_response()
        }
    }
}

/// `POST /v1/upload/generic/:inbox_id` — Phase 3 generic byte sink.
///
/// Same plumbing as the screenshot route, just with longer TTL (90 s)
/// because typical generic-upload flows (drag-drop a file, computer-use
/// uploads) don't have a tightly-coupled tool handler awaiting on the
/// other side — there may be a brief delay before the consumer picks
/// it up. Returns the SHA-256 of the body so callers can verify
/// integrity after the round-trip.
const GENERIC_INBOX_TTL: Duration = Duration::from_secs(90);

pub async fn handle_generic(
    State(state): State<Arc<AssetServerState>>,
    Path(inbox_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let bytes_len = body.len();
    let sha256 = {
        let mut h = Sha256::new();
        h.update(&body);
        format!("{:x}", h.finalize())
    };
    let filename = headers
        .get("x-nf-filename")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match state
        .upload_inbox
        .deliver(&inbox_id, body, GENERIC_INBOX_TTL)
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": inbox_id,
                "bytes": bytes_len,
                "sha256": sha256,
                "filename": filename,
                "content_type": content_type,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(inbox_id, error = %e, "asset_server: generic upload rejected");
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "ok": false,
                    "id": inbox_id,
                    "error": e.to_string(),
                })),
            )
                .into_response()
        }
    }
}

/// `POST /v1/upload/asset/:composition_id/:name` — Phase 3 asset drop.
///
/// Receives raw bytes, base64-encodes them (matching the on-disk
/// `assets/X` format every other consumer reads), and dispatches to
/// `CanvasVideoService::attach_asset` which:
///   1. Reads the artifact's stage dims from `composition.meta.json`
///   2. Decodes the base64, resizes oversized rasters to fit the stage
///   3. Re-encodes (PNG↔JPEG when alpha allows) and re-base64s
///   4. `update_files(composition_id, files, content)` — the same
///      dual-write path `canvas_attach_asset` (LLM tool) uses
///   5. Returns the canonical path (`assets/<sanitized-name>`)
pub async fn handle_asset(
    State(state): State<Arc<AssetServerState>>,
    Path((composition_id, name)): Path<(String, String)>,
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

    let mime_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let size_bytes = body.len() as u64;

    // attach_asset expects base64. We always have the raw bytes here, so
    // encode once and hand off; attach_asset's resize step decodes + re-
    // encodes, but the cost is dominated by the resize itself, not the
    // base64 round-trip.
    use base64::{engine::general_purpose::STANDARD, Engine};
    let payload_b64 = STANDARD.encode(&body);

    match svc
        .attach_asset(&composition_id, &name, &mime_type, &payload_b64, size_bytes)
        .await
    {
        Ok(stored_path) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "composition_id": composition_id,
                "path": stored_path,
                "bytes": size_bytes,
                "mime_type": mime_type,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(
                composition_id,
                name,
                error = %e,
                "asset_server: asset upload failed"
            );
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "ok": false,
                    "composition_id": composition_id,
                    "name": name,
                    "error": e.to_string(),
                })),
            )
                .into_response()
        }
    }
}
