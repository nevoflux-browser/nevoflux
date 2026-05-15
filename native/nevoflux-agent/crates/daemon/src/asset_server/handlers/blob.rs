// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/blob/:id` — URL-as-handle dereference (Phase 5).
//!
//! When a tool result is too large to ride native messaging (>100 KB
//! threshold proposed by §7.4), the daemon parks the bytes in
//! `AssetServerState::blob_tokens` via `AssetServer::register_blob` and
//! returns a `BlobRef { blob_id, content_type, bytes }` to the consumer.
//! The consumer then GETs `/v1/blob/<blob_id>` to retrieve the bytes
//! out-of-band. Tokens are single-use by default — second GET on the
//! same id 404s, matching the legacy `/file/:token` semantics.
//!
//! Bearer-protected. The token in the URL path is what scopes access
//! to a single registration; bearer authenticates the caller's right
//! to use the asset plane at all.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::asset_server::state::AssetServerState;

pub async fn handle(
    State(state): State<Arc<AssetServerState>>,
    Path(id): Path<String>,
) -> Response {
    let entry = match state.blob_tokens.take(&id) {
        Some(e) => e,
        None => {
            return (StatusCode::NOT_FOUND, "blob not found or expired").into_response();
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, entry.content_type)
        .header(header::CONTENT_LENGTH, entry.bytes.len().to_string())
        .body(axum::body::Body::from(entry.bytes))
        .unwrap()
}
