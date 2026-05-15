// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GET /v1/capabilities` — bearer-protected probe so the extension can
//! discover which Phase routes are lit on this daemon build.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

use crate::asset_server::state::AssetServerState;
use crate::asset_server::ASSET_SERVER_VERSION;

pub async fn handle(State(state): State<Arc<AssetServerState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "version": ASSET_SERVER_VERSION,
            "phases": ["screenshot-upload"],
            "max_body_size": state.max_body_size,
        })),
    )
}
