// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Generic short-lived token store for the AssetServer.
//!
//! Three independent instances live on `AssetServerState` per design D12:
//! - `download_tokens` — single-use, 60s TTL, browser_upload `/file/:token`
//! - `composition_tokens` — multi-use, 5min TTL, `/v1/asset/composition/:id/:name`
//! - `blob_tokens` — single-or-multi, 1h TTL, `/v1/blob/:id`
//!
//! All three reuse the same `TokenStore<E>` shape; only the entry type
//! and the consumer's `take` vs `peek` semantics differ.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

/// Trait for entries that have an absolute expiration instant.
pub trait EvictableEntry: Clone + Send + Sync + 'static {
    fn expires_at(&self) -> Instant;
}

/// Thread-safe, lock-free store for short-lived tokens.
///
/// Tokens are UUID v4 strings. Each entry carries its own `expires_at`,
/// so different stores can apply different TTL policies even though they
/// share the same `TokenStore` type.
#[derive(Debug)]
pub struct TokenStore<E: EvictableEntry> {
    inner: DashMap<String, E>,
}

impl<E: EvictableEntry> Default for TokenStore<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EvictableEntry> TokenStore<E> {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Insert an entry and return the freshly generated UUID token.
    pub fn insert(&self, entry: E) -> String {
        let token = Uuid::new_v4().to_string();
        self.inner.insert(token.clone(), entry);
        token
    }

    /// Atomically remove and return an entry (single-use semantics).
    ///
    /// Returns `None` if the token is unknown OR if it has expired —
    /// either way the entry is gone from the map afterward.
    pub fn take(&self, token: &str) -> Option<E> {
        let (_, entry) = self.inner.remove(token)?;
        if Instant::now() > entry.expires_at() {
            return None;
        }
        Some(entry)
    }

    /// Multi-use lookup — clone the entry without removing it.
    ///
    /// Returns `None` if unknown or expired; expired entries are NOT
    /// removed by `peek` (they are reaped by the eviction loop).
    pub fn peek(&self, token: &str) -> Option<E> {
        let entry = self.inner.get(token)?.clone();
        if Instant::now() > entry.expires_at() {
            return None;
        }
        Some(entry)
    }

    /// Drop every entry whose `expires_at` is in the past.
    pub fn sweep_expired(&self) {
        let now = Instant::now();
        self.inner.retain(|_, e| e.expires_at() > now);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Spawn a detached task that calls `sweep_expired` on `store` every
/// `interval`. The task lives for the daemon's lifetime; it stops when
/// the last `Arc<TokenStore>` reference is dropped.
pub fn spawn_eviction_loop<E: EvictableEntry>(store: Arc<TokenStore<E>>, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // First tick fires immediately — skip it so we don't sweep before
        // anyone has had a chance to insert.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            // If we are the only Arc left, the daemon shut down —— exit.
            if Arc::strong_count(&store) == 1 {
                break;
            }
            store.sweep_expired();
        }
    });
}

/// Generate a 32-byte URL-safe random token (used for daemon-wide bearer
/// + session_id). Falls back to UUIDv4 if no rng available — both are
/// fine for loopback-only auth.
pub fn random_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

// ---------------------------------------------------------------------------
// Concrete entries
// ---------------------------------------------------------------------------

/// Reuses the existing `agent::browser_input::upload::TokenEntry` shape
/// so Step B can switch `browser_input` over to `AssetServer::register_download`
/// without changing the validator/blocklist call sites.
impl EvictableEntry for crate::agent::browser_input::upload::TokenEntry {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

/// Composition asset URL token (Phase 2).
///
/// One entry covers ALL of a composition's assets — `register_composition_assets`
/// inserts a single CompositionEntry whose `composition_id` is what the
/// asset GET handler verifies against the URL path. Multi-use within the
/// 5-minute TTL.
#[derive(Clone, Debug)]
pub struct CompositionEntry {
    pub composition_id: String,
    pub expires_at: Instant,
}

impl EvictableEntry for CompositionEntry {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

/// Phase 5 placeholder — URL-as-handle blob registry entry.
#[derive(Clone, Debug)]
#[allow(dead_code)] // lit in Phase 5
pub struct BlobEntry {
    pub bytes: bytes::Bytes,
    pub content_type: String,
    pub expires_at: Instant,
}

impl EvictableEntry for BlobEntry {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::upload::TokenEntry;
    use std::path::PathBuf;

    fn entry_in(secs: i64) -> TokenEntry {
        let expires = if secs >= 0 {
            Instant::now() + Duration::from_secs(secs as u64)
        } else {
            Instant::now() - Duration::from_secs((-secs) as u64)
        };
        TokenEntry {
            path: PathBuf::from("/tmp/x"),
            mime_type: "image/png".into(),
            file_name: "x.png".into(),
            size: 0,
            expires_at: expires,
        }
    }

    #[test]
    fn take_returns_inserted_entry() {
        let s: TokenStore<TokenEntry> = TokenStore::new();
        let tok = s.insert(entry_in(60));
        assert!(s.take(&tok).is_some());
        assert!(s.is_empty());
    }

    #[test]
    fn take_rejects_expired() {
        let s: TokenStore<TokenEntry> = TokenStore::new();
        let tok = s.insert(entry_in(-1));
        assert!(s.take(&tok).is_none());
    }

    #[test]
    fn peek_does_not_remove() {
        let s: TokenStore<TokenEntry> = TokenStore::new();
        let tok = s.insert(entry_in(60));
        assert!(s.peek(&tok).is_some());
        assert!(s.peek(&tok).is_some(), "peek should be repeatable");
    }

    #[test]
    fn sweep_drops_expired_entries() {
        let s: TokenStore<TokenEntry> = TokenStore::new();
        s.insert(entry_in(-1));
        let live = s.insert(entry_in(60));
        assert_eq!(s.len(), 2);
        s.sweep_expired();
        assert_eq!(s.len(), 1);
        assert!(s.take(&live).is_some());
    }

    #[test]
    fn composition_entry_evicts_correctly() {
        let s: TokenStore<CompositionEntry> = TokenStore::new();
        s.insert(CompositionEntry {
            composition_id: "c1".into(),
            expires_at: Instant::now() - Duration::from_secs(1),
        });
        let live = s.insert(CompositionEntry {
            composition_id: "c2".into(),
            expires_at: Instant::now() + Duration::from_secs(60),
        });
        s.sweep_expired();
        assert_eq!(s.len(), 1);
        assert!(s.peek(&live).is_some());
    }

    #[test]
    fn blob_entry_evicts_correctly() {
        let s: TokenStore<BlobEntry> = TokenStore::new();
        s.insert(BlobEntry {
            bytes: bytes::Bytes::from_static(b"a"),
            content_type: "text/plain".into(),
            expires_at: Instant::now() - Duration::from_secs(1),
        });
        s.insert(BlobEntry {
            bytes: bytes::Bytes::from_static(b"b"),
            content_type: "text/plain".into(),
            expires_at: Instant::now() + Duration::from_secs(60),
        });
        s.sweep_expired();
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn random_token_is_unique() {
        let a = random_token();
        let b = random_token();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }
}
