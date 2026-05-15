// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/oauth/:provider/callback?code=...&state=...` (Phase 5).
//!
//! Bearer-LESS — the OAuth provider hits this URL via browser redirect
//! and has no daemon bearer token. The `state` query parameter (a CSRF
//! nonce the originator generated) is the auth: the registry only
//! resolves a callback if `state` matches a pending flow AND the URL
//! path's `provider` matches the registered expected provider.
//!
//! On success, the handler:
//! 1. Resolves the OAuthRegistry entry via the oneshot
//! 2. Returns a small HTML page that closes the window
//!    (`window.close()`) and shows a fallback "you can close this tab"
//!    message in case `close()` is blocked

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::asset_server::oauth::OAuthCallbackResult;
use crate::asset_server::state::AssetServerState;

pub async fn handle(
    State(state): State<Arc<AssetServerState>>,
    Path(provider): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let oauth_state = match params.get("state").cloned() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return success_html(
                "Missing `state` parameter — this OAuth callback is malformed.",
                false,
            );
        }
    };

    let result = OAuthCallbackResult {
        provider: provider.clone(),
        state: oauth_state.clone(),
        code: params.get("code").cloned().filter(|s| !s.is_empty()),
        error: params.get("error").cloned().filter(|s| !s.is_empty()),
        error_description: params
            .get("error_description")
            .cloned()
            .filter(|s| !s.is_empty()),
    };

    let dispatched = state.oauth_registry.resolve(result.clone());

    if !dispatched {
        // Either the state is unknown / expired, or the provider name in
        // the URL path didn't match the registered expected provider.
        // Don't leak which — same opaque message either way.
        return success_html(
            "OAuth callback received, but no matching pending flow was found \
             (it may have expired). You can close this window.",
            false,
        );
    }

    // Dispatched OK. Tell the user the flow is done.
    if result.error.is_some() {
        let msg = format!(
            "OAuth provider returned an error: {}.\nThe originating session has been notified. You can close this window.",
            result.error.as_deref().unwrap_or("unknown")
        );
        success_html(&msg, true)
    } else {
        success_html(
            "OAuth flow complete. The originating session has the access token now. \
             You can close this window.",
            true,
        )
    }
}

/// Render a small self-contained HTML page that attempts to close the
/// window and falls back to a visible message. `auto_close` controls
/// whether the page also runs `window.close()`.
fn success_html(message: &str, auto_close: bool) -> Response {
    // Escape `<`, `>`, `&`, `"` in the message — the asset server's
    // OAuth handler is not user-content but the message is built from
    // provider-supplied error strings, which we treat as untrusted.
    let safe = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let close_script = if auto_close {
        "<script>window.close();</script>"
    } else {
        ""
    };
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<title>NevoFlux OAuth</title>
<style>
  body {{ font: 14px/1.4 system-ui, sans-serif; padding: 32px; background: #0b1020; color: #f5f5f7; }}
  .card {{ max-width: 480px; margin: 64px auto; padding: 24px;
          border-radius: 8px; background: #1a2030; }}
  h1 {{ margin: 0 0 12px; font-size: 16px; }}
  p {{ margin: 8px 0; color: #c4c8d4; }}
</style>
</head><body><div class="card">
<h1>NevoFlux OAuth</h1>
<p>{safe}</p>
</div>{close_script}</body></html>"#
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(html))
        .unwrap()
}
