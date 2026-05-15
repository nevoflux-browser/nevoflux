// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! 501 stubs for routes reserved for Phase 2-5.
//!
//! Per design D9 / §5.5: extension code can `fetch()` against a future
//! route, get `501`, and fall back to NM gracefully. When the route
//! lights up in a later phase, extension upgrades transparently.
//!
//! Each stub sets `X-NF-Future-Phase: phase-N` so probers can tell which
//! phase the route belongs to.

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::Response,
};

fn stub(phase: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .header("X-NF-Future-Phase", phase)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(format!(
            "reserved for asset-stream-plane {phase}"
        )))
        .unwrap()
}

pub async fn stub_phase2() -> Response {
    stub("phase-2")
}

pub async fn stub_phase3() -> Response {
    stub("phase-3")
}

pub async fn stub_phase4() -> Response {
    stub("phase-4")
}

pub async fn stub_phase5() -> Response {
    stub("phase-5")
}
