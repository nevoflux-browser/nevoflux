use std::sync::Mutex;
use std::time::{Duration, Instant};

use dashmap::DashMap;

use crate::learning::types::LearningEntry;

/// In-memory buffer for learning entries that deduplicates by a composite key
/// and supports periodic flushing to persistent storage (SQLite).
///
/// The buffer uses a `DashMap` for lock-free concurrent reads and writes.
/// Entries with the same `(category, domain, selector, source_event)` tuple
/// are merged: `occurrence_count` is incremented, `last_seen_at` is updated,
/// and the maximum `confidence` is kept.
pub struct MemoryBuffer {
    /// Concurrent map from dedup key to learning entry.
    entries: DashMap<String, LearningEntry>,
    /// Number of entries that triggers a flush.
    flush_threshold: usize,
    /// Maximum time between flushes.
    flush_interval: Duration,
    /// Timestamp of the last flush, protected by a mutex.
    last_flush: Mutex<Instant>,
}

impl MemoryBuffer {
    /// Create a new `MemoryBuffer` with the given flush threshold and interval.
    ///
    /// - `flush_threshold`: the number of entries at which `should_flush()` returns true.
    /// - `flush_interval`: the duration after which `should_flush()` returns true
    ///   even if the threshold has not been reached.
    pub fn new(flush_threshold: usize, flush_interval: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            flush_threshold,
            flush_interval,
            last_flush: Mutex::new(Instant::now()),
        }
    }

    /// Insert a learning entry into the buffer.
    ///
    /// If an entry with the same dedup key already exists, the entries are merged:
    /// - `occurrence_count` is incremented by the new entry's count.
    /// - `last_seen_at` is updated to the later timestamp.
    /// - `confidence` is set to the maximum of both entries.
    pub fn insert(&self, entry: LearningEntry) {
        let key = Self::dedup_key(&entry);

        self.entries
            .entry(key)
            .and_modify(|existing| {
                existing.occurrence_count += entry.occurrence_count;
                if entry.last_seen_at > existing.last_seen_at {
                    existing.last_seen_at = entry.last_seen_at;
                }
                if entry.confidence > existing.confidence {
                    existing.confidence = entry.confidence;
                }
            })
            .or_insert(entry);
    }

    /// Return the number of distinct (deduplicated) entries in the buffer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the buffer contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check whether the buffer should be flushed to persistent storage.
    ///
    /// Returns `true` if either:
    /// - The number of entries has reached or exceeded the `flush_threshold`, or
    /// - More time than `flush_interval` has elapsed since the last flush.
    pub fn should_flush(&self) -> bool {
        if self.entries.len() >= self.flush_threshold {
            return true;
        }
        let last = self.last_flush.lock().expect("last_flush lock poisoned");
        Instant::now().duration_since(*last) > self.flush_interval
    }

    /// Remove all entries from the buffer and return them as a `Vec`.
    ///
    /// After this call, `len()` will return 0.
    pub fn drain_all(&self) -> Vec<LearningEntry> {
        // Collect all keys first, then remove each one.
        // This avoids holding any DashMap shard locks while iterating.
        let keys: Vec<String> = self.entries.iter().map(|r| r.key().clone()).collect();
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, entry)) = self.entries.remove(&key) {
                result.push(entry);
            }
        }
        result
    }

    /// Remove all entries from the buffer without returning them.
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Record that a flush just happened, resetting the interval timer.
    pub fn mark_flushed(&self) {
        let mut last = self.last_flush.lock().expect("last_flush lock poisoned");
        *last = Instant::now();
    }

    /// Generate a dedup key from the entry's category, domain, selector, and source event.
    fn dedup_key(entry: &LearningEntry) -> String {
        let domain = entry.context.domain.as_deref().unwrap_or("");
        let selector = entry.context.selector.as_deref().unwrap_or("");
        format!(
            "{:?}|{}|{}|{}",
            entry.category, domain, selector, entry.source_event
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::types::*;

    #[test]
    fn buffer_inserts_and_retrieves() {
        let buffer = MemoryBuffer::new(20, Duration::from_secs(30));
        let entry = LearningEntry::new(LearningCategory::SiteInteraction, "test", "test");
        let id = entry.id.clone();

        buffer.insert(entry);
        assert_eq!(buffer.len(), 1);

        let drained = buffer.drain_all();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, id);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn buffer_signals_flush_at_threshold() {
        let buffer = MemoryBuffer::new(3, Duration::from_secs(30));
        for i in 0..3 {
            buffer.insert(LearningEntry::new(
                LearningCategory::SiteInteraction,
                &format!("event-{}", i),
                "summary",
            ));
        }
        assert!(buffer.should_flush());
    }

    #[test]
    fn buffer_merges_duplicate_entries() {
        let buffer = MemoryBuffer::new(20, Duration::from_secs(30));

        let entry1 = LearningEntry::new(LearningCategory::SiteInteraction, "click", "click failed")
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                selector: Some(".btn".into()),
                ..Default::default()
            });

        let entry2 = LearningEntry::new(LearningCategory::SiteInteraction, "click", "click failed")
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                selector: Some(".btn".into()),
                ..Default::default()
            });

        buffer.insert(entry1);
        buffer.insert(entry2);

        // Should merge into one entry with occurrence_count=2
        assert_eq!(buffer.len(), 1);
        let entries = buffer.drain_all();
        assert_eq!(entries[0].occurrence_count, 2);
    }

    #[test]
    fn buffer_is_empty_initially() {
        let buffer = MemoryBuffer::new(10, Duration::from_secs(60));
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn buffer_not_empty_after_insert() {
        let buffer = MemoryBuffer::new(10, Duration::from_secs(60));
        buffer.insert(LearningEntry::new(
            LearningCategory::ToolOptimization,
            "timeout",
            "tool timed out",
        ));
        assert!(!buffer.is_empty());
    }

    #[test]
    fn buffer_should_not_flush_below_threshold() {
        let buffer = MemoryBuffer::new(10, Duration::from_secs(3600));
        buffer.insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "clicked",
        ));
        assert!(!buffer.should_flush());
    }

    #[test]
    fn buffer_merge_keeps_max_confidence() {
        let buffer = MemoryBuffer::new(20, Duration::from_secs(30));

        let mut entry1 =
            LearningEntry::new(LearningCategory::ToolOptimization, "retry", "retry worked");
        entry1.confidence = 0.3;

        let mut entry2 =
            LearningEntry::new(LearningCategory::ToolOptimization, "retry", "retry worked");
        entry2.confidence = 0.9;

        buffer.insert(entry1);
        buffer.insert(entry2);

        let entries = buffer.drain_all();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(entries[0].occurrence_count, 2);
    }

    #[test]
    fn buffer_merge_updates_last_seen_at() {
        let buffer = MemoryBuffer::new(20, Duration::from_secs(30));

        let entry1 =
            LearningEntry::new(LearningCategory::UserPreference, "lang", "prefers English");

        // Small sleep to ensure distinct timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));

        let entry2 =
            LearningEntry::new(LearningCategory::UserPreference, "lang", "prefers English");
        let expected_last_seen = entry2.last_seen_at;

        buffer.insert(entry1);
        buffer.insert(entry2);

        let entries = buffer.drain_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].last_seen_at, expected_last_seen);
    }

    #[test]
    fn buffer_mark_flushed_resets_timer() {
        // Create buffer with a very short interval so should_flush would trigger
        let buffer = MemoryBuffer::new(100, Duration::from_millis(1));

        // Wait for interval to pass
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(buffer.should_flush());

        // After marking flushed, the timer resets
        buffer.mark_flushed();
        // Immediately after marking, should not flush (assuming threshold not reached)
        assert!(!buffer.should_flush());
    }

    #[test]
    fn buffer_different_domains_are_separate_entries() {
        let buffer = MemoryBuffer::new(20, Duration::from_secs(30));

        let entry1 = LearningEntry::new(LearningCategory::SiteInteraction, "click", "click failed")
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                selector: Some(".btn".into()),
                ..Default::default()
            });

        let entry2 = LearningEntry::new(LearningCategory::SiteInteraction, "click", "click failed")
            .with_context(LearningContext {
                domain: Some("other.com".into()),
                selector: Some(".btn".into()),
                ..Default::default()
            });

        buffer.insert(entry1);
        buffer.insert(entry2);

        // Different domains should not merge
        assert_eq!(buffer.len(), 2);
    }
}
