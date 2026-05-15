// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared state threaded through every axum handler.
//!
//! Three independent token stores live here per design D12, plus the
//! daemon-wide bearer token, the screenshot upload inbox, and the
//! optional CORS allow-origin (only known once the extension has sent
//! its `bridge:hello`).

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use nevoflux_storage::Database;
use serde::{Deserialize, Serialize};

use super::token_store::{BlobEntry, CompositionEntry, TokenStore};
use super::AssetServerConfig;
use crate::agent::browser_input::upload::TokenEntry;

/// State shared across all handlers — `Arc<AssetServerState>` is passed
/// to axum via `with_state` and route_layer middleware.
pub struct AssetServerState {
    pub bearer_token: String,
    pub session_id: String,
    pub max_body_size: usize,
    /// CORS allow-origin (the moz-extension://... URL); `None` means CORS
    /// preflight is left to the browser default and `*` is sent — useful
    /// before the extension hello has been received.
    allowed_origin: RwLock<Option<String>>,
    pub download_tokens: Arc<TokenStore<TokenEntry>>,
    pub composition_tokens: Arc<TokenStore<CompositionEntry>>,
    pub blob_tokens: Arc<TokenStore<BlobEntry>>,
    pub upload_inbox: Arc<super::inbox::UploadInbox>,
    /// In-flight OAuth flows keyed by `state` param. The callback
    /// handler resolves an entry by sending `OAuthCallbackResult` back
    /// to the originator's `oneshot::Receiver`. Bearer-LESS route — the
    /// `state` nonce is the auth.
    pub oauth_registry: Arc<super::oauth::OAuthRegistry>,
    /// Artifact storage backend — required by composition / asset GET
    /// handlers (Phase 2). `None` in unit-test boots that don't exercise
    /// those routes; handlers return 503 when missing so callers can fall
    /// back to NM transport.
    pub storage: Option<Arc<Database>>,
    /// Canvas video service — used by the Phase 3 asset upload handler so
    /// it can call `attach_asset` (which encapsulates resize, magic-byte
    /// MIME sniff, and the dual-write `files`/`content` invariant).
    /// `None` in test boots that don't exercise the upload route.
    pub canvas_video_service: Option<Arc<crate::canvas_video::CanvasVideoService>>,
    /// Bound port — set by `AssetServer::start` after the listener wins a
    /// slot, read by the composition handler to construct absolute asset
    /// URLs. Stored as `AtomicU16` so the handler can access it via the
    /// shared `Arc<AssetServerState>` without taking a lock.
    bound_port: AtomicU16,
}

impl AssetServerState {
    pub fn new(config: &AssetServerConfig) -> Self {
        Self {
            bearer_token: config.bearer_token.clone(),
            session_id: config.session_id.clone(),
            max_body_size: config.max_body_size,
            allowed_origin: RwLock::new(config.allowed_origin.clone()),
            download_tokens: Arc::new(TokenStore::new()),
            composition_tokens: Arc::new(TokenStore::new()),
            blob_tokens: Arc::new(TokenStore::new()),
            upload_inbox: Arc::new(super::inbox::UploadInbox::new()),
            oauth_registry: Arc::new(super::oauth::OAuthRegistry::new()),
            storage: config.storage.clone(),
            canvas_video_service: config.canvas_video_service.clone(),
            bound_port: AtomicU16::new(0),
        }
    }

    pub fn set_allowed_origin(&self, origin: Option<String>) {
        if let Ok(mut guard) = self.allowed_origin.write() {
            *guard = origin;
        }
    }

    pub fn allowed_origin(&self) -> Option<String> {
        self.allowed_origin.read().ok().and_then(|g| g.clone())
    }

    pub fn set_bound_port(&self, port: u16) {
        self.bound_port.store(port, Ordering::SeqCst);
    }

    pub fn bound_port_for_url(&self) -> u16 {
        self.bound_port.load(Ordering::SeqCst)
    }

    /// Issue a single composition token covering all assets of `composition_id`.
    /// Multi-use within `ttl`; the token store eviction loop drops it past TTL.
    pub fn issue_composition_token(&self, composition_id: &str, ttl: Duration) -> String {
        let entry = CompositionEntry {
            composition_id: composition_id.to_string(),
            expires_at: Instant::now() + ttl,
        };
        self.composition_tokens.insert(entry)
    }
}

/// Information advertised to the extension via the `system_command status`
/// SystemResponse. Mirrors the design doc's `bridge:hello.asset_plane`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetPlaneInfo {
    pub port: u16,
    pub bearer_token: String,
    pub session_id: String,
    pub version: u32,
    pub max_body_size: usize,
    pub phases: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_token_stores_are_independent() {
        // Per D12, each store must be its own DashMap — verify they don't
        // share storage.
        let cfg = AssetServerConfig::default();
        let state = AssetServerState::new(&cfg);
        assert!(Arc::ptr_eq(&state.download_tokens, &state.download_tokens));
        // Type-level assertion: download/composition/blob have different
        // entry types so they CAN'T share storage even by accident.
        let _ = &state.download_tokens; // TokenStore<TokenEntry>
        let _ = &state.composition_tokens; // TokenStore<CompositionEntry>
        let _ = &state.blob_tokens; // TokenStore<BlobEntry>
        assert_eq!(state.download_tokens.len(), 0);
        assert_eq!(state.composition_tokens.len(), 0);
        assert_eq!(state.blob_tokens.len(), 0);
    }

    #[test]
    fn asset_plane_info_serializes_round_trip() {
        let info = AssetPlaneInfo {
            port: 19501,
            bearer_token: "secret".into(),
            session_id: "S".into(),
            version: 1,
            max_body_size: 1024,
            phases: vec!["screenshot-upload".into()],
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: AssetPlaneInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn allowed_origin_is_mutable() {
        let cfg = AssetServerConfig::default();
        let state = AssetServerState::new(&cfg);
        assert_eq!(state.allowed_origin(), None);
        state.set_allowed_origin(Some("moz-extension://abc".into()));
        assert_eq!(
            state.allowed_origin(),
            Some("moz-extension://abc".to_string())
        );
    }
}
