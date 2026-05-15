//! Concrete `LearningSource` implementations that produce learning entries
//! from real system signals.

use std::sync::{Arc, Mutex};

use nevoflux_storage::Storage;

use super::source::LearningSource;
use super::types::{LearningCategory, LearningContext, LearningEntry};

// ---------------------------------------------------------------------------
// ToolTraceLearningSource
// ---------------------------------------------------------------------------

/// Produces learning entries by analysing recent tool execution traces stored
/// in SQLite.
///
/// Each `collect()` call reads tool execution spans added since the last
/// collection (tracked by `last_seen_id`), groups them by tool name, and emits
/// a `LearningEntry` for every tool whose failure rate exceeds
/// `failure_threshold` during the window.
pub struct ToolTraceLearningSource {
    storage: Arc<Storage>,
    /// Row-id high-water mark — we only look at spans with id > this value.
    last_seen_id: Mutex<i64>,
    /// Minimum number of calls in a window before we emit a learning entry.
    min_calls: u32,
    /// Failure rate (0.0–1.0) above which we emit a learning entry.
    failure_threshold: f64,
}

impl ToolTraceLearningSource {
    /// Create a new source backed by the given storage.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            last_seen_id: Mutex::new(0),
            min_calls: 2,
            failure_threshold: 0.4,
        }
    }
}

impl LearningSource for ToolTraceLearningSource {
    fn source_name(&self) -> &str {
        "tool_trace"
    }

    fn collect(&self) -> Vec<LearningEntry> {
        let last_id = *self.last_seen_id.lock().unwrap();

        // Query recent tool_exec spans with id > last_seen_id
        let spans = match self.storage.traces().tool_spans_since(last_id, 500) {
            Ok(rows) => {
                if !rows.is_empty() {
                    tracing::debug!(
                        "ToolTraceLearningSource: found {} spans after id={}",
                        rows.len(),
                        last_id
                    );
                }
                rows
            }
            Err(e) => {
                tracing::warn!("ToolTraceLearningSource: query failed: {}", e);
                return Vec::new();
            }
        };

        if spans.is_empty() {
            return Vec::new();
        }

        // Update high-water mark
        if let Some(max_id) = spans.iter().map(|s| s.id).max() {
            *self.last_seen_id.lock().unwrap() = max_id;
        }

        // Group by tool_name → (total, failures, last_error, last_session)
        let mut tool_stats: std::collections::HashMap<String, ToolAgg> =
            std::collections::HashMap::new();
        for span in &spans {
            let name = span.tool_name.as_deref().unwrap_or("unknown");
            let agg = tool_stats.entry(name.to_string()).or_default();
            agg.total += 1;
            if !span.success {
                agg.failures += 1;
                if let Some(ref msg) = &span.error_msg {
                    agg.last_error = Some(msg.clone());
                }
            }
            if let Some(ms) = span.duration_ms {
                agg.total_duration_ms += ms;
            }
            agg.last_session = span.session_id.clone();
        }

        // Emit learning entries for tools with high failure rates
        let mut entries = Vec::new();
        for (tool_name, agg) in &tool_stats {
            if agg.total < self.min_calls as u64 {
                continue;
            }

            let failure_rate = agg.failures as f64 / agg.total as f64;
            if failure_rate >= self.failure_threshold {
                let summary = format!(
                    "Tool '{}' has {:.0}% failure rate ({}/{} calls)",
                    tool_name,
                    failure_rate * 100.0,
                    agg.failures,
                    agg.total,
                );
                let details = agg.last_error.as_deref().unwrap_or("no error details");
                let entry = LearningEntry::new(
                    LearningCategory::ToolOptimization,
                    "tool_high_failure_rate",
                    &summary,
                )
                .with_context(LearningContext {
                    tool_name: Some(tool_name.clone()),
                    session_id: Some(agg.last_session.clone()),
                    ..Default::default()
                });
                let mut entry = entry;
                entry.details = Some(details.to_string());
                entry.confidence = failure_rate.min(1.0);
                entries.push(entry);
            }

            // Also detect slow tools (avg > 10s)
            let avg_ms = agg.total_duration_ms as f64 / agg.total as f64;
            if avg_ms > 10_000.0 {
                let summary = format!(
                    "Tool '{}' is slow (avg {:.0}ms over {} calls)",
                    tool_name, avg_ms, agg.total,
                );
                let entry = LearningEntry::new(
                    LearningCategory::ToolOptimization,
                    "tool_slow_execution",
                    &summary,
                )
                .with_context(LearningContext {
                    tool_name: Some(tool_name.clone()),
                    session_id: Some(agg.last_session.clone()),
                    ..Default::default()
                });
                entries.push(entry);
            }
        }

        entries
    }
}

/// Aggregated stats for a single tool name.
#[derive(Default)]
struct ToolAgg {
    total: u64,
    failures: u64,
    total_duration_ms: u64,
    last_error: Option<String>,
    last_session: String,
}

// ---------------------------------------------------------------------------
// SiteAdaptationSource
// ---------------------------------------------------------------------------

/// Produces learning entries from site adaptation records that have a low
/// success rate, indicating the agent is struggling with a particular domain.
///
/// Each `collect()` call queries site_adaptations with `success_rate < 0.8`
/// and `sample_count >= 3`, emitting a `SiteInteraction` entry for each.
pub struct SiteAdaptationSource {
    storage: Arc<Storage>,
    /// Success rate threshold below which we emit a learning entry.
    max_success_rate: f64,
    /// Minimum number of samples before we consider the success rate meaningful.
    min_samples: i64,
    /// IDs we've already emitted, to avoid re-emitting every cycle.
    seen_ids: Mutex<std::collections::HashSet<String>>,
}

impl SiteAdaptationSource {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            max_success_rate: 0.8,
            min_samples: 3,
            seen_ids: Mutex::new(std::collections::HashSet::new()),
        }
    }
}

impl LearningSource for SiteAdaptationSource {
    fn source_name(&self) -> &str {
        "site_adaptation"
    }

    fn collect(&self) -> Vec<LearningEntry> {
        let records = match self.storage.site_adaptations().query_low_success_rate(
            self.max_success_rate,
            self.min_samples,
            100,
        ) {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("SiteAdaptationSource: query failed: {}", e);
                return Vec::new();
            }
        };

        let mut seen = self.seen_ids.lock().unwrap();
        let mut entries = Vec::new();

        for record in records {
            if seen.contains(&record.id) {
                continue;
            }
            seen.insert(record.id.clone());

            let summary = format!(
                "Site '{}' adaptation '{}' has low success rate ({:.0}% over {} samples)",
                record.domain,
                record.adaptation_type,
                record.success_rate * 100.0,
                record.sample_count,
            );
            let entry = LearningEntry::new(
                LearningCategory::SiteInteraction,
                "low_success_rate_adaptation",
                &summary,
            )
            .with_context(LearningContext {
                domain: Some(record.domain.clone()),
                url: record.url_pattern.clone(),
                ..Default::default()
            })
            .with_details(format!(
                "adaptation_type={}, content={}, verified={}",
                record.adaptation_type, record.content, record.verified
            ));
            let mut entry = entry;
            entry.confidence = 1.0 - record.success_rate; // lower success = higher confidence of problem
            entries.push(entry);
        }

        entries
    }
}

// ---------------------------------------------------------------------------
// MemoryChunkPreferenceSource
// ---------------------------------------------------------------------------

/// Produces learning entries from user-created memory chunks that contain
/// preference-like content.
///
/// When users use `memory_create` to save preferences (e.g., "I prefer dark
/// mode", "always respond in English"), those memory chunks become candidates
/// for promotion to USER.md via the learning pipeline.
pub struct MemoryChunkPreferenceSource {
    storage: Arc<Storage>,
    /// Last seen chunk count — we only process new chunks.
    last_count: Mutex<u32>,
}

impl MemoryChunkPreferenceSource {
    pub fn new(storage: Arc<Storage>) -> Self {
        let initial_count = storage.database().memory().count().unwrap_or(0);
        Self {
            storage,
            last_count: Mutex::new(initial_count),
        }
    }
}

impl LearningSource for MemoryChunkPreferenceSource {
    fn source_name(&self) -> &str {
        "memory_chunk_preference"
    }

    fn collect(&self) -> Vec<LearningEntry> {
        let current_count = match self.storage.database().memory().count() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut last = self.last_count.lock().unwrap();
        if current_count <= *last {
            return Vec::new();
        }

        // Fetch recent chunks (the diff between last and current count)
        let diff = (current_count - *last) as usize;
        *last = current_count;

        let chunks = match self.storage.database().memory().list(Some(diff)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("MemoryChunkPreferenceSource: list failed: {}", e);
                return Vec::new();
            }
        };

        // Filter for preference-like chunks via metadata or keyword heuristics
        let preference_keywords = [
            // English
            "prefer",
            "always",
            "never",
            "like",
            "dislike",
            "want",
            "don't want",
            "language",
            "style",
            "mode",
            "format",
            // Chinese
            "喜欢",
            "偏好",
            "总是",
            "始终",
            "永远不",
            "不要",
            "不喜欢",
            "习惯",
            "风格",
            "模式",
            "语言",
            "格式",
            "主题",
            "每次都",
            "一定要",
            "记住",
        ];

        let mut entries = Vec::new();
        for chunk in &chunks {
            let lower = chunk.content.to_lowercase();
            let is_preference = preference_keywords.iter().any(|kw| lower.contains(kw))
                || chunk
                    .metadata
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t == "preference" || t == "user_preference");

            if !is_preference {
                continue;
            }

            let entry = LearningEntry::new(
                LearningCategory::UserPreference,
                "memory_chunk_preference",
                &chunk.content,
            )
            .with_context(LearningContext {
                session_id: chunk.session_id.clone(),
                ..Default::default()
            });
            let mut entry = entry;
            entry.confidence = 0.7; // user explicitly saved → moderate confidence
            entries.push(entry);
        }

        entries
    }
}

// ---------------------------------------------------------------------------
// BufferedLearningSource
// ---------------------------------------------------------------------------

/// A simple push-based learning source backed by a thread-safe buffer.
///
/// External components (e.g., the agent runner, MCP tool executor) can call
/// [`push()`](Self::push) to enqueue entries. The collector drains the buffer
/// when it calls [`collect()`](LearningSource::collect).
pub struct BufferedLearningSource {
    name: String,
    buffer: Mutex<Vec<LearningEntry>>,
}

impl BufferedLearningSource {
    /// Create a new buffered source with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// Push a learning entry into the buffer.
    pub fn push(&self, entry: LearningEntry) {
        self.buffer.lock().unwrap().push(entry);
    }
}

impl LearningSource for BufferedLearningSource {
    fn source_name(&self) -> &str {
        &self.name
    }

    fn collect(&self) -> Vec<LearningEntry> {
        let mut buf = self.buffer.lock().unwrap();
        std::mem::take(&mut *buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::{CreateSiteAdaptationParams, Storage};

    #[test]
    fn buffered_source_collects_and_drains() {
        let source = BufferedLearningSource::new("test");
        assert_eq!(source.source_name(), "test");

        source.push(LearningEntry::new(
            LearningCategory::ToolOptimization,
            "test_event",
            "test summary",
        ));
        source.push(LearningEntry::new(
            LearningCategory::UserPreference,
            "pref_event",
            "user likes dark mode",
        ));

        let entries = source.collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source_event, "test_event");
        assert_eq!(entries[1].source_event, "pref_event");

        // Second collect should be empty (buffer drained)
        let entries = source.collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn tool_trace_source_returns_empty_when_no_spans() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let source = ToolTraceLearningSource::new(storage);
        let entries = source.collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn tool_trace_source_detects_high_failure_rate() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let traces = storage.traces();

        // Insert 5 calls: 4 failures, 1 success
        for i in 0..5 {
            let params = nevoflux_storage::CreateTraceSpanParams {
                session_id: "sess-1".into(),
                iteration: i,
                span_type: "tool_exec".into(),
                tool_name: Some("browser_click".into()),
                tool_params: None,
                success: i == 0, // only first one succeeds
                error_code: if i > 0 {
                    Some("TOOL_ERROR".into())
                } else {
                    None
                },
                error_msg: if i > 0 {
                    Some("Element not found".into())
                } else {
                    None
                },
                duration_ms: Some(100),
            };
            traces.create(params).unwrap();
        }

        let source = ToolTraceLearningSource::new(storage);
        let entries = source.collect();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_event, "tool_high_failure_rate");
        assert!(entries[0].summary.contains("browser_click"));
        assert!(entries[0].summary.contains("80%"));

        // Second collect should return empty (high-water mark updated)
        let entries = source.collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn site_adaptation_source_detects_low_success_rate() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let repo = storage.site_adaptations();

        // Create a site adaptation with low success rate
        let params = CreateSiteAdaptationParams::new(
            "tricky-site.com",
            "selector_result",
            r#"{"selector": ".dynamic-content"}"#,
        )
        .with_id("SA-low");
        repo.create(params).unwrap();
        repo.update_stats("SA-low", 0.3, 10).unwrap(); // 30% success rate, 10 samples

        // Create one with good success rate (should not be detected)
        let params2 = CreateSiteAdaptationParams::new(
            "good-site.com",
            "selector_result",
            r#"{"selector": ".main"}"#,
        )
        .with_id("SA-good");
        repo.create(params2).unwrap();
        repo.update_stats("SA-good", 0.95, 20).unwrap();

        let source = SiteAdaptationSource::new(Arc::clone(&storage));
        let entries = source.collect();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].summary.contains("tricky-site.com"));
        assert_eq!(entries[0].source_event, "low_success_rate_adaptation");

        // Second collect should return empty (seen_ids tracks already-emitted)
        let entries2 = source.collect();
        assert!(entries2.is_empty());
    }

    #[test]
    fn site_adaptation_source_ignores_insufficient_samples() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let repo = storage.site_adaptations();

        let params =
            CreateSiteAdaptationParams::new("new-site.com", "spa_behavior", r#"{"wait": 1000}"#)
                .with_id("SA-new");
        repo.create(params).unwrap();
        repo.update_stats("SA-new", 0.2, 2).unwrap(); // low success but only 2 samples

        let source = SiteAdaptationSource::new(Arc::clone(&storage));
        let entries = source.collect();
        assert!(
            entries.is_empty(),
            "should ignore records with too few samples"
        );
    }

    #[test]
    fn memory_chunk_preference_source_detects_preferences() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());

        // Source starts with current count
        let source = MemoryChunkPreferenceSource::new(Arc::clone(&storage));

        // Create a preference-like memory chunk
        let chunk =
            nevoflux_storage::MemoryChunk::new("I always prefer dark mode for all interfaces");
        storage.database().memory().create(&chunk).unwrap();

        // Create a non-preference chunk
        let chunk2 = nevoflux_storage::MemoryChunk::new("The weather in Paris is nice today");
        storage.database().memory().create(&chunk2).unwrap();

        let entries = source.collect();
        assert_eq!(
            entries.len(),
            1,
            "should only detect preference-like chunks"
        );
        assert!(entries[0].summary.contains("dark mode"));
    }

    #[test]
    fn memory_chunk_preference_source_no_duplicates() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());

        let source = MemoryChunkPreferenceSource::new(Arc::clone(&storage));

        let chunk = nevoflux_storage::MemoryChunk::new("I prefer English language responses");
        storage.database().memory().create(&chunk).unwrap();

        let entries1 = source.collect();
        assert_eq!(entries1.len(), 1);

        // Second collect without new chunks should return empty
        let entries2 = source.collect();
        assert!(entries2.is_empty());
    }

    #[test]
    fn test_memory_chunk_preference_source_chinese() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let source = MemoryChunkPreferenceSource::new(Arc::clone(&storage));

        // Create a Chinese preference chunk
        let chunk = nevoflux_storage::MemoryChunk::new("我喜欢暗色主题");
        storage.database().memory().create(&chunk).unwrap();

        let entries = source.collect();
        assert_eq!(entries.len(), 1, "Chinese preference should be detected");
    }

    #[test]
    fn test_memory_chunk_preference_source_mixed() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let source = MemoryChunkPreferenceSource::new(Arc::clone(&storage));

        let chunk = nevoflux_storage::MemoryChunk::new("I prefer 暗色模式");
        storage.database().memory().create(&chunk).unwrap();

        let entries = source.collect();
        assert_eq!(
            entries.len(),
            1,
            "Mixed language preference should be detected"
        );
    }

    #[test]
    fn tool_trace_source_ignores_low_failure_rate() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let traces = storage.traces();

        // Insert 5 calls: 4 success, 1 failure (20% failure rate < 50% threshold)
        for i in 0..5 {
            let params = nevoflux_storage::CreateTraceSpanParams {
                session_id: "sess-1".into(),
                iteration: i,
                span_type: "tool_exec".into(),
                tool_name: Some("read_file".into()),
                tool_params: None,
                success: i != 2, // only 3rd one fails
                error_code: if i == 2 {
                    Some("TOOL_ERROR".into())
                } else {
                    None
                },
                error_msg: None,
                duration_ms: Some(50),
            };
            traces.create(params).unwrap();
        }

        let source = ToolTraceLearningSource::new(storage);
        let entries = source.collect();
        assert!(entries.is_empty());
    }
}
