//! Async persistence layer for the EventBus.
//!
//! `PersistentWriter` receives events via a tokio mpsc channel and writes
//! each to the `event_bus_persistent` SQLite table. `PersistentCleaner`
//! periodically deletes expired rows.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing;

use nevoflux_storage::Storage;

use super::types::{BusEvent, PublisherIdentity};

/// Channel capacity for the writer's mpsc queue.
const WRITER_CHANNEL_CAPACITY: usize = 4096;

/// Interval between cleanup runs (seconds).
const CLEANUP_INTERVAL_SECS: u64 = 3600;

/// Clonable handle for sending events to the `PersistentWriter`.
#[derive(Clone)]
pub struct PersistentWriterHandle {
    tx: mpsc::Sender<BusEvent>,
}

impl PersistentWriterHandle {
    /// Try to enqueue an event for persistence.
    ///
    /// Returns `Err(event)` if the channel is full (back-pressure).
    #[allow(clippy::result_large_err)]
    pub fn send(&self, event: BusEvent) -> Result<(), BusEvent> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(ev) => ev,
            mpsc::error::TrySendError::Closed(ev) => ev,
        })
    }
}

/// Async writer that consumes events from its mpsc channel and persists them.
pub struct PersistentWriter {
    rx: mpsc::Receiver<BusEvent>,
    storage: Arc<Storage>,
}

impl PersistentWriter {
    /// Create a new writer and its associated handle.
    pub fn new(storage: Arc<Storage>) -> (PersistentWriterHandle, Self) {
        let (tx, rx) = mpsc::channel(WRITER_CHANNEL_CAPACITY);
        let handle = PersistentWriterHandle { tx };
        let writer = Self { rx, storage };
        (handle, writer)
    }

    /// Run the writer loop, consuming events until the channel closes.
    pub async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            if let Err(e) = self.write_event(&event) {
                tracing::error!(event_id = %event.id, "failed to persist event: {}", e);
            }
        }
        tracing::debug!("PersistentWriter shutting down — channel closed");
    }

    /// Write a single event to the `event_bus_persistent` table.
    fn write_event(&self, event: &BusEvent) -> nevoflux_storage::Result<()> {
        let (publisher_kind, publisher_id) = extract_publisher_identity(&event.publisher);
        let payload_json = event.payload.to_string();
        let created_at = event.created_at.timestamp();
        let expires_at = event.ttl.map(|ttl| created_at + ttl.as_secs() as i64);

        self.storage.database().with_connection(|conn| {
            conn.execute(
                "INSERT INTO event_bus_persistent (id, topic, payload, publisher_kind, publisher_id, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    event.id,
                    event.topic,
                    payload_json,
                    publisher_kind,
                    publisher_id,
                    created_at,
                    expires_at,
                ],
            )?;
            Ok(())
        })
    }
}

/// Periodic cleaner that deletes expired rows from `event_bus_persistent`.
pub struct PersistentCleaner {
    storage: Arc<Storage>,
}

impl PersistentCleaner {
    /// Create a new cleaner.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Run the cleanup loop forever (hourly ticks).
    pub async fn run(self) {
        let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
        // The first tick fires immediately; skip it so we don't clean on startup.
        interval.tick().await;

        loop {
            interval.tick().await;
            match self.cleanup_expired() {
                Ok(n) if n > 0 => {
                    tracing::info!(deleted = n, "cleaned up expired persistent events");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("persistent event cleanup failed: {}", e);
                }
            }
        }
    }

    /// Delete all rows whose `expires_at` is in the past.
    ///
    /// Returns the number of deleted rows.
    pub fn cleanup_expired(&self) -> nevoflux_storage::Result<usize> {
        self.storage.database().with_connection(|conn| {
            let now = chrono::Utc::now().timestamp();
            let deleted = conn.execute(
                "DELETE FROM event_bus_persistent WHERE expires_at IS NOT NULL AND expires_at < ?1",
                rusqlite::params![now],
            )?;
            Ok(deleted)
        })
    }
}

/// Extract (kind, id) strings from a `PublisherIdentity`.
fn extract_publisher_identity(publisher: &PublisherIdentity) -> (&'static str, String) {
    match publisher {
        PublisherIdentity::Internal => ("internal", String::new()),
        PublisherIdentity::Agent { session_id } => ("agent", session_id.clone()),
        PublisherIdentity::Extension { proxy_id } => ("extension", proxy_id.clone()),
        PublisherIdentity::Wasm { plugin_id } => ("wasm", plugin_id.clone()),
        PublisherIdentity::Mcp { server_id } => ("mcp", server_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::types::PublisherIdentity;
    use serde_json::json;
    use std::time::Duration;

    fn make_storage() -> Arc<Storage> {
        Arc::new(Storage::open_in_memory().expect("in-memory storage"))
    }

    fn make_event(topic: &str, ttl: Option<Duration>) -> BusEvent {
        BusEvent::persistent(
            topic,
            json!({"key": "value"}),
            PublisherIdentity::Agent {
                session_id: "sess-1".into(),
            },
            ttl,
        )
    }

    fn count_rows(storage: &Storage) -> i64 {
        storage
            .database()
            .with_connection(|conn| {
                let count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM event_bus_persistent", [], |row| {
                        row.get(0)
                    })?;
                Ok(count)
            })
            .unwrap()
    }

    #[tokio::test]
    async fn test_persistent_writer_writes_event() {
        let storage = make_storage();
        let (handle, writer) = PersistentWriter::new(Arc::clone(&storage));

        let event = make_event("task:created", Some(Duration::from_secs(3600)));
        let event_id = event.id.clone();
        handle.send(event).expect("send should succeed");

        // Drop the handle so the writer loop terminates after processing.
        drop(handle);
        writer.run().await;

        // Verify the event was written.
        let topic: String = storage
            .database()
            .with_connection(|conn| {
                let t: String = conn.query_row(
                    "SELECT topic FROM event_bus_persistent WHERE id = ?1",
                    rusqlite::params![event_id],
                    |row| row.get(0),
                )?;
                Ok(t)
            })
            .unwrap();
        assert_eq!(topic, "task:created");
    }

    #[tokio::test]
    async fn test_persistent_writer_writes_multiple_events() {
        let storage = make_storage();
        let (handle, writer) = PersistentWriter::new(Arc::clone(&storage));

        for i in 0..5 {
            let event = make_event(&format!("task:step:{}", i), None);
            handle.send(event).expect("send should succeed");
        }

        drop(handle);
        writer.run().await;

        assert_eq!(count_rows(&storage), 5);
    }

    #[tokio::test]
    async fn test_cleanup_expired_events() {
        let storage = make_storage();

        // Insert 2 expired events and 1 non-expired event directly.
        storage
            .database()
            .with_connection(|conn| {
                let now = chrono::Utc::now().timestamp();
                // Expired: expires_at in the past.
                conn.execute(
                    "INSERT INTO event_bus_persistent (id, topic, payload, publisher_kind, publisher_id, created_at, expires_at)
                     VALUES ('exp-1', 't', '{}', 'internal', '', ?1, ?2)",
                    rusqlite::params![now - 7200, now - 3600],
                )?;
                conn.execute(
                    "INSERT INTO event_bus_persistent (id, topic, payload, publisher_kind, publisher_id, created_at, expires_at)
                     VALUES ('exp-2', 't', '{}', 'internal', '', ?1, ?2)",
                    rusqlite::params![now - 7200, now - 1],
                )?;
                // Not expired: expires_at in the future.
                conn.execute(
                    "INSERT INTO event_bus_persistent (id, topic, payload, publisher_kind, publisher_id, created_at, expires_at)
                     VALUES ('live-1', 't', '{}', 'internal', '', ?1, ?2)",
                    rusqlite::params![now, now + 3600],
                )?;
                Ok(())
            })
            .unwrap();

        assert_eq!(count_rows(&storage), 3);

        let cleaner = PersistentCleaner::new(Arc::clone(&storage));
        let deleted = cleaner.cleanup_expired().unwrap();

        assert_eq!(deleted, 2);
        assert_eq!(count_rows(&storage), 1);

        // The remaining row should be the live one.
        let remaining_id: String = storage
            .database()
            .with_connection(|conn| {
                let id: String =
                    conn.query_row("SELECT id FROM event_bus_persistent", [], |row| row.get(0))?;
                Ok(id)
            })
            .unwrap();
        assert_eq!(remaining_id, "live-1");
    }

    #[tokio::test]
    async fn test_writer_handle_clone() {
        let storage = make_storage();
        let (handle, writer) = PersistentWriter::new(Arc::clone(&storage));

        let handle2 = handle.clone();
        handle
            .send(make_event("a:b", None))
            .expect("send via handle");
        handle2
            .send(make_event("c:d", None))
            .expect("send via clone");

        drop(handle);
        drop(handle2);
        writer.run().await;

        assert_eq!(count_rows(&storage), 2);
    }
}
