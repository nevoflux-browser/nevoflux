// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/composition/:id` — composition HTML with `assets/X` references
//! rewritten to absolute `/v1/asset/composition/<id>/X?t=<token>` URLs.
//!
//! Auth: bearer header (default) OR `?t=<short_token>` query param. The
//! short token is what the daemon also embeds in the rewritten URLs, so a
//! consumer that received the HTML can re-fetch it with the same token
//! (e.g. an iframe re-loading the composition URL on navigation).
//!
//! Per design §7.1 / C1 the artifact's stored `index.html` is unchanged
//! (relative `assets/X` refs). Rewriting happens at GET time only.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::asset_server::auth::check_composition_request_auth;
use crate::asset_server::state::AssetServerState;
use crate::asset_server::COMPOSITION_TOKEN_TTL;
use crate::canvas_video::asset_inline;

#[derive(serde::Deserialize)]
pub struct AuthQuery {
    /// Short-lived per-composition token alternative to the bearer header.
    pub t: Option<String>,
}

pub async fn handle(
    State(state): State<Arc<AssetServerState>>,
    Path(id): Path<String>,
    Query(query): Query<AuthQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_composition_request_auth(&state, &id, &headers, query.t.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let storage = match state.storage.as_ref() {
        Some(db) => db.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "asset server: storage not wired",
            )
                .into_response();
        }
    };

    use nevoflux_storage::repositories::ArtifactRepository;
    let repo = ArtifactRepository::new(&storage);
    let rec = match repo.get(&id) {
        Ok(Some(rec)) => rec,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                format!("composition not found: {id}"),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(composition_id = %id, error = %e, "composition handler: storage error");
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
        }
    };

    let files = match rec.files.as_ref() {
        Some(f) => f,
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                "artifact has no files map (not a composition)",
            )
                .into_response();
        }
    };

    let entry = rec.entry.as_deref().unwrap_or("index.html");
    let html_raw = match files.get(entry) {
        Some(s) => s.clone(),
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("entry file missing: {entry}"),
            )
                .into_response();
        }
    };

    // Asset names come from the dedicated `composition_assets` table
    // (migration 016) — no longer interleaved with text files in the
    // JSON map. The token is shared across all assets of this
    // composition (D12: composition tokens are multi-use, 5-min TTL).
    use nevoflux_storage::repositories::CompositionAssetRepository;
    let asset_repo = CompositionAssetRepository::new(&storage);
    let asset_records = match asset_repo.list_all(&id) {
        Ok(rs) => rs,
        Err(e) => {
            tracing::error!(composition_id = %id, error = %e, "composition handler: list_all failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "asset list error").into_response();
        }
    };

    let bound_port = state.bound_port_for_url();
    let token = state.issue_composition_token(&id, COMPOSITION_TOKEN_TTL);
    let asset_urls: HashMap<String, String> = asset_records
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                format!(
                    "http://127.0.0.1:{bound_port}/v1/asset/composition/{}/{}?t={token}",
                    id, a.name
                ),
            )
        })
        .collect();

    let html = asset_inline::rewrite_assets_to_urls(&html_raw, &asset_urls);

    // ETag = sha256 over (entry html + sorted asset (name, len)) read
    // from the dedicated table. Stable across daemon restarts as long
    // as the artifact bytes haven't changed.
    let etag = compute_etag(entry, &html_raw, &asset_records);

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none_match.trim_matches('"') == etag {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, format!("\"{etag}\""))
                .body(axum::body::Body::empty())
                .unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "private, max-age=300")
        .header(header::ETAG, format!("\"{etag}\""))
        .body(axum::body::Body::from(html))
        .unwrap()
}

fn compute_etag(
    entry: &str,
    entry_html: &str,
    assets: &[nevoflux_storage::repositories::CompositionAsset],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entry.as_bytes());
    hasher.update(b"\0");
    hasher.update(entry_html.as_bytes());
    hasher.update(b"\0");
    // Records come pre-sorted by name from list_all (ORDER BY name).
    for a in assets {
        hasher.update(a.name.as_bytes());
        hasher.update(b":");
        hasher.update(a.bytes.len().to_string().as_bytes());
        hasher.update(b"\0");
    }
    let digest = hasher.finalize();
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest[..16])
}
