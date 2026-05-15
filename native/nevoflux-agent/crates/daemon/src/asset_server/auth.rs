// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Bearer-token middleware (mounted on `/v1/*`) and CORS layer (applied
//! globally with origin echoed from `AssetServerState::allowed_origin`).
//!
//! Per design C3: legacy `/file/:token` is at the root and bypasses
//! bearer auth; its security model is the URL-path UUID single-use token.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};

use super::state::AssetServerState;

/// Reject any /v1/* request that does not present `Authorization: Bearer <token>`
/// matching the daemon-wide bearer.
pub async fn bearer_middleware(
    State(state): State<Arc<AssetServerState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Host header check — defense against DNS rebinding (Threat 5).
    if let Some(host) = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        if !host.starts_with("127.0.0.1") && !host.starts_with("localhost") {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from("forbidden host"))
                .unwrap();
        }
    }

    // Pass-through OPTIONS preflights — the cors_middleware below answers them.
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }

    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let presented = header_value.strip_prefix("Bearer ").unwrap_or("");

    // Constant-time compare to avoid trivial timing oracle.
    if presented.len() != state.bearer_token.len()
        || !constant_time_eq(presented.as_bytes(), state.bearer_token.as_bytes())
    {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("unauthorized"))
            .unwrap();
    }

    next.run(req).await
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// CORS handling — echo the configured allow-origin (or `*` if unset) and
/// short-circuit OPTIONS preflights with a complete response.
pub async fn cors_middleware(
    State(state): State<Arc<AssetServerState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let origin = state.allowed_origin().unwrap_or_else(|| "*".to_string());

    // Preflight short-circuit.
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(
                "Access-Control-Allow-Origin",
                HeaderValue::from_str(&origin).unwrap_or_else(|_| HeaderValue::from_static("*")),
            )
            .header(
                "Access-Control-Allow-Methods",
                "GET, POST, OPTIONS",
            )
            .header(
                "Access-Control-Allow-Headers",
                "Authorization, Content-Type, X-NF-Request-Id, X-NF-Frame-Index, X-NF-Filename, X-NF-Total-Size",
            )
            .header("Access-Control-Max-Age", "86400")
            .body(Body::empty())
            .unwrap();
    }

    // Non-preflight: run the inner handler then append the origin header.
    let mut resp = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&origin) {
        resp.headers_mut()
            .insert("Access-Control-Allow-Origin", value);
    }
    resp
}

/// Dual-auth check used by the composition + asset GET handlers (Phase 2).
///
/// Accepts EITHER:
/// - a daemon-wide `Authorization: Bearer <token>` header (extension
///   `fetch()` path), OR
/// - a `?t=<short_token>` query param whose entry exists in
///   `composition_tokens` AND whose `composition_id` matches `expected_id`
///   (rendered HTML / iframe / `<img src=>` path — these consumers can't
///   add headers).
///
/// Per design D5 / §5.4: short tokens are multi-use within their TTL
/// (`peek` validates without consuming). The defense-in-depth check that
/// the token's stored `composition_id` matches the URL path prevents a
/// token issued for composition A from being replayed against composition B.
pub fn check_composition_request_auth(
    state: &AssetServerState,
    expected_composition_id: &str,
    headers: &axum::http::HeaderMap,
    query_token: Option<&str>,
) -> bool {
    if let Some(presented) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        if presented.len() == state.bearer_token.len()
            && constant_time_eq(presented.as_bytes(), state.bearer_token.as_bytes())
        {
            return true;
        }
    }

    if let Some(token) = query_token {
        if let Some(entry) = state.composition_tokens.peek(token) {
            return entry.composition_id == expected_composition_id;
        }
    }

    false
}

// Auth + CORS unit tests live alongside the integration tests in
// `super::tests` (mod.rs). They exercise these middlewares via a real
// running AssetServer + reqwest, which is faster to write and matches
// how the file_server.rs tests have always been organized.
