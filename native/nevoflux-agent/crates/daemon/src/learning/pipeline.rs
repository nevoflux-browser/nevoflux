//! Learning pipeline that drains entries from the in-memory buffer, persists
//! them to SQLite via `KnowledgeRepository`, validates pending entries
//! that meet configurable thresholds, and promotes validated entries into
//! the soul Markdown documents.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use nevoflux_storage::{CreateKnowledgeParams, CreateLearningMetricParams, Storage};

use crate::wasm::services::{get_embedding, SharedEmbedding};

use super::buffer::MemoryBuffer;
use super::conflict::{detect_conflict_against, resolve_conflict, ConflictAction};
use super::crypto::EncryptionService;
use super::types::LearningEntry;
use crate::error::Result;

/// Configurable thresholds that a pending knowledge entry must meet
/// before it can be promoted to "validated" status.
///
/// An entry qualifies for validation when ALL thresholds are satisfied:
/// - `hit_count >= min_occurrences`
/// - `confidence >= min_confidence`
/// - age (in hours since `created_at`) >= `min_alive_hours`
#[derive(Debug, Clone)]
pub struct ValidationThresholds {
    /// Minimum number of times the entry must have been observed.
    pub min_occurrences: u32,
    /// Minimum confidence score (0.0 to 1.0).
    pub min_confidence: f64,
    /// Minimum age in hours since the entry was created.
    pub min_alive_hours: u64,
}

impl Default for ValidationThresholds {
    fn default() -> Self {
        Self {
            min_occurrences: 2,
            min_confidence: 0.6,
            min_alive_hours: 12,
        }
    }
}

/// Per-category thresholds for the promotion step.
#[derive(Debug, Clone)]
pub struct CategoryPromotionThresholds {
    /// Minimum hit count before an entry can be promoted.
    pub min_hits: u32,
    /// Minimum effectiveness / confidence score for promotion.
    pub min_effectiveness: f64,
}

/// Configurable thresholds for the promotion step.
///
/// Each knowledge category has its own minimum hits and effectiveness.
/// An entry must also survive at least `min_alive_days` days before
/// it is eligible for promotion.
#[derive(Debug, Clone)]
pub struct PromotionThresholds {
    /// Maximum number of entries to promote in a single batch.
    pub batch_size: usize,
    /// Minimum age in days since the entry was created.
    pub min_alive_days: u64,
    /// Thresholds for `site_interaction` entries.
    pub site_interaction: CategoryPromotionThresholds,
    /// Thresholds for `tool_optimization` entries.
    pub tool_optimization: CategoryPromotionThresholds,
    /// Thresholds for `user_preference` entries.
    pub user_preference: CategoryPromotionThresholds,
    /// Maximum number of hot entries per category (capacity limits).
    pub hot_limit_site_interaction: usize,
    /// Maximum number of hot entries for tool_optimization.
    pub hot_limit_tool_optimization: usize,
    /// Maximum number of hot entries for user_preference.
    pub hot_limit_user_preference: usize,
}

impl Default for PromotionThresholds {
    fn default() -> Self {
        Self {
            batch_size: 50,
            min_alive_days: 3,
            site_interaction: CategoryPromotionThresholds {
                min_hits: 3,
                min_effectiveness: 0.6,
            },
            tool_optimization: CategoryPromotionThresholds {
                min_hits: 5,
                min_effectiveness: 0.6,
            },
            user_preference: CategoryPromotionThresholds {
                min_hits: 2,
                min_effectiveness: 0.5,
            },
            hot_limit_site_interaction: 15,
            hot_limit_tool_optimization: 10,
            hot_limit_user_preference: 10,
        }
    }
}

impl PromotionThresholds {
    /// Look up the category-specific thresholds for a given category string.
    fn for_category(&self, category: &str) -> &CategoryPromotionThresholds {
        match category {
            "site_interaction" | "siteinteraction" => &self.site_interaction,
            "tool_optimization" | "tooloptimization" => &self.tool_optimization,
            "user_preference" | "userpreference" => &self.user_preference,
            _ => &self.site_interaction, // default fallback
        }
    }

    /// Look up the hot-entry capacity limit for a given category.
    fn hot_limit_for_category(&self, category: &str) -> usize {
        match category {
            "site_interaction" | "siteinteraction" => self.hot_limit_site_interaction,
            "tool_optimization" | "tooloptimization" => self.hot_limit_tool_optimization,
            "user_preference" | "userpreference" => self.hot_limit_user_preference,
            _ => self.hot_limit_site_interaction,
        }
    }
}

/// Result of a promotion run, reporting how many entries were promoted vs skipped.
#[derive(Debug, Clone, Default)]
pub struct PromotionResult {
    /// Number of entries successfully marked as hot.
    pub promoted: usize,
    /// Number of entries skipped because they did not meet the promotion
    /// thresholds (e.g., confidence too low).
    pub skipped_threshold: usize,
    /// Number of entries that failed during the mark_hot step.
    pub failed: usize,
}

/// Pipeline that flushes `LearningEntry` items from a `MemoryBuffer` into the
/// SQLite `knowledge` table, validates pending entries that meet thresholds,
/// and promotes validated entries into soul documents.
///
/// When an [`EncryptionService`] is attached via [`with_encryption`](Self::with_encryption),
/// the `details` and `summary` fields of entries whose `privacy_level` is
/// `"sensitive"` are encrypted before being written to SQLite.
pub struct LearningPipeline {
    buffer: Arc<MemoryBuffer>,
    storage: Arc<Storage>,
    enabled: Arc<AtomicBool>,
    encryption: Option<Arc<EncryptionService>>,
    embedding: SharedEmbedding,
}

impl LearningPipeline {
    /// Create a new pipeline that reads from `buffer` and writes to `storage`.
    ///
    /// A [`SharedEmbedding`] is supplied so that vector embeddings become
    /// available once the background init completes.
    pub fn new(
        buffer: Arc<MemoryBuffer>,
        storage: Arc<Storage>,
        embedding: SharedEmbedding,
    ) -> Self {
        Self {
            buffer,
            storage,
            enabled: Arc::new(AtomicBool::new(true)),
            encryption: None,
            embedding,
        }
    }

    /// Attach an encryption service so that sensitive entries are encrypted
    /// before being written to SQLite during [`flush`](Self::flush).
    pub fn with_encryption(mut self, service: Arc<EncryptionService>) -> Self {
        self.encryption = Some(service);
        self
    }

    /// Pause the learning pipeline. While paused, `flush()`, `validate()`,
    /// and `promote()` will return early without processing.
    pub fn pause(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }

    /// Resume the learning pipeline after a pause.
    pub fn resume(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    /// Check whether the learning pipeline is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Return a clone of the shared enabled flag for use by other components
    /// (e.g., `LearningCollector`).
    pub fn enabled_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.enabled)
    }

    /// Clear all learning data: buffer entries, knowledge entries, and metrics.
    pub async fn clear_all(&self) -> Result<()> {
        self.buffer.clear();
        self.storage.knowledge().delete_all()?;
        self.storage.learning_metrics().delete_all()?;
        Ok(())
    }

    /// Record a learning metric for tracking pipeline operations.
    ///
    /// This is best-effort: errors are logged but not propagated so that
    /// metrics recording never causes a pipeline operation to fail.
    fn record_metric(&self, metric_type: &str, value: f64, metadata: Option<&str>) {
        let period = Utc::now().format("%Y-%m-%d").to_string();
        let params = CreateLearningMetricParams::new(metric_type, &period, value)
            .with_sample_count(metadata.map_or(0, |_| 1));
        if let Err(e) = self.storage.learning_metrics().create(params) {
            tracing::warn!(
                metric_type = metric_type,
                error = %e,
                "failed to record learning metric"
            );
        }
    }

    /// Drain all entries from the buffer and insert them into the SQLite
    /// knowledge table.
    ///
    /// Before creating a new knowledge row, the method checks whether an
    /// existing entry with the same `category`, `domain`, and `summary`
    /// already exists.  When a duplicate is found the existing row is
    /// *merged* (hit-count is accumulated, confidence is promoted to the
    /// maximum of the two values) instead of inserting a new row.
    ///
    /// When an [`EncryptionService`] is attached, the `summary` and `details`
    /// fields of entries whose `privacy_level` is `"sensitive"` are encrypted
    /// before insertion.
    ///
    /// Returns the number of **new** entries that were created (merged
    /// duplicates are not counted). Returns `Ok(0)` if the pipeline is paused.
    pub fn flush(&self) -> Result<usize> {
        if !self.is_enabled() {
            return Ok(0);
        }
        let entries = self.buffer.drain_all();
        let total = entries.len();
        let mut written = 0;

        for entry in &entries {
            let category_str = format!("{:?}", entry.category).to_lowercase();
            let domain = entry.context.domain.as_deref();

            // Check for an existing entry with the same category+domain+summary
            let existing =
                self.storage
                    .knowledge()
                    .find_duplicate(&category_str, domain, &entry.summary)?;

            if let Some(existing) = existing {
                // Merge: accumulate hits, take max confidence
                self.storage.knowledge().merge_entry(
                    &existing.id,
                    entry.occurrence_count,
                    entry.confidence,
                )?;
            } else if category_str == "tooloptimization" {
                // For tool_optimization entries, check if there's an existing hot entry
                // for the same tool_name. If so, update it rather than creating a duplicate.
                // This prevents "Tool 'X' has 75% failure (6/8)" and "Tool 'X' has 100%
                // failure (2/2)" from coexisting as separate hot entries.
                if let Some(ref tool_name) = entry.context.tool_name {
                    let existing_for_tool = self
                        .storage
                        .knowledge()
                        .find_hot_by_tool_name(&category_str, tool_name)?;

                    if let Some(old) = existing_for_tool {
                        // Update existing entry's content with new stats
                        let summary = &entry.summary;
                        let details = entry.details.as_deref().unwrap_or("");
                        self.storage
                            .knowledge()
                            .update_content(&old.id, details, summary)?;

                        // Update hot_summary
                        let domain_tag = entry.context.domain.as_deref().unwrap_or("universal");
                        let hot_summary = format!("[{}] {}", domain_tag, summary);
                        self.storage.knowledge().mark_hot(&old.id, &hot_summary)?;

                        // Merge hit counts
                        self.storage.knowledge().merge_entry(
                            &old.id,
                            entry.occurrence_count,
                            entry.confidence,
                        )?;

                        tracing::debug!(
                            "Updated existing tool_optimization for '{}': {}",
                            tool_name,
                            summary
                        );
                        written += 1;
                        continue;
                    }
                }
                // No existing hot entry for this tool — create new (fall through)
                let mut params = Self::entry_to_knowledge_params(entry);

                if let Some(ref provider) = get_embedding(&self.embedding) {
                    let text = format!(
                        "{} {}",
                        entry.summary,
                        entry.details.as_deref().unwrap_or("")
                    );
                    let emb_result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(provider.embed(&text))
                    });
                    match emb_result {
                        Ok(vec) => params.embedding = Some(vec),
                        Err(e) => {
                            tracing::debug!(
                                error = %e,
                                "Embedding generation failed for new knowledge entry, skipping"
                            );
                        }
                    }
                }

                if let Some(ref enc) = self.encryption {
                    let privacy = params.privacy_level.as_deref().unwrap_or("internal");
                    if privacy == "sensitive" {
                        params.summary = enc.encrypt_if_sensitive(&params.summary, privacy)?;
                        params.details = enc.encrypt_if_sensitive(&params.details, privacy)?;
                    }
                }

                self.storage.knowledge().create(params)?;
                written += 1;
            } else {
                // New entry: create as before
                let mut params = Self::entry_to_knowledge_params(entry);

                if let Some(ref provider) = get_embedding(&self.embedding) {
                    let text = format!(
                        "{} {}",
                        entry.summary,
                        entry.details.as_deref().unwrap_or("")
                    );
                    let emb_result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(provider.embed(&text))
                    });
                    match emb_result {
                        Ok(vec) => params.embedding = Some(vec),
                        Err(e) => {
                            tracing::debug!(
                                error = %e,
                                "Embedding generation failed for new knowledge entry, skipping"
                            );
                        }
                    }
                }

                if let Some(ref enc) = self.encryption {
                    let privacy = params.privacy_level.as_deref().unwrap_or("internal");
                    if privacy == "sensitive" {
                        params.summary = enc.encrypt_if_sensitive(&params.summary, privacy)?;
                        params.details = enc.encrypt_if_sensitive(&params.details, privacy)?;
                    }
                }

                self.storage.knowledge().create(params)?;
                written += 1;
            }
        }
        self.buffer.mark_flushed();
        self.record_metric("flush", total as f64, None);
        Ok(written)
    }

    /// Convert a `LearningEntry` to `CreateKnowledgeParams` for SQLite
    /// insertion.
    fn entry_to_knowledge_params(entry: &LearningEntry) -> CreateKnowledgeParams {
        CreateKnowledgeParams {
            category: format!("{:?}", entry.category).to_lowercase(),
            subcategory: entry.subcategory.clone(),
            domain: entry.context.domain.clone(),
            summary: entry.summary.clone(),
            details: entry.details.clone().unwrap_or_default(),
            source_type: Some("system".into()),
            privacy_level: Some(format!("{:?}", entry.privacy_level).to_lowercase()),
            ..Default::default()
        }
    }

    /// Validate pending knowledge entries that meet the given thresholds.
    ///
    /// Queries all pending entries from the `knowledge` table, checks each
    /// against the configured thresholds, and promotes qualifying entries
    /// from "pending" to "validated" status.
    ///
    /// Returns the number of entries that were validated. Returns `Ok(0)` if
    /// the pipeline is paused.
    pub fn validate(&self, thresholds: &ValidationThresholds) -> Result<usize> {
        if !self.is_enabled() {
            return Ok(0);
        }
        let pending = self.storage.knowledge().query_pending(1000)?;
        let now = Utc::now();
        let mut validated_count = 0;

        for entry in &pending {
            // Check hit_count threshold
            if (entry.hit_count as u32) < thresholds.min_occurrences {
                continue;
            }

            // Check confidence threshold
            if entry.confidence < thresholds.min_confidence {
                continue;
            }

            // Check age threshold: parse created_at as RFC3339 and compute hours elapsed
            if thresholds.min_alive_hours > 0 {
                if let Ok(created) = entry.created_at.parse::<DateTime<Utc>>() {
                    let age_hours = (now - created).num_hours();
                    if age_hours < 0 || (age_hours as u64) < thresholds.min_alive_hours {
                        continue;
                    }
                } else {
                    // If we cannot parse the timestamp, skip this entry
                    continue;
                }
            }

            // Entry meets all thresholds — check for conflicts before validating
            let existing_entries = self.storage.knowledge().query_by_subject(
                &entry.category,
                entry.domain.as_deref(),
                10,
            )?;

            let conflict = detect_conflict_against(entry, &existing_entries);

            match conflict {
                None => {
                    // No conflict, proceed with validation
                    self.storage
                        .knowledge()
                        .update_status(&entry.id, "validated")?;
                    validated_count += 1;
                }
                Some(conflict) => {
                    let action = resolve_conflict(&conflict);
                    match action {
                        ConflictAction::Archive(old_id) => {
                            self.storage
                                .knowledge()
                                .update_status(&old_id, "archived")?;
                            self.storage
                                .knowledge()
                                .update_status(&entry.id, "validated")?;
                            validated_count += 1;
                        }
                        ConflictAction::Keep => {
                            self.storage
                                .knowledge()
                                .update_status(&entry.id, "validated")?;
                            validated_count += 1;
                        }
                        ConflictAction::RejectIncoming(id) => {
                            self.storage.knowledge().update_status(&id, "archived")?;
                        }
                        ConflictAction::FlagForUser(ref c) => {
                            tracing::info!(
                                conflict_type = ?c.conflict_type,
                                entry_id = &entry.id,
                                "Knowledge conflict requires user arbitration, skipping"
                            );
                            // Leave as pending — no UI confirmation flow yet
                        }
                    }
                }
            }
        }

        self.record_metric("validation", validated_count as f64, None);
        Ok(validated_count)
    }

    /// Promote validated knowledge entries by marking them as "hot".
    ///
    /// For each validated entry that meets category-specific thresholds:
    /// 1. Checks confidence, hit count, and age thresholds.
    /// 2. Generates a one-line hot_summary.
    /// 3. Calls `mark_hot()` in SQLite (sets hot=1, status='promoted').
    /// 4. Enforces per-category capacity limits by unmarking excess entries.
    ///
    /// Returns a `PromotionResult` with counts of promoted/skipped entries.
    pub async fn promote(&self, thresholds: &PromotionThresholds) -> Result<PromotionResult> {
        if !self.is_enabled() {
            return Ok(PromotionResult::default());
        }
        let validated = self
            .storage
            .knowledge()
            .query_validated(thresholds.batch_size)?;

        let mut result = PromotionResult::default();

        for entry in &validated {
            // Skip entries that are already hot (idempotent)
            if entry.hot {
                continue;
            }

            // Look up category-specific thresholds
            let cat_thresholds = thresholds.for_category(&entry.category);

            // Check confidence / effectiveness threshold
            if entry.confidence < cat_thresholds.min_effectiveness {
                result.skipped_threshold += 1;
                continue;
            }

            // Check minimum hit count
            if (entry.hit_count as u32) < cat_thresholds.min_hits {
                result.skipped_threshold += 1;
                continue;
            }

            // Check minimum age (days since created_at)
            if thresholds.min_alive_days > 0 {
                if let Ok(created) = entry.created_at.parse::<DateTime<Utc>>() {
                    let age_days = (Utc::now() - created).num_days();
                    if age_days < 0 || (age_days as u64) < thresholds.min_alive_days {
                        result.skipped_threshold += 1;
                        continue;
                    }
                }
            }

            // Build hot_summary: a concise one-liner for system prompt injection
            let domain_tag = entry.domain.as_deref().unwrap_or("universal");
            let hot_summary = if let Some(ref resolution) = entry.resolution {
                format!("[{}] {} — {}", domain_tag, entry.summary, resolution)
            } else {
                format!("[{}] {}", domain_tag, entry.summary)
            };

            // Mark as hot in SQLite
            match self.storage.knowledge().mark_hot(&entry.id, &hot_summary) {
                Ok(()) => result.promoted += 1,
                Err(e) => {
                    tracing::warn!(
                        entry_id = %entry.id,
                        error = %e,
                        "failed to mark knowledge entry as hot"
                    );
                    result.failed += 1;
                }
            }
        }

        // Enforce per-category hot limits
        for category in &["site_interaction", "tool_optimization", "user_preference"] {
            if let Err(e) = self.enforce_hot_limits(category, thresholds) {
                tracing::warn!(
                    category = category,
                    error = %e,
                    "failed to enforce hot limits for category"
                );
            }
        }

        let metadata = format!(
            "promoted={},skipped_threshold={},failed={}",
            result.promoted, result.skipped_threshold, result.failed
        );
        self.record_metric("promotion", result.promoted as f64, Some(&metadata));

        Ok(result)
    }

    /// Enforce the maximum number of hot entries for a category.
    ///
    /// When the number of hot entries exceeds the limit, the entries with the
    /// lowest `confidence` are unmarked (hot=0). This keeps the system prompt
    /// from growing unboundedly.
    fn enforce_hot_limits(&self, category: &str, thresholds: &PromotionThresholds) -> Result<()> {
        let limit = thresholds.hot_limit_for_category(category);
        let hot_entries = self.storage.knowledge().list_hot_by_category(category)?;

        if hot_entries.len() <= limit {
            return Ok(());
        }

        // hot_entries are already sorted by confidence DESC from the query.
        // Unmark entries beyond the limit (i.e., the lowest-confidence ones).
        for entry in &hot_entries[limit..] {
            tracing::debug!(
                entry_id = %entry.id,
                category = category,
                confidence = entry.confidence,
                "Unmarking hot entry (over capacity limit)"
            );
            self.storage.knowledge().unmark_hot(&entry.id)?;
        }

        Ok(())
    }

    /// Resurrect an archived knowledge entry when it receives a new hit.
    ///
    /// Changes the entry's status from "archived" to "validated", updates
    /// `last_hit_at`, and increments `hit_count`. The decay score will be
    /// recalculated lazily on the next read.
    ///
    /// Returns `Ok(true)` if the entry was resurrected, `Ok(false)` if the
    /// entry was not in "archived" status.
    pub fn resurrect(&self, knowledge_id: &str) -> Result<bool> {
        // Only resurrect if the entry is currently archived
        let entry = self.storage.knowledge().get(knowledge_id)?;
        match entry {
            Some(e) if e.status == "archived" => {
                self.storage.knowledge().resurrect_entry(knowledge_id)?;
                Ok(true)
            }
            Some(_) => Ok(false), // Not archived, no resurrection needed
            None => Ok(false),    // Entry doesn't exist
        }
    }

    /// Get a reference to the underlying buffer (for inserting entries).
    pub fn buffer(&self) -> &MemoryBuffer {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::types::*;
    use nevoflux_storage::CreateKnowledgeParams;
    use rusqlite::params;
    use std::sync::Arc;
    use std::time::Duration;

    fn setup() -> (LearningPipeline, Arc<Storage>) {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let buffer = Arc::new(MemoryBuffer::new(20, Duration::from_secs(30)));
        let pipeline = LearningPipeline::new(
            buffer,
            storage.clone(),
            std::sync::Arc::new(std::sync::RwLock::new(None)),
        );
        (pipeline, storage)
    }

    /// Create relaxed promotion thresholds suitable for unit tests.
    ///
    /// Uses min_hits=1, min_effectiveness=0.0, min_alive_days=0 so that
    /// freshly-created entries qualify immediately.
    fn test_promotion_thresholds(batch_size: usize) -> PromotionThresholds {
        let relaxed = CategoryPromotionThresholds {
            min_hits: 1,
            min_effectiveness: 0.0,
        };
        PromotionThresholds {
            batch_size,
            min_alive_days: 0,
            site_interaction: relaxed.clone(),
            tool_optimization: relaxed.clone(),
            user_preference: relaxed,
            hot_limit_site_interaction: 15,
            hot_limit_tool_optimization: 10,
            hot_limit_user_preference: 10,
        }
    }

    #[test]
    fn flush_moves_entries_to_sqlite() {
        let (pipeline, storage) = setup();

        // Insert entries into buffer
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "click_failed",
                "Button click failed",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );

        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::ToolOptimization,
            "timeout",
            "Tool timed out",
        ));

        assert_eq!(pipeline.buffer().len(), 2);

        // Flush
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 2);
        assert_eq!(pipeline.buffer().len(), 0);

        // Verify in SQLite
        let results = storage
            .knowledge()
            .query_by_domain("example.com", 10)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "Button click failed");
    }

    #[test]
    fn flush_empty_buffer_returns_zero() {
        let (pipeline, _) = setup();
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn entry_to_knowledge_params_maps_fields_correctly() {
        let entry = LearningEntry::new(
            LearningCategory::SiteInteraction,
            "selector_changed",
            "Selector .btn was replaced",
        )
        .with_context(LearningContext {
            domain: Some("test.com".into()),
            ..Default::default()
        })
        .with_subcategory("css_selectors")
        .with_details("The .btn class was renamed to .button")
        .with_privacy(PrivacyLevel::Public);

        let params = LearningPipeline::entry_to_knowledge_params(&entry);

        assert_eq!(params.category, "siteinteraction");
        assert_eq!(params.subcategory, Some("css_selectors".into()));
        assert_eq!(params.domain, Some("test.com".into()));
        assert_eq!(params.summary, "Selector .btn was replaced");
        assert_eq!(params.details, "The .btn class was renamed to .button");
        assert_eq!(params.source_type, Some("system".into()));
        assert_eq!(params.privacy_level, Some("public".into()));
    }

    #[test]
    fn flush_preserves_domain_none_entries() {
        let (pipeline, storage) = setup();

        // Insert an entry without a domain
        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::UserPreference,
            "language",
            "User prefers English",
        ));

        let count = pipeline.flush().unwrap();
        assert_eq!(count, 1);

        // Entry without domain should not appear in domain-specific query
        let results = storage
            .knowledge()
            .query_by_domain("example.com", 10)
            .unwrap();
        assert!(results.is_empty());
    }

    // --- Encryption-aware flush tests ---

    fn setup_with_encryption() -> (LearningPipeline, Arc<Storage>, Arc<EncryptionService>) {
        use crate::learning::crypto::InMemoryKeyProvider;
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let buffer = Arc::new(MemoryBuffer::new(20, Duration::from_secs(30)));
        let provider = InMemoryKeyProvider::random();
        let enc = Arc::new(EncryptionService::new(&provider).unwrap());
        let pipeline = LearningPipeline::new(
            buffer,
            storage.clone(),
            std::sync::Arc::new(std::sync::RwLock::new(None)),
        )
        .with_encryption(Arc::clone(&enc));
        (pipeline, storage, enc)
    }

    #[test]
    fn flush_encrypts_sensitive_entries() {
        let (pipeline, storage, enc) = setup_with_encryption();

        // Insert a sensitive entry
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::UserPreference,
                "sensitive_pref",
                "User SSN is 123-45-6789",
            )
            .with_details("Detailed sensitive info")
            .with_privacy(PrivacyLevel::Sensitive)
            .with_context(LearningContext {
                domain: Some("bank.com".into()),
                ..Default::default()
            }),
        );

        let count = pipeline.flush().unwrap();
        assert_eq!(count, 1);

        // Retrieve from SQLite — summary and details should be encrypted
        let results = storage.knowledge().query_by_domain("bank.com", 10).unwrap();
        assert_eq!(results.len(), 1);

        let row = &results[0];
        // The stored values should NOT be the plaintext
        assert_ne!(row.summary, "User SSN is 123-45-6789");
        assert_ne!(row.details, "Detailed sensitive info");

        // They should be decryptable back to the original
        let decrypted_summary = enc.decrypt(&row.summary).unwrap();
        assert_eq!(decrypted_summary, "User SSN is 123-45-6789");

        let decrypted_details = enc.decrypt(&row.details).unwrap();
        assert_eq!(decrypted_details, "Detailed sensitive info");
    }

    #[test]
    fn flush_does_not_encrypt_public_entries() {
        let (pipeline, storage, _enc) = setup_with_encryption();

        // Insert a public entry
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "selector_changed",
                "GitHub uses data-testid",
            )
            .with_details("Publicly observable behavior")
            .with_privacy(PrivacyLevel::Public)
            .with_context(LearningContext {
                domain: Some("github.com".into()),
                ..Default::default()
            }),
        );

        pipeline.flush().unwrap();

        let results = storage
            .knowledge()
            .query_by_domain("github.com", 10)
            .unwrap();
        assert_eq!(results.len(), 1);

        let row = &results[0];
        // Public entries should be stored as plaintext
        assert_eq!(row.summary, "GitHub uses data-testid");
        assert_eq!(row.details, "Publicly observable behavior");
    }

    #[test]
    fn multiple_flushes_accumulate_in_sqlite() {
        let (pipeline, storage) = setup();

        // First flush
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "click_failed",
                "First failure",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );
        pipeline.flush().unwrap();

        // Second flush
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "scroll_failed",
                "Second failure",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );
        pipeline.flush().unwrap();

        // Both entries should be in SQLite
        let results = storage
            .knowledge()
            .query_by_domain("example.com", 10)
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    // --- Validation pipeline tests ---

    #[test]
    fn validation_thresholds_default_values() {
        let thresholds = ValidationThresholds::default();
        assert_eq!(thresholds.min_occurrences, 2);
        assert!((thresholds.min_confidence - 0.6).abs() < f64::EPSILON);
        assert_eq!(thresholds.min_alive_hours, 12);
    }

    #[test]
    fn validate_promotes_qualifying_entries() {
        let (pipeline, storage) = setup();

        // Create a knowledge entry directly in SQLite
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "test entry".into(),
                details: "test details".into(),
                domain: Some("example.com".into()),
                ..Default::default()
            })
            .unwrap();

        // Default confidence is 0.5 and hit_count is 1.
        // Update hit_count and confidence via raw SQL so the entry qualifies.
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Use relaxed age threshold (0 hours) so the freshly-created entry qualifies
        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1);

        // Verify status was updated to "validated"
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");
    }

    #[test]
    fn validate_skips_entries_below_threshold() {
        let (pipeline, storage) = setup();

        // Create entry with default hit_count=1, confidence=0.5
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "low confidence".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Strict thresholds that the default entry cannot meet
        let thresholds = ValidationThresholds {
            min_occurrences: 5,
            min_confidence: 0.9,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn validate_skips_entries_below_hit_count() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                summary: "high confidence but low hits".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Set high confidence but leave hit_count at 1
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET confidence = 0.95 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);

        // Entry should still be pending
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "pending");
    }

    #[test]
    fn validate_skips_entries_below_confidence() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                summary: "high hits but low confidence".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Set high hit_count but leave confidence at 0.5
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 10 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.9,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "pending");
    }

    #[test]
    fn validate_skips_entries_too_young() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "freshly created".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Set high hit_count and confidence
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 10, confidence = 0.9 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Require 24 hours of age — the entry was just created
        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.5,
            min_alive_hours: 24,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "pending");
    }

    #[test]
    fn validate_promotes_old_entry_with_backdated_created_at() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "old entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Backdate created_at to 48 hours ago and set qualifying stats
        let old_time = (Utc::now() - chrono::Duration::hours(48))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.8, created_at = ?1 WHERE id = ?2",
                    params![old_time, created.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 24,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1);

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");
    }

    #[test]
    fn validate_only_affects_pending_entries() {
        let (pipeline, storage) = setup();

        // Create two entries, both with qualifying stats
        let entry1 = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "pending entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        let entry2 = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "already validated".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Give both high stats
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 10, confidence = 0.9 WHERE id = ?1",
                    params![entry1.id],
                )?;
                conn.execute(
                    "UPDATE knowledge SET hit_count = 10, confidence = 0.9, status = 'validated' WHERE id = ?1",
                    params![entry2.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };

        // Only the pending entry should be validated
        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn validate_with_multiple_qualifying_entries() {
        let (pipeline, storage) = setup();

        // Create three qualifying entries
        for i in 0..3 {
            let created = storage
                .knowledge()
                .create(CreateKnowledgeParams {
                    category: "site_interaction".into(),
                    summary: format!("entry {}", i),
                    details: "details".into(),
                    ..Default::default()
                })
                .unwrap();

            storage
                .database()
                .with_connection(|conn| {
                    conn.execute(
                        "UPDATE knowledge SET hit_count = 5, confidence = 0.8 WHERE id = ?1",
                        params![created.id],
                    )?;
                    Ok(())
                })
                .unwrap();
        }

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn validate_no_pending_entries_returns_zero() {
        let (pipeline, _storage) = setup();

        let thresholds = ValidationThresholds::default();
        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);
    }

    // --- Promotion pipeline tests ---

    #[test]
    fn promotion_thresholds_default_values() {
        let thresholds = PromotionThresholds::default();
        assert_eq!(thresholds.batch_size, 50);
        assert_eq!(thresholds.min_alive_days, 3);
        assert!((thresholds.site_interaction.min_effectiveness - 0.6).abs() < f64::EPSILON);
        assert_eq!(thresholds.site_interaction.min_hits, 3);
        assert!((thresholds.tool_optimization.min_effectiveness - 0.6).abs() < f64::EPSILON);
        assert_eq!(thresholds.tool_optimization.min_hits, 5);
        assert!((thresholds.user_preference.min_effectiveness - 0.5).abs() < f64::EPSILON);
        assert_eq!(thresholds.user_preference.min_hits, 2);
    }

    #[test]
    fn promotion_result_default_is_zero() {
        let result = PromotionResult::default();
        assert_eq!(result.promoted, 0);
        assert_eq!(result.skipped_threshold, 0);
        assert_eq!(result.failed, 0);
    }

    /// Helper to create a validated entry with high confidence in SQLite.
    fn create_validated_entry(
        storage: &Storage,
        category: &str,
        subcategory: Option<&str>,
        domain: Option<&str>,
        summary: &str,
        confidence: f64,
    ) -> nevoflux_storage::Knowledge {
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: category.into(),
                subcategory: subcategory.map(|s| s.to_string()),
                domain: domain.map(|d| d.to_string()),
                summary: summary.into(),
                details: "test details".into(),
                ..Default::default()
            })
            .unwrap();

        // Set confidence and status to validated
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET confidence = ?1, status = 'validated' WHERE id = ?2",
                    params![confidence, created.id],
                )?;
                Ok(())
            })
            .unwrap();

        storage.knowledge().get(&created.id).unwrap().unwrap()
    }

    #[tokio::test]
    async fn promote_marks_entry_as_hot() {
        let (pipeline, storage) = setup();

        // Create a validated site_interaction entry
        let entry = create_validated_entry(
            &storage,
            "site_interaction",
            Some("selector"),
            Some("github.com"),
            "GitHub uses data-testid selectors",
            0.85,
        );

        let thresholds = test_promotion_thresholds(10);

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 1);
        assert_eq!(result.skipped_threshold, 0);
        assert_eq!(result.failed, 0);

        // Verify the entry is now "promoted" and hot in SQLite
        let updated = storage.knowledge().get(&entry.id).unwrap().unwrap();
        assert_eq!(updated.status, "promoted");
        assert!(updated.promoted_at.is_some());
        assert!(updated.hot);
        assert!(updated.hot_summary.is_some());
        assert!(updated.hot_summary.unwrap().contains("github.com"));
    }

    #[tokio::test]
    async fn promote_marks_user_preference_as_hot() {
        let (pipeline, storage) = setup();

        create_validated_entry(
            &storage,
            "user_preference",
            Some("language"),
            None,
            "User prefers concise replies",
            0.9,
        );

        let thresholds = test_promotion_thresholds(10);
        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 1);

        // Verify hot entry exists
        let hot = storage.knowledge().list_hot().unwrap();
        assert_eq!(hot.len(), 1);
        assert!(hot[0]
            .hot_summary
            .as_ref()
            .unwrap()
            .contains("User prefers concise replies"));
    }

    #[tokio::test]
    async fn promote_skips_low_confidence_entries() {
        let (pipeline, storage) = setup();

        // Entry with low confidence (0.3 < 0.6 threshold)
        create_validated_entry(
            &storage,
            "site_interaction",
            None,
            None,
            "Low confidence entry",
            0.3,
        );

        // Use strict effectiveness threshold so 0.3 confidence is below it
        let mut thresholds = test_promotion_thresholds(10);
        thresholds.site_interaction.min_effectiveness = 0.6;

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 0);
        assert_eq!(result.skipped_threshold, 1);
    }

    #[tokio::test]
    async fn promote_no_validated_entries_returns_empty_result() {
        let (pipeline, _storage) = setup();

        let thresholds = PromotionThresholds::default();
        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 0);
        assert_eq!(result.skipped_threshold, 0);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn promote_multiple_entries_in_one_batch() {
        let (pipeline, storage) = setup();

        // Create 3 validated entries with different categories
        create_validated_entry(
            &storage,
            "site_interaction",
            None,
            Some("example.com"),
            "Site uses react",
            0.8,
        );
        create_validated_entry(
            &storage,
            "tool_optimization",
            None,
            None,
            "Increase timeout to 60s",
            0.75,
        );
        create_validated_entry(
            &storage,
            "user_preference",
            Some("domain"),
            None,
            "User works in fintech",
            0.9,
        );

        let thresholds = test_promotion_thresholds(10);

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 3);

        // All 3 should be hot
        let hot = storage.knowledge().list_hot().unwrap();
        assert_eq!(hot.len(), 3);
    }

    #[tokio::test]
    async fn promote_respects_batch_size_limit() {
        let (pipeline, storage) = setup();

        // Create 5 validated entries
        for i in 0..5 {
            create_validated_entry(
                &storage,
                "site_interaction",
                None,
                Some(&format!("site{}.com", i)),
                &format!("Entry {}", i),
                0.8,
            );
        }

        // batch_size of 2 should only process 2
        let thresholds = test_promotion_thresholds(2);

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 2);

        // 3 entries should still be validated
        let remaining = storage.knowledge().query_validated(10).unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[tokio::test]
    async fn promote_end_to_end_flush_validate_promote() {
        let (pipeline, storage) = setup();

        // Step 1: Insert entry into buffer and flush to SQLite
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "selector",
                "example.com uses semantic selectors",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            })
            .with_details("Verified across multiple pages"),
        );

        let flush_count = pipeline.flush().unwrap();
        assert_eq!(flush_count, 1);

        // Step 2: Update entry to meet validation thresholds, then validate
        let pending = storage.knowledge().query_pending(10).unwrap();
        assert_eq!(pending.len(), 1);

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.85 WHERE id = ?1",
                    params![pending[0].id],
                )?;
                Ok(())
            })
            .unwrap();

        let val_thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };
        let validated_count = pipeline.validate(&val_thresholds).unwrap();
        assert_eq!(validated_count, 1);

        // Step 3: Promote (mark as hot)
        let promo_thresholds = test_promotion_thresholds(10);
        let result = pipeline.promote(&promo_thresholds).await.unwrap();
        assert_eq!(result.promoted, 1);

        // Verify the full pipeline result
        let entry = storage.knowledge().get(&pending[0].id).unwrap().unwrap();
        assert_eq!(entry.status, "promoted");
        assert!(entry.promoted_at.is_some());
        assert!(entry.hot);
    }

    #[tokio::test]
    async fn promote_enforces_hot_limits() {
        let (pipeline, storage) = setup();

        // Create 5 validated entries in site_interaction
        for i in 0..5 {
            create_validated_entry(
                &storage,
                "site_interaction",
                None,
                Some(&format!("site{}.com", i)),
                &format!("Entry {}", i),
                0.5 + (i as f64) * 0.1, // 0.5, 0.6, 0.7, 0.8, 0.9
            );
        }

        // Set hot limit to 3 for site_interaction
        let mut thresholds = test_promotion_thresholds(10);
        thresholds.hot_limit_site_interaction = 3;

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 5);

        // After enforcement, only 3 should remain hot (highest confidence)
        let hot = storage
            .knowledge()
            .list_hot_by_category("site_interaction")
            .unwrap();
        assert_eq!(hot.len(), 3);

        // The top 3 by confidence should be kept
        let confidences: Vec<f64> = hot.iter().map(|e| e.confidence).collect();
        assert!(confidences.iter().all(|&c| c >= 0.7));
    }

    // --- Flush deduplication tests ---

    #[test]
    fn flush_deduplicates_against_existing_entries() {
        let (pipeline, storage) = setup();

        // First flush: create entry
        let mut entry1 = LearningEntry::new(
            LearningCategory::ToolOptimization,
            "tool_slow",
            "Tool X is slow",
        );
        entry1.details = Some("avg 15s".into());
        entry1.context.domain = Some("example.com".into());
        entry1.confidence = 0.6;
        entry1.occurrence_count = 1;

        pipeline.buffer().insert(entry1);
        let count1 = pipeline.flush().unwrap();
        assert_eq!(count1, 1);

        // Second flush: same category+domain+summary → should merge
        let mut entry2 = LearningEntry::new(
            LearningCategory::ToolOptimization,
            "tool_slow",
            "Tool X is slow",
        );
        entry2.details = Some("avg 20s".into());
        entry2.context.domain = Some("example.com".into());
        entry2.confidence = 0.8;
        entry2.occurrence_count = 2;

        pipeline.buffer().insert(entry2);
        let count2 = pipeline.flush().unwrap();
        assert_eq!(count2, 0); // 0 new entries (merged into existing)

        // Verify only 1 entry, with merged values
        let all = storage.knowledge().query_all(100).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].hit_count, 3); // 1 + 2
        assert!((all[0].confidence - 0.8).abs() < 0.01); // max(0.6, 0.8)
    }

    #[test]
    fn flush_creates_new_entry_when_summary_differs() {
        let (pipeline, storage) = setup();

        // First entry
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::ToolOptimization,
                "tool_slow",
                "Tool X is slow",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );
        pipeline.flush().unwrap();

        // Second entry: same category+domain but different summary
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::ToolOptimization,
                "tool_slow",
                "Tool Y is slow",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 1); // new entry, not merged

        let all = storage.knowledge().query_all(100).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn flush_creates_new_entry_when_domain_differs() {
        let (pipeline, storage) = setup();

        // First entry on example.com
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "click_failed",
                "Button click failed",
            )
            .with_context(LearningContext {
                domain: Some("example.com".into()),
                ..Default::default()
            }),
        );
        pipeline.flush().unwrap();

        // Second entry: same category+summary but different domain
        pipeline.buffer().insert(
            LearningEntry::new(
                LearningCategory::SiteInteraction,
                "click_failed",
                "Button click failed",
            )
            .with_context(LearningContext {
                domain: Some("other.com".into()),
                ..Default::default()
            }),
        );
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 1); // new entry, not merged

        let all = storage.knowledge().query_all(100).unwrap();
        assert_eq!(all.len(), 2);
    }

    // --- Resurrection pipeline tests ---

    #[test]
    fn test_resurrect_archived_entry_returns_true() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "archived entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Set to archived
        storage
            .knowledge()
            .update_status(&created.id, "archived")
            .unwrap();

        let result = pipeline.resurrect(&created.id).unwrap();
        assert!(result, "should return true for archived entry");

        // Verify the entry is now validated
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");
        assert_eq!(entry.hit_count, 2); // was 1, now incremented
        assert!(entry.last_hit_at.is_some());
    }

    #[test]
    fn test_resurrect_pending_entry_returns_false() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "pending entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Entry is pending by default
        assert_eq!(created.status, "pending");

        let result = pipeline.resurrect(&created.id).unwrap();
        assert!(!result, "should return false for pending entry");

        // Status should remain pending
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "pending");
    }

    #[test]
    fn test_resurrect_validated_entry_returns_false() {
        let (pipeline, storage) = setup();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "validated entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .update_status(&created.id, "validated")
            .unwrap();

        let result = pipeline.resurrect(&created.id).unwrap();
        assert!(!result, "should return false for validated entry");

        // Status should remain validated
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");
    }

    #[test]
    fn test_resurrect_nonexistent_returns_false() {
        let (pipeline, _storage) = setup();

        let result = pipeline.resurrect("K-00000000-000000").unwrap();
        assert!(!result, "should return false for nonexistent entry");
    }

    // --- Metrics recording tests ---

    #[test]
    fn flush_records_metric() {
        let (pipeline, storage) = setup();

        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click_failed",
            "Button click failed",
        ));

        pipeline.flush().unwrap();

        let metrics = storage
            .learning_metrics()
            .query_by_type("flush", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flush_empty_records_zero_metric() {
        let (pipeline, storage) = setup();

        pipeline.flush().unwrap();

        let metrics = storage
            .learning_metrics()
            .query_by_type("flush", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn validate_records_metric() {
        let (pipeline, storage) = setup();

        // Create a qualifying entry
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "metric test entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0,
        };

        pipeline.validate(&thresholds).unwrap();

        let metrics = storage
            .learning_metrics()
            .query_by_type("validation", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn validate_records_zero_metric_when_nothing_qualifies() {
        let (pipeline, storage) = setup();

        let thresholds = ValidationThresholds::default();
        pipeline.validate(&thresholds).unwrap();

        let metrics = storage
            .learning_metrics()
            .query_by_type("validation", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn promote_records_metric() {
        let (pipeline, storage) = setup();

        create_validated_entry(
            &storage,
            "site_interaction",
            None,
            Some("example.com"),
            "Metric test entry",
            0.85,
        );

        let thresholds = test_promotion_thresholds(10);

        pipeline.promote(&thresholds).await.unwrap();

        let metrics = storage
            .learning_metrics()
            .query_by_type("promotion", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn promote_records_metric_with_skipped_counts() {
        let (pipeline, storage) = setup();

        // One entry that will be skipped due to low confidence
        create_validated_entry(
            &storage,
            "site_interaction",
            None,
            None,
            "Low confidence",
            0.3,
        );

        // Use strict effectiveness threshold so 0.3 confidence is below it
        let mut thresholds = test_promotion_thresholds(10);
        thresholds.site_interaction.min_effectiveness = 0.6;

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.skipped_threshold, 1);

        let metrics = storage
            .learning_metrics()
            .query_by_type("promotion", 10)
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 0.0).abs() < f64::EPSILON);
    }

    // --- Pause / Resume / Clear controls tests ---

    #[test]
    fn pause_disables_pipeline() {
        let (pipeline, _) = setup();
        pipeline.pause();

        // flush should return 0 without draining
        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        ));
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 0);

        // validate should return 0
        let thresholds = ValidationThresholds::default();
        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn resume_enables_pipeline() {
        let (pipeline, storage) = setup();

        // Pause then resume
        pipeline.pause();
        pipeline.resume();

        // flush should work normally after resume
        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        ));
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 1);

        // Verify entry made it to SQLite
        let pending = storage.knowledge().query_pending(10).unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn is_enabled_returns_correct_state() {
        let (pipeline, _) = setup();

        // Default: enabled
        assert!(pipeline.is_enabled());

        // After pause: disabled
        pipeline.pause();
        assert!(!pipeline.is_enabled());

        // After resume: enabled again
        pipeline.resume();
        assert!(pipeline.is_enabled());
    }

    #[tokio::test]
    async fn clear_all_removes_everything() {
        let (pipeline, storage) = setup();

        // Insert buffer entries
        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        ));

        // Create knowledge entries
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "test".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Create metrics
        storage
            .learning_metrics()
            .create(
                nevoflux_storage::CreateLearningMetricParams::new("flush", "2026-02-17", 1.0)
                    .with_id("LM-clear-test"),
            )
            .unwrap();

        // Verify data exists
        assert_eq!(pipeline.buffer().len(), 1);
        assert_eq!(storage.knowledge().query_pending(10).unwrap().len(), 1);
        assert_eq!(
            storage
                .learning_metrics()
                .query_by_type("flush", 10)
                .unwrap()
                .len(),
            1
        );

        // Clear all
        pipeline.clear_all().await.unwrap();

        // Verify everything is gone
        assert_eq!(pipeline.buffer().len(), 0);
        assert_eq!(storage.knowledge().query_pending(10).unwrap().len(), 0);
        assert_eq!(
            storage
                .learning_metrics()
                .query_by_type("flush", 10)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn flush_skips_when_paused() {
        let (pipeline, _) = setup();

        // Insert entries
        pipeline.buffer().insert(LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        ));

        // Pause the pipeline
        pipeline.pause();

        // Flush should skip
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 0);

        // Buffer entries should still be there
        assert_eq!(pipeline.buffer().len(), 1);
    }

    #[test]
    fn enabled_flag_returns_shared_arc() {
        let (pipeline, _) = setup();

        let flag = pipeline.enabled_flag();
        assert!(flag.load(Ordering::Relaxed));

        // Pausing pipeline should be visible through the shared flag
        pipeline.pause();
        assert!(!flag.load(Ordering::Relaxed));

        // And resuming too
        pipeline.resume();
        assert!(flag.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn promote_returns_default_when_paused() {
        let (pipeline, storage) = setup();

        // Create a validated entry that would normally be promoted
        create_validated_entry(
            &storage,
            "site_interaction",
            None,
            Some("example.com"),
            "Should not be promoted",
            0.85,
        );

        // Pause the pipeline
        pipeline.pause();

        let thresholds = PromotionThresholds::default();
        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 0);
        assert_eq!(result.skipped_threshold, 0);
        assert_eq!(result.failed, 0);

        // Entry should still be validated (not promoted)
        let entries = storage.knowledge().query_validated(10).unwrap();
        assert_eq!(entries.len(), 1);
    }

    // --- Conflict resolution in validate() tests ---

    #[test]
    fn validate_archives_old_entry_on_direct_contradiction() {
        let (pipeline, storage) = setup();
        let repo = storage.knowledge();

        // Create an existing validated entry
        let old = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Use approach A".into(),
                details: "Strategy A works".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();
        repo.update_status(&old.id, "validated").unwrap();

        // Set subcategory on the old entry so it shares subcategory with the new
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'timeout' WHERE id = ?1",
                    params![old.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Create a pending entry with same category+domain+subcategory
        // but different details (direct contradiction)
        let new_entry = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Use approach B".into(),
                details: "Strategy B works better".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();

        // Set subcategory and qualifying stats on the new entry
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'timeout', hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![new_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.0,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1, "New entry should be validated");

        // Old entry should be archived
        let old_entry = repo.get(&old.id).unwrap().unwrap();
        assert_eq!(old_entry.status, "archived");

        // New entry should be validated
        let new_entry = repo.get(&new_entry.id).unwrap().unwrap();
        assert_eq!(new_entry.status, "validated");
    }

    #[test]
    fn validate_keeps_both_on_strategy_conflict() {
        let (pipeline, storage) = setup();
        let repo = storage.knowledge();

        // Create an existing validated entry
        let old = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Approach A".into(),
                details: "Strategy A".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();
        repo.update_status(&old.id, "validated").unwrap();

        // Set subcategory=login on old
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'login' WHERE id = ?1",
                    params![old.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Create a pending entry with same category+domain but different
        // subcategory and details (strategy conflict: same subject, different
        // approach, different subcategory -> not contradicting)
        let new_entry = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Approach B".into(),
                details: "Strategy B".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();

        // Different subcategory triggers strategy conflict (not direct contradiction)
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'checkout', hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![new_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.0,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1, "New entry should be validated (keep both)");

        // Old entry should still be validated (not archived)
        let old_entry = repo.get(&old.id).unwrap().unwrap();
        assert_eq!(old_entry.status, "validated");

        // New entry should also be validated
        let new_entry = repo.get(&new_entry.id).unwrap().unwrap();
        assert_eq!(new_entry.status, "validated");
    }

    #[test]
    fn validate_rejects_incoming_when_manual_edit_protected() {
        let (pipeline, storage) = setup();
        let repo = storage.knowledge();

        // Create a manually-edited, validated entry
        let manual = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Manual approach".into(),
                details: "Manually curated".into(),
                source_type: Some("manual".into()),
                ..Default::default()
            })
            .unwrap();
        repo.update_status(&manual.id, "validated").unwrap();

        // Create a pending system entry with the same subject
        let system_entry = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "System approach".into(),
                details: "System generated".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();

        // Make it meet thresholds
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![system_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.0,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(
            count, 0,
            "System entry should be rejected (manual edit protected)"
        );

        // System entry should be archived (rejected)
        let sys = repo.get(&system_entry.id).unwrap().unwrap();
        assert_eq!(sys.status, "archived");

        // Manual entry should still be validated
        let man = repo.get(&manual.id).unwrap().unwrap();
        assert_eq!(man.status, "validated");
    }

    #[test]
    fn validate_no_conflict_when_categories_differ() {
        let (pipeline, storage) = setup();
        let repo = storage.knowledge();

        // Create an existing validated entry in a different category
        let old = repo
            .create(CreateKnowledgeParams {
                category: "siteinteraction".into(),
                domain: Some("example.com".into()),
                summary: "Site entry".into(),
                details: "Site details".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();
        repo.update_status(&old.id, "validated").unwrap();

        // Create a pending entry in a different category
        let new_entry = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Tool entry".into(),
                details: "Tool details".into(),
                source_type: Some("system".into()),
                ..Default::default()
            })
            .unwrap();

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET hit_count = 5, confidence = 0.8 WHERE id = ?1",
                    params![new_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.0,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 1, "No conflict across different categories");

        // New entry should be validated
        let entry = repo.get(&new_entry.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");

        // Old entry should still be validated (untouched)
        let old_entry = repo.get(&old.id).unwrap().unwrap();
        assert_eq!(old_entry.status, "validated");
    }

    #[test]
    fn validate_flags_high_value_conflict_for_user() {
        // Both entries must be manual for arbitration (manual vs system hits ManualEditProtected first)
        let (pipeline, storage) = setup();
        let repo = storage.knowledge();

        // Create a high-confidence, high-hit MANUAL entry
        let old = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "High value approach".into(),
                details: "Well-tested strategy".into(),
                source_type: Some("manual".into()),
                ..Default::default()
            })
            .unwrap();
        repo.update_status(&old.id, "validated").unwrap();

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'timeout', hit_count = 100, confidence = 0.95 WHERE id = ?1",
                    params![old.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Create a low-confidence pending MANUAL entry that contradicts
        let new_entry = repo
            .create(CreateKnowledgeParams {
                category: "tooloptimization".into(),
                domain: Some("example.com".into()),
                summary: "Low value approach".into(),
                details: "New untested strategy".into(),
                source_type: Some("manual".into()),
                ..Default::default()
            })
            .unwrap();

        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET subcategory = 'timeout', hit_count = 1, confidence = 0.5 WHERE id = ?1",
                    params![new_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        let thresholds = ValidationThresholds {
            min_occurrences: 1,
            min_confidence: 0.0,
            min_alive_hours: 0,
        };

        let count = pipeline.validate(&thresholds).unwrap();
        assert_eq!(count, 0, "Should be flagged for user, not validated");

        // New entry should still be pending (flagged for user arbitration)
        let entry = repo.get(&new_entry.id).unwrap().unwrap();
        assert_eq!(entry.status, "pending");

        // Old manual entry should be untouched
        let old_entry = repo.get(&old.id).unwrap().unwrap();
        assert_eq!(old_entry.status, "validated");
    }

    // -----------------------------------------------------------------------
    // E2E tests: full pipeline chain
    // -----------------------------------------------------------------------

    /// Full chain E2E test covering the entire learning pipeline:
    /// ToolTraceLearningSource → Collector → Buffer → Flush → Validate → Promote → hot=1
    ///
    /// This test uses real storage with trace span data (simulating a tool
    /// with high failure rate) and verifies the complete data flow from
    /// signal source through to hot knowledge promotion.
    #[tokio::test]
    async fn e2e_source_to_hot_full_pipeline() {
        use crate::learning::collector::LearningCollector;
        use crate::learning::sources::ToolTraceLearningSource;
        use nevoflux_storage::CreateTraceSpanParams;
        use std::sync::atomic::AtomicBool;

        // 1. Setup: in-memory Storage + Pipeline
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let buffer = Arc::new(MemoryBuffer::new(20, Duration::from_secs(30)));
        let pipeline = LearningPipeline::new(
            buffer.clone(),
            storage.clone(),
            std::sync::Arc::new(std::sync::RwLock::new(None)),
        );

        // 2. Insert tool execution spans (simulating a tool with 80% failure rate)
        //    Need >= 3 calls to pass ToolTraceLearningSource.min_calls threshold
        for i in 0..5u32 {
            storage
                .traces()
                .create(CreateTraceSpanParams {
                    session_id: "e2e-session".into(),
                    iteration: i,
                    span_type: "tool_exec".into(),
                    tool_name: Some("flaky_tool".into()),
                    tool_params: None,
                    success: i == 0, // only first call succeeds → 80% failure rate
                    error_code: if i > 0 { Some("TIMEOUT".into()) } else { None },
                    error_msg: if i > 0 {
                        Some("connection timeout".into())
                    } else {
                        None
                    },
                    duration_ms: Some(100),
                })
                .unwrap();
        }

        // 3. Source → Collector → collect entries
        let source = ToolTraceLearningSource::new(storage.clone());
        let mut collector = LearningCollector::new();
        collector.set_enabled(Arc::new(AtomicBool::new(true)));
        collector.register_source(Box::new(source));

        let entries = collector.collect_all();
        assert!(
            !entries.is_empty(),
            "Source should produce entries for tool with 80% failure rate"
        );

        // Verify entries reference the flaky tool
        let has_flaky = entries.iter().any(|e| e.summary.contains("flaky_tool"));
        assert!(has_flaky, "Should have entry about flaky_tool");

        // 4. Insert collected entries into buffer
        for entry in entries {
            buffer.insert(entry);
        }
        assert!(!buffer.is_empty());

        // 5. Flush → knowledge table (pending status)
        let flushed = pipeline.flush().unwrap();
        assert!(
            flushed > 0,
            "Should flush at least one entry to knowledge table"
        );

        let pending = storage.knowledge().query_pending(10).unwrap();
        assert!(
            !pending.is_empty(),
            "Should have pending entries after flush"
        );

        // Verify the flushed entry references flaky_tool
        let flaky_entry = pending
            .iter()
            .find(|e| e.summary.contains("flaky_tool"))
            .expect("Should have a pending entry about flaky_tool");
        assert_eq!(flaky_entry.status, "pending");

        // 6. Adjust hit_count and confidence to meet validation thresholds
        //    (simulating multiple observations over time)
        storage
            .database()
            .with_connection(|conn| {
                conn.execute("UPDATE knowledge SET hit_count = 5, confidence = 0.85", [])?;
                Ok(())
            })
            .unwrap();

        // 7. Validate: pending → validated
        let val_thresholds = ValidationThresholds {
            min_occurrences: 3,
            min_confidence: 0.6,
            min_alive_hours: 0, // skip time constraint for test
        };
        let validated = pipeline.validate(&val_thresholds).unwrap();
        assert!(validated > 0, "Should validate at least one entry");

        // 8. Promote: validated → promoted (hot=1)
        let promo_thresholds = test_promotion_thresholds(10);
        let result = pipeline.promote(&promo_thresholds).await.unwrap();
        assert!(result.promoted > 0, "Should promote at least one entry");

        // 9. Verify final state: hot=1 with hot_summary containing tool name
        let hot = storage.knowledge().list_hot().unwrap();
        assert!(!hot.is_empty(), "Should have hot entries after promotion");

        let hot_flaky = hot
            .iter()
            .find(|e| e.summary.contains("flaky_tool"))
            .expect("Hot entries should include flaky_tool knowledge");
        assert!(hot_flaky.hot);
        assert_eq!(hot_flaky.status, "promoted");
        assert!(hot_flaky.promoted_at.is_some());
        assert!(
            hot_flaky.hot_summary.is_some(),
            "Promoted entry must have a hot_summary"
        );
        assert!(
            hot_flaky
                .hot_summary
                .as_ref()
                .unwrap()
                .contains("flaky_tool"),
            "hot_summary should reference the tool name"
        );
    }

    /// E2E test: verify that promoted hot knowledge with capacity limits
    /// correctly evicts low-confidence entries when the limit is exceeded.
    #[tokio::test]
    async fn e2e_hot_limit_evicts_low_confidence() {
        let (pipeline, storage) = setup();

        // Create 5 validated entries with varying confidence
        for i in 0..5 {
            create_validated_entry(
                &storage,
                "tool_optimization",
                None,
                Some(&format!("tool{}.io", i)),
                &format!("Tool {} optimization tip", i),
                0.5 + (i as f64) * 0.1, // 0.5, 0.6, 0.7, 0.8, 0.9
            );
        }

        // Promote all with hot_limit_tool_optimization = 3
        let mut thresholds = test_promotion_thresholds(10);
        thresholds.hot_limit_tool_optimization = 3;

        let result = pipeline.promote(&thresholds).await.unwrap();
        assert_eq!(result.promoted, 5, "All 5 should be promoted initially");

        // After enforcement, only 3 should remain hot (highest confidence)
        let hot = storage
            .knowledge()
            .list_hot_by_category("tool_optimization")
            .unwrap();
        assert_eq!(
            hot.len(),
            3,
            "Only 3 should remain hot after limit enforcement"
        );

        // Verify the kept entries have the highest confidence
        let confidences: Vec<f64> = hot.iter().map(|e| e.confidence).collect();
        assert!(
            confidences.iter().all(|&c| c >= 0.7),
            "Only entries with confidence >= 0.7 should remain hot"
        );
    }
}
