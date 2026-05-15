// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/health` — liveness probe. Bearer-protected (consistent with
//! the rest of `/v1/*`); not strictly necessary but uniform.

use axum::{http::StatusCode, response::IntoResponse, Json};

pub async fn handle() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}
