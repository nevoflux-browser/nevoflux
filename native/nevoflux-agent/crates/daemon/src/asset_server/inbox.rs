// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Upload inbox — a request_id-keyed rendezvous between an HTTP POST
//! handler (the producer) and a tool-dispatch coroutine (the consumer).
//!
//! Phase 1 use case: `browser_screenshot` wakes up on
//! `inbox.await_request(req_id)` while the extension content script
//! POSTs the captured PNG to `/v1/upload/screenshot/<req_id>`. Whichever
//! side arrives first parks; whichever arrives second resolves the
//! oneshot.
//!
//! Bidirectional symmetry matters because in production the tool handler
//! starts awaiting BEFORE dispatching the browser request (so the channel
//! is set up before the extension can possibly POST), but tests prefer
//! the producer-first ordering. Both work via the `Pending` enum below.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Debug, Error)]
pub enum InboxError {
    #[error("inbox timeout waiting for request_id={request_id}")]
    Timeout { request_id: String },
    #[error("inbox channel closed unexpectedly for request_id={request_id}")]
    ChannelClosed { request_id: String },
    #[error("duplicate POST to request_id={request_id}: bytes already delivered")]
    DuplicatePost { request_id: String },
}

/// Pending state of one request_id slot.
enum Pending {
    /// Waiter parked first; sender will deliver bytes via this oneshot.
    Waiter(oneshot::Sender<Bytes>, Instant),
    /// POST arrived first; bytes parked until a waiter calls `await_request`.
    Bytes(Bytes, Instant),
}

impl Pending {
    fn deadline(&self) -> Instant {
        match self {
            Pending::Waiter(_, t) => *t,
            Pending::Bytes(_, t) => *t,
        }
    }
}

/// request_id → pending slot. Keys are request IDs (typically UUIDs);
/// the same key MUST NOT be POSTed to twice.
pub struct UploadInbox {
    inner: DashMap<String, Pending>,
    /// Hard ceiling for parked-bytes-without-waiter to bound memory.
    pub max_orphan_bytes: usize,
}

impl Default for UploadInbox {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadInbox {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
            max_orphan_bytes: 32 * 1024 * 1024, // 32 MiB
        }
    }

    /// Producer side: a POST handler delivers `bytes` for `request_id`.
    ///
    /// If a waiter is parked, the bytes are forwarded to it and the slot
    /// is removed. Otherwise the bytes are parked until a waiter shows up
    /// (subject to TTL).
    pub fn deliver(
        &self,
        request_id: &str,
        bytes: Bytes,
        ttl: Duration,
    ) -> Result<(), InboxError> {
        match self.inner.remove(request_id) {
            Some((_, Pending::Waiter(tx, _))) => {
                tx.send(bytes).map_err(|_| InboxError::ChannelClosed {
                    request_id: request_id.to_string(),
                })?;
                Ok(())
            }
            Some((_, Pending::Bytes(_, _))) => {
                // Slot already had bytes — second POST is an error.
                Err(InboxError::DuplicatePost {
                    request_id: request_id.to_string(),
                })
            }
            None => {
                let deadline = Instant::now() + ttl;
                self.inner
                    .insert(request_id.to_string(), Pending::Bytes(bytes, deadline));
                Ok(())
            }
        }
    }

    /// Consumer side: park until a POST delivers bytes for `request_id`,
    /// or the timeout fires.
    pub async fn await_request(
        &self,
        request_id: &str,
        timeout: Duration,
    ) -> Result<Bytes, InboxError> {
        // If bytes already parked, take them and return.
        if let Some(entry) = self.inner.remove(request_id) {
            match entry.1 {
                Pending::Bytes(b, _) => return Ok(b),
                Pending::Waiter(tx, deadline) => {
                    // Re-insert: another concurrent waiter is already parked.
                    // First-come-first-served; reject duplicates.
                    self.inner
                        .insert(request_id.to_string(), Pending::Waiter(tx, deadline));
                    return Err(InboxError::DuplicatePost {
                        request_id: request_id.to_string(),
                    });
                }
            }
        }

        // Otherwise, park a oneshot.
        let (tx, rx) = oneshot::channel();
        let deadline = Instant::now() + timeout;
        self.inner
            .insert(request_id.to_string(), Pending::Waiter(tx, deadline));

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(_)) => {
                self.inner.remove(request_id);
                Err(InboxError::ChannelClosed {
                    request_id: request_id.to_string(),
                })
            }
            Err(_) => {
                self.inner.remove(request_id);
                Err(InboxError::Timeout {
                    request_id: request_id.to_string(),
                })
            }
        }
    }

    /// Reap parked entries past their deadline.
    pub fn sweep_expired(&self) {
        let now = Instant::now();
        self.inner.retain(|_, p| p.deadline() > now);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Spawn a detached eviction loop that periodically reaps expired
/// inbox entries.
pub fn spawn_inbox_eviction_loop(inbox: Arc<UploadInbox>, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if Arc::strong_count(&inbox) == 1 {
                break;
            }
            inbox.sweep_expired();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deliver_then_await_returns_bytes() {
        let inbox = UploadInbox::new();
        inbox
            .deliver("R1", Bytes::from_static(b"hello"), Duration::from_secs(30))
            .unwrap();
        let bytes = inbox
            .await_request("R1", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"hello");
        assert!(inbox.is_empty());
    }

    #[tokio::test]
    async fn await_then_deliver_returns_bytes() {
        let inbox = Arc::new(UploadInbox::new());
        let inbox_clone = inbox.clone();

        let waiter =
            tokio::spawn(async move { inbox_clone.await_request("R2", Duration::from_secs(2)).await });

        // Yield so the waiter parks first.
        tokio::time::sleep(Duration::from_millis(50)).await;
        inbox
            .deliver("R2", Bytes::from_static(b"world"), Duration::from_secs(30))
            .unwrap();

        let bytes = waiter.await.unwrap().unwrap();
        assert_eq!(bytes.as_ref(), b"world");
    }

    #[tokio::test]
    async fn await_times_out_when_no_post() {
        let inbox = UploadInbox::new();
        let err = inbox
            .await_request("R3", Duration::from_millis(80))
            .await
            .unwrap_err();
        assert!(matches!(err, InboxError::Timeout { .. }));
        assert!(inbox.is_empty(), "timeout should clean up the slot");
    }

    #[tokio::test]
    async fn duplicate_post_returns_error() {
        let inbox = UploadInbox::new();
        inbox
            .deliver("R4", Bytes::from_static(b"a"), Duration::from_secs(30))
            .unwrap();
        let err = inbox
            .deliver("R4", Bytes::from_static(b"b"), Duration::from_secs(30))
            .unwrap_err();
        assert!(matches!(err, InboxError::DuplicatePost { .. }));
    }

    #[tokio::test]
    async fn sweep_expired_reaps_old_bytes() {
        let inbox = UploadInbox::new();
        inbox
            .deliver("R5", Bytes::from_static(b"x"), Duration::from_millis(20))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        inbox.sweep_expired();
        assert!(inbox.is_empty());
    }
}
