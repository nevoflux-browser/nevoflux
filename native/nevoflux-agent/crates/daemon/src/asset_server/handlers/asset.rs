// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/asset/composition/:id/:name` — single asset bytes from the
//! composition's `files` map.
//!
//! Auth: bearer header (default) OR `?t=<short_token>` query param. The
//! token is the multi-use composition token issued by
//! `register_composition_assets` / the composition GET handler; it is
//! `peek`-validated (multi-use within the 5-minute TTL).
//!
//! Range support: a single `Range: bytes=A-B` request returns 206 with a
//! `Content-Range` header. Multi-range requests fall back to 416 — Phase 2
//! simplification per the user spec.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::asset_server::auth::check_composition_request_auth;
use crate::asset_server::state::AssetServerState;
use crate::canvas_video::asset_inline;

#[derive(serde::Deserialize)]
pub struct AuthQuery {
    pub t: Option<String>,
}

pub async fn handle(
    State(state): State<Arc<AssetServerState>>,
    Path((id, name)): Path<(String, String)>,
    Query(query): Query<AuthQuery>,
    headers: HeaderMap,
) -> Response {
    tracing::info!(
        composition_id = %id,
        asset_name = %name,
        has_token = query.t.is_some(),
        "asset_server: GET /v1/asset/composition/:id/:name"
    );
    if !check_composition_request_auth(&state, &id, &headers, query.t.as_deref()) {
        tracing::warn!(
            composition_id = %id,
            asset_name = %name,
            "asset_server: 401 unauthorized"
        );
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

    use nevoflux_storage::repositories::CompositionAssetRepository;
    let asset_repo = CompositionAssetRepository::new(&storage);
    let asset = match asset_repo.get(&id, &name) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                format!("asset not found: {id}/{name}"),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(composition_id = %id, name = %name, error = %e, "asset handler: storage error");
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
        }
    };

    // MIME priority: magic-byte sniff of the raw bytes (most reliable;
    // immune to mis-extensioned filenames) → stored mime hint → path
    // extension. The migration recorded a path-based hint; magic-bytes
    // wins so that a JPEG-saved-as-foo.png still serves `image/jpeg`.
    let mime = sniff_mime(&asset.bytes, &name).unwrap_or_else(|| {
        asset.mime_type.clone().unwrap_or_else(|| {
            asset_inline::mime_for_path(&name).to_string()
        })
    });

    serve_with_range(&headers, mime, asset.bytes)
}

/// Magic-byte MIME sniff over raw bytes. Mirrors the encoded-payload
/// version in `canvas_video::asset_inline::magic_bytes_mime` but reads
/// raw bytes directly instead of a base64-encoded prefix.
fn sniff_mime(bytes: &[u8], _name: &str) -> Option<String> {
    if bytes.len() < 4 {
        return None;
    }
    let m = match bytes {
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x47, 0x49, 0x46, 0x38, ..] => "image/gif",
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => "image/webp",
        [_, _, _, _, 0x66, 0x74, 0x79, 0x70, ..] => "video/mp4",
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x41, 0x56, 0x45, ..] => "audio/wav",
        [0x49, 0x44, 0x33, ..] => "audio/mpeg",
        [0xFF, 0xFB, ..] | [0xFF, 0xF3, ..] | [0xFF, 0xF2, ..] => "audio/mpeg",
        [0x4F, 0x67, 0x67, 0x53, ..] => "audio/ogg",
        [b'w', b'O', b'F', b'2', ..] => "font/woff2",
        [b'w', b'O', b'F', b'F', ..] => "font/woff",
        _ => return None,
    };
    Some(m.to_string())
}

fn serve_with_range(headers: &HeaderMap, mime: String, bytes: Vec<u8>) -> Response {
    let total = bytes.len();
    let range_hdr = headers.get(header::RANGE).and_then(|v| v.to_str().ok());

    let parsed = match range_hdr {
        Some(s) => parse_single_range(s, total),
        None => RangeResult::None,
    };

    match parsed {
        RangeResult::None => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, total.to_string())
            .body(Body::from(bytes))
            .unwrap(),
        RangeResult::Partial { start, end } => {
            let slice = bytes[start..=end].to_vec();
            let len = slice.len();
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, len.to_string())
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                )
                .body(Body::from(slice))
                .unwrap()
        }
        RangeResult::Unsatisfiable => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Body::empty())
            .unwrap(),
    }
}

enum RangeResult {
    None,
    Partial { start: usize, end: usize },
    Unsatisfiable,
}

fn parse_single_range(s: &str, total: usize) -> RangeResult {
    let s = s.trim();
    let s = match s.strip_prefix("bytes=") {
        Some(v) => v.trim(),
        None => return RangeResult::Unsatisfiable,
    };
    // Multi-range (`A-B,C-D`) collapses to 416 per Phase 2 simplification.
    if s.contains(',') {
        return RangeResult::Unsatisfiable;
    }
    let (raw_start, raw_end) = match s.split_once('-') {
        Some(p) => p,
        None => return RangeResult::Unsatisfiable,
    };
    if total == 0 {
        return RangeResult::Unsatisfiable;
    }

    if raw_start.is_empty() {
        // suffix range: `-N` = last N bytes
        let n: usize = match raw_end.parse() {
            Ok(v) => v,
            Err(_) => return RangeResult::Unsatisfiable,
        };
        if n == 0 {
            return RangeResult::Unsatisfiable;
        }
        let n = n.min(total);
        return RangeResult::Partial {
            start: total - n,
            end: total - 1,
        };
    }

    let start: usize = match raw_start.parse() {
        Ok(v) => v,
        Err(_) => return RangeResult::Unsatisfiable,
    };
    if start >= total {
        return RangeResult::Unsatisfiable;
    }
    let end: usize = if raw_end.is_empty() {
        total - 1
    } else {
        match raw_end.parse::<usize>() {
            Ok(v) => v.min(total - 1),
            Err(_) => return RangeResult::Unsatisfiable,
        }
    };
    if end < start {
        return RangeResult::Unsatisfiable;
    }
    RangeResult::Partial { start, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_range() {
        match parse_single_range("bytes=0-9", 100) {
            RangeResult::Partial { start, end } => {
                assert_eq!((start, end), (0, 9));
            }
            _ => panic!("expected partial"),
        }
    }

    #[test]
    fn parses_open_end_range() {
        match parse_single_range("bytes=10-", 100) {
            RangeResult::Partial { start, end } => {
                assert_eq!((start, end), (10, 99));
            }
            _ => panic!("expected partial"),
        }
    }

    #[test]
    fn parses_suffix_range() {
        match parse_single_range("bytes=-5", 100) {
            RangeResult::Partial { start, end } => {
                assert_eq!((start, end), (95, 99));
            }
            _ => panic!("expected partial"),
        }
    }

    #[test]
    fn rejects_multi_range() {
        assert!(matches!(
            parse_single_range("bytes=0-1,3-4", 100),
            RangeResult::Unsatisfiable
        ));
    }

    #[test]
    fn rejects_out_of_bounds() {
        assert!(matches!(
            parse_single_range("bytes=200-300", 100),
            RangeResult::Unsatisfiable
        ));
    }
}
