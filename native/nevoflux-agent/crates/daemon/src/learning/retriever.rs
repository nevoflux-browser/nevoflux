// crates/daemon/src/learning/retriever.rs
//
// Knowledge retriever with session-scoped cache. Provides the read path for the
// learning system by caching the five soul documents and querying SQLite for
// relevant knowledge entries filtered by domain and category.

use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use nevoflux_storage::{cosine_similarity, Knowledge, SiteAdaptation, Storage};

use crate::wasm::services::{get_embedding, SharedEmbedding};

use super::soul::manager::FiveDocCache;

/// Configuration for knowledge retrieval.
#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    /// Maximum number of knowledge entries to return per query.
    pub top_k: usize,
    /// Minimum decay score for an entry to be included in results.
    pub min_decay_score: f64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_decay_score: 0.05, // Exclude archived entries
        }
    }
}

/// Result of a knowledge retrieval query.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    /// Relevant knowledge entries from SQLite, sorted by relevance score descending.
    pub entries: Vec<ScoredKnowledge>,
    /// Site-specific adaptation entries from the site_adaptations table.
    pub site_adaptations: Vec<SiteAdaptation>,
}

/// A knowledge entry annotated with its computed relevance score.
#[derive(Debug, Clone)]
pub struct ScoredKnowledge {
    pub entry: Knowledge,
    pub relevance_score: f64,
}

/// Compute the composite relevance score for a knowledge entry.
///
/// Four factors:
/// - `category_match`: 1.0 if exact match, 0.3 otherwise (no query category = 0.3)
/// - `domain_match`: 1.0 if exact domain match, 0.5 if universal (entry domain is None)
///   or no query domain filter, 0.1 if domain mismatch
/// - `decay`: exponential decay based on age, category half-life, effectiveness, hit count
/// - `confidence`: the entry's confidence score
///
/// Final score = category_match * domain_match * decay * confidence
pub fn relevance_score(
    entry: &Knowledge,
    query_domain: Option<&str>,
    query_category: Option<&str>,
    now: DateTime<Utc>,
) -> f64 {
    // Category matching
    let category_match = match query_category {
        Some(qc) if entry.category == qc => 1.0,
        Some(_) => 0.3,
        None => 0.3, // No category filter = partial match
    };

    // Domain matching
    let domain_match = match (entry.domain.as_deref(), query_domain) {
        (Some(ed), Some(qd)) if ed == qd => 1.0, // Exact domain match
        (None, _) => 0.5,                        // Universal knowledge
        (Some(_), None) => 0.5,                  // No domain filter
        _ => 0.1,                                // Domain mismatch
    };

    // Decay calculation: parse last_hit_at (or fall back to updated_at)
    let last_hit = entry
        .last_hit_at
        .as_deref()
        .or(Some(entry.updated_at.as_str()))
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or(now); // If unparseable, treat as fresh

    let decay = super::decay::calculate_decay(
        last_hit,
        &entry.category,
        entry.effectiveness,
        entry.hit_count as u32,
        now,
    );

    category_match * domain_match * decay * entry.confidence
}

/// Compute relevance score with optional semantic similarity.
///
/// When both a query embedding and entry embedding are available, uses a
/// semantic-weighted formula (40% semantic, 20% category, 15% domain,
/// 15% decay, 10% confidence). Otherwise falls back to a structural-only
/// additive formula (30% category, 30% domain, 25% decay, 15% confidence).
pub fn relevance_score_hybrid(
    entry: &Knowledge,
    query_domain: Option<&str>,
    query_category: Option<&str>,
    query_embedding: Option<&[f32]>,
    now: DateTime<Utc>,
) -> f64 {
    // Category matching
    let category_match = match query_category {
        Some(qc) if qc == entry.category => 1.0,
        Some(_) => 0.3,
        None => 0.3,
    };

    // Domain matching
    let domain_match = match (query_domain, &entry.domain) {
        (Some(qd), Some(ed)) if qd == ed => 1.0,
        (None, Some(_)) | (Some(_), None) => 0.5,
        (None, None) => 0.5,
        _ => 0.1,
    };

    // Decay calculation
    let decay = super::decay::calculate_decay(
        entry
            .last_hit_at
            .as_deref()
            .or(Some(entry.updated_at.as_str()))
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or(now),
        &entry.category,
        entry.effectiveness,
        entry.hit_count as u32,
        now,
    );

    let confidence = entry.confidence;

    // Semantic similarity
    let semantic = match (query_embedding, &entry.embedding) {
        (Some(q), Some(e)) => cosine_similarity(q, e) as f64,
        _ => 0.0,
    };
    let has_semantic = query_embedding.is_some() && entry.embedding.is_some();

    if has_semantic {
        // Semantic-weighted formula
        0.40 * semantic
            + 0.20 * category_match
            + 0.15 * domain_match
            + 0.15 * decay
            + 0.10 * confidence
    } else {
        // Structural-only formula (normalized)
        0.30 * category_match + 0.30 * domain_match + 0.25 * decay + 0.15 * confidence
    }
}

/// Retrieves relevant knowledge for a given context.
///
/// Holds a session-scoped cache of the five soul documents (loaded at session
/// start) and queries SQLite for knowledge entries matching domain/category
/// filters. Entries are scored using lazy decay and confidence, then filtered
/// and truncated to the configured top-K.
pub struct KnowledgeRetriever {
    soul_cache: RwLock<Arc<FiveDocCache>>,
    storage: Arc<Storage>,
    config: RetrievalConfig,
    embedding: SharedEmbedding,
}

impl KnowledgeRetriever {
    /// Create a new retriever initialized from a soul cache and storage handle.
    pub fn new(soul_cache: Arc<FiveDocCache>, storage: Arc<Storage>) -> Self {
        Self {
            soul_cache: RwLock::new(soul_cache),
            storage,
            config: RetrievalConfig::default(),
            embedding: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Create a retriever with custom configuration.
    pub fn with_config(
        soul_cache: Arc<FiveDocCache>,
        storage: Arc<Storage>,
        config: RetrievalConfig,
    ) -> Self {
        Self {
            soul_cache: RwLock::new(soul_cache),
            storage,
            config,
            embedding: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Attach a shared embedding provider for semantic scoring in `retrieve()`.
    pub fn with_embedding(mut self, shared: SharedEmbedding) -> Self {
        self.embedding = shared;
        self
    }

    /// Replace the cached soul documents with a fresh snapshot.
    ///
    /// This method is safe to call through a shared `&self` reference
    /// thanks to interior mutability.
    pub fn invalidate_cache(&self, new_cache: Arc<FiveDocCache>) {
        *self.soul_cache.write().unwrap() = new_cache;
    }

    /// Update the soul cache from a new `FiveDocCache` value.
    ///
    /// Thread-safe; can be called through `Arc<KnowledgeRetriever>`.
    pub fn update_soul_cache(&self, cache: FiveDocCache) {
        *self.soul_cache.write().unwrap() = Arc::new(cache);
    }

    /// Get a snapshot of the cached soul documents.
    pub fn soul_cache(&self) -> Arc<FiveDocCache> {
        self.soul_cache.read().unwrap().clone()
    }

    /// Retrieve relevant knowledge for a given query, domain, and category.
    ///
    /// 1. Optionally generates a query embedding for semantic scoring.
    /// 2. Queries SQLite for knowledge entries matching the domain (if provided).
    ///    When no domain is given, queries validated entries instead.
    /// 3. Computes a hybrid relevance score combining semantic similarity with
    ///    structural features (category, domain, decay, confidence).
    /// 4. Filters out archived entries and those below `min_decay_score`.
    /// 5. Sorts by relevance descending and truncates to `top_k`.
    /// 6. Queries site adaptations for the domain (if provided).
    pub async fn retrieve(
        &self,
        query: &str,
        domain: Option<&str>,
        category: Option<&str>,
    ) -> crate::error::Result<RetrievalResult> {
        // Generate query embedding (failure = None, graceful degradation)
        let query_emb = if !query.is_empty() {
            if let Some(provider) = get_embedding(&self.embedding) {
                provider.embed(query).await.ok()
            } else {
                None
            }
        } else {
            None
        };

        // 1. Query knowledge entries by domain (if provided)
        let candidates = if let Some(d) = domain {
            self.storage.knowledge().query_by_domain(d, 100)?
        } else {
            // No domain filter -- query validated entries as a starting point
            self.storage.knowledge().query_validated(100)?
        };

        // 2. Compute relevance scores and filter
        let now = chrono::Utc::now();
        let mut scored: Vec<ScoredKnowledge> = candidates
            .into_iter()
            .filter(|e| e.status != "archived") // Exclude archived
            .map(|entry| {
                let score =
                    relevance_score_hybrid(&entry, domain, category, query_emb.as_deref(), now);
                ScoredKnowledge {
                    entry,
                    relevance_score: score,
                }
            })
            .filter(|sk| sk.relevance_score >= self.config.min_decay_score)
            .collect();

        // 3. Sort by relevance descending
        scored.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 4. Take top-K
        scored.truncate(self.config.top_k);

        // 5. Query site adaptations
        let site_adaptations = if let Some(d) = domain {
            self.storage.site_adaptations().query_by_domain(d, 10)?
        } else {
            Vec::new()
        };

        Ok(RetrievalResult {
            entries: scored,
            site_adaptations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use nevoflux_storage::{CreateKnowledgeParams, Storage};

    fn make_cache() -> Arc<FiveDocCache> {
        Arc::new(FiveDocCache {
            identity_raw: "# Identity".to_string(),
            soul_raw: "# Soul".to_string(),
            user_raw: "# User".to_string(),
            tools_raw: "# Tools".to_string(),
            agents_raw: "# Agents".to_string(),
            last_parsed_at: Utc::now(),
        })
    }

    fn create_test_entry(
        storage: &Storage,
        category: &str,
        domain: Option<&str>,
        summary: &str,
    ) -> Knowledge {
        let entry = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: category.to_string(),
                domain: domain.map(|d| d.to_string()),
                summary: summary.to_string(),
                details: "test details".to_string(),
                ..Default::default()
            })
            .unwrap();
        // Default status is "pending"; queries may need validated status.
        entry
    }

    #[tokio::test]
    async fn retriever_empty_database_returns_empty() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();
        let retriever = KnowledgeRetriever::new(cache, storage);

        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();
        assert!(result.entries.is_empty());
        assert!(result.site_adaptations.is_empty());
    }

    #[tokio::test]
    async fn retriever_returns_matching_entries() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create an entry for github.com
        let entry = create_test_entry(
            &storage,
            "site_interaction",
            Some("github.com"),
            "github uses data-testid",
        );
        // query_by_domain returns entries regardless of status, so this should work
        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));

        let result = retriever
            .retrieve("", Some("github.com"), None)
            .await
            .unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].entry.id, entry.id);
    }

    #[tokio::test]
    async fn retriever_filters_archived_entries() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create an entry and archive it
        let entry = create_test_entry(
            &storage,
            "site_interaction",
            Some("example.com"),
            "archived entry",
        );
        storage
            .knowledge()
            .update_status(&entry.id, "archived")
            .unwrap();

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();
        assert!(
            result.entries.is_empty(),
            "archived entries should be excluded"
        );
    }

    #[tokio::test]
    async fn retriever_respects_top_k_limit() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create more entries than top_k (default = 5)
        for i in 0..8 {
            create_test_entry(
                &storage,
                "site_interaction",
                Some("example.com"),
                &format!("entry {}", i),
            );
        }

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();
        assert!(
            result.entries.len() <= 5,
            "should respect top_k=5, got {}",
            result.entries.len()
        );
    }

    #[tokio::test]
    async fn retriever_sorts_by_relevance_descending() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create entries; they all start fresh so should have similar decay,
        // but the scoring formula is decay * confidence. We can manipulate
        // confidence by not having a direct setter, but we can verify ordering.
        for i in 0..3 {
            create_test_entry(
                &storage,
                "site_interaction",
                Some("example.com"),
                &format!("entry {}", i),
            );
        }

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();

        // Verify descending order
        for window in result.entries.windows(2) {
            assert!(
                window[0].relevance_score >= window[1].relevance_score,
                "entries should be sorted by relevance descending: {} >= {}",
                window[0].relevance_score,
                window[1].relevance_score,
            );
        }
    }

    #[tokio::test]
    async fn retriever_no_domain_queries_validated() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create a pending entry (should not appear in validated query)
        create_test_entry(&storage, "site_interaction", None, "pending entry");

        // Create a validated entry
        let validated = create_test_entry(&storage, "tool_optimization", None, "validated entry");
        storage
            .knowledge()
            .update_status(&validated.id, "validated")
            .unwrap();

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever.retrieve("", None, None).await.unwrap();

        // Only the validated entry should be returned
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].entry.id, validated.id);
    }

    #[test]
    fn invalidate_cache_updates_cache() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();
        let retriever = KnowledgeRetriever::new(cache, storage);

        assert_eq!(retriever.soul_cache().identity_raw, "# Identity");

        // Build a new cache with different content
        let new_cache = Arc::new(FiveDocCache {
            identity_raw: "# New Identity".to_string(),
            soul_raw: "# New Soul".to_string(),
            user_raw: "# New User".to_string(),
            tools_raw: "# New Tools".to_string(),
            agents_raw: "# New Agents".to_string(),
            last_parsed_at: Utc::now(),
        });

        retriever.invalidate_cache(new_cache);
        assert_eq!(retriever.soul_cache().identity_raw, "# New Identity");
    }

    #[tokio::test]
    async fn retriever_with_custom_config() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create entries
        for i in 0..5 {
            create_test_entry(
                &storage,
                "site_interaction",
                Some("example.com"),
                &format!("entry {}", i),
            );
        }

        let config = RetrievalConfig {
            top_k: 2,
            min_decay_score: 0.05,
        };
        let retriever = KnowledgeRetriever::with_config(cache, Arc::clone(&storage), config);

        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();
        assert!(
            result.entries.len() <= 2,
            "custom top_k=2 should limit results, got {}",
            result.entries.len()
        );
    }

    #[tokio::test]
    async fn retriever_includes_site_adaptations() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create a site adaptation
        use nevoflux_storage::CreateSiteAdaptationParams;
        storage
            .site_adaptations()
            .create(
                CreateSiteAdaptationParams::new(
                    "example.com",
                    "selector_result",
                    r#"{"selector": ".content"}"#,
                )
                .with_id("SA-test01"),
            )
            .unwrap();

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();

        assert_eq!(result.site_adaptations.len(), 1);
        assert_eq!(result.site_adaptations[0].id, "SA-test01");
    }

    #[tokio::test]
    async fn retriever_no_domain_returns_no_site_adaptations() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create a site adaptation (should not be returned without domain)
        use nevoflux_storage::CreateSiteAdaptationParams;
        storage
            .site_adaptations()
            .create(CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r#"{}"#,
            ))
            .unwrap();

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever.retrieve("", None, None).await.unwrap();

        assert!(
            result.site_adaptations.is_empty(),
            "no domain = no site adaptations"
        );
    }

    #[test]
    fn retrieval_config_default_values() {
        let config = RetrievalConfig::default();
        assert_eq!(config.top_k, 5);
        assert!((config.min_decay_score - 0.05).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn relevance_score_is_positive_for_fresh_entries() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        create_test_entry(
            &storage,
            "site_interaction",
            Some("example.com"),
            "fresh entry",
        );

        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("", Some("example.com"), None)
            .await
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert!(
            result.entries[0].relevance_score > 0.0,
            "fresh entry should have positive relevance, got {}",
            result.entries[0].relevance_score,
        );
    }

    // --- Tests for the public relevance_score() function ---

    /// Build a Knowledge entry in-memory without touching the database.
    /// Uses the current time for timestamps so decay is ~1.0 for fresh entries.
    fn make_knowledge(
        category: &str,
        domain: Option<&str>,
        confidence: f64,
        hit_count: i64,
    ) -> Knowledge {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        Knowledge {
            id: "K-test-000001".to_string(),
            category: category.to_string(),
            subcategory: None,
            domain: domain.map(|d| d.to_string()),
            summary: "test summary".to_string(),
            details: "test details".to_string(),
            resolution: None,
            confidence,
            hit_count,
            success_count: 0,
            fail_count: 0,
            effectiveness: 0.5,
            priority: "medium".to_string(),
            status: "validated".to_string(),
            source_ids: None,
            related_ids: None,
            tags: None,
            privacy_level: "internal".to_string(),
            promotion_target: None,
            promoted_section: None,
            source_type: "system".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_hit_at: Some(now),
            promoted_at: None,
            embedding: None,
            hot: false,
            hot_summary: None,
        }
    }

    /// Build a Knowledge entry with a specific last_hit_at / updated_at timestamp.
    fn make_knowledge_with_dates(
        category: &str,
        domain: Option<&str>,
        confidence: f64,
        hit_count: i64,
        timestamp: String,
    ) -> Knowledge {
        Knowledge {
            id: "K-test-000002".to_string(),
            category: category.to_string(),
            subcategory: None,
            domain: domain.map(|d| d.to_string()),
            summary: "test summary".to_string(),
            details: "test details".to_string(),
            resolution: None,
            confidence,
            hit_count,
            success_count: 0,
            fail_count: 0,
            effectiveness: 0.5,
            priority: "medium".to_string(),
            status: "validated".to_string(),
            source_ids: None,
            related_ids: None,
            tags: None,
            privacy_level: "internal".to_string(),
            promotion_target: None,
            promoted_section: None,
            source_type: "system".to_string(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            last_hit_at: Some(timestamp),
            promoted_at: None,
            embedding: None,
            hot: false,
            hot_summary: None,
        }
    }

    #[test]
    fn exact_match_scores_highest() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let score = relevance_score(
            &entry,
            Some("example.com"),
            Some("site_interaction"),
            Utc::now(),
        );
        // category_match=1.0, domain_match=1.0, decay~1.0, confidence=0.9
        assert!(score > 0.8, "exact match should score > 0.8, got {score}");
    }

    #[test]
    fn domain_mismatch_reduces_score() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let now = Utc::now();
        let exact = relevance_score(&entry, Some("example.com"), Some("site_interaction"), now);
        let mismatch = relevance_score(&entry, Some("other.com"), Some("site_interaction"), now);
        assert!(
            exact > mismatch,
            "exact domain should score higher: exact={exact}, mismatch={mismatch}"
        );
    }

    #[test]
    fn category_mismatch_reduces_score() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let now = Utc::now();
        let exact = relevance_score(&entry, Some("example.com"), Some("site_interaction"), now);
        let mismatch = relevance_score(&entry, Some("example.com"), Some("tool_optimization"), now);
        assert!(
            exact > mismatch,
            "exact category should score higher: exact={exact}, mismatch={mismatch}"
        );
    }

    #[test]
    fn universal_knowledge_gets_half_domain_match() {
        let entry = make_knowledge("site_interaction", None, 0.9, 10);
        let now = Utc::now();
        let score = relevance_score(&entry, Some("example.com"), Some("site_interaction"), now);
        // domain_match should be 0.5 for universal knowledge
        let exact_entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let exact_score = relevance_score(
            &exact_entry,
            Some("example.com"),
            Some("site_interaction"),
            now,
        );
        assert!(
            exact_score > score,
            "exact domain should beat universal: exact={exact_score}, universal={score}"
        );
        assert!(score > 0.0, "universal knowledge should still score > 0");
    }

    #[test]
    fn old_entry_scores_lower_due_to_decay() {
        let now = Utc::now();
        let old_timestamp = (now - chrono::Duration::days(90))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let entry = make_knowledge_with_dates(
            "site_interaction",
            Some("example.com"),
            0.9,
            10,
            old_timestamp,
        );
        let score = relevance_score(&entry, Some("example.com"), Some("site_interaction"), now);

        let fresh = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let fresh_score =
            relevance_score(&fresh, Some("example.com"), Some("site_interaction"), now);

        assert!(
            fresh_score > score,
            "fresh entry should score higher than 90-day-old: fresh={fresh_score}, old={score}"
        );
    }

    #[test]
    fn no_query_filters_gives_partial_match() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let score = relevance_score(&entry, None, None, Utc::now());
        // category_match=0.3 (no query category), domain_match=0.5 (no domain filter)
        assert!(score > 0.0, "should be positive, got {score}");
        assert!(
            score < 0.5,
            "partial match without filters should be < 0.5, got {score}"
        );
    }

    #[test]
    fn score_is_always_non_negative() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.0, 0);
        let score = relevance_score(
            &entry,
            Some("other.com"),
            Some("tool_optimization"),
            Utc::now(),
        );
        assert!(score >= 0.0, "score should never be negative, got {score}");
    }

    #[test]
    fn no_domain_filter_treats_domain_entry_as_half_match() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let now = Utc::now();
        // No domain filter => domain_match should be 0.5
        let no_filter = relevance_score(&entry, None, Some("site_interaction"), now);
        // Exact domain => domain_match should be 1.0
        let exact = relevance_score(&entry, Some("example.com"), Some("site_interaction"), now);
        assert!(
            exact > no_filter,
            "exact domain should beat no-filter: exact={exact}, no_filter={no_filter}"
        );
        // No filter should be approximately half of exact (both have same category/decay/conf)
        let ratio = no_filter / exact;
        assert!(
            (ratio - 0.5).abs() < 0.01,
            "no-domain-filter / exact-domain ratio should be ~0.5, got {ratio}"
        );
    }

    #[test]
    fn domain_mismatch_is_worse_than_universal() {
        let now = Utc::now();
        // Entry with specific domain that does NOT match the query
        let mismatched = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let mismatch_score = relevance_score(
            &mismatched,
            Some("other.com"),
            Some("site_interaction"),
            now,
        );

        // Universal entry (domain = None)
        let universal = make_knowledge("site_interaction", None, 0.9, 10);
        let universal_score =
            relevance_score(&universal, Some("other.com"), Some("site_interaction"), now);

        assert!(
            universal_score > mismatch_score,
            "universal (0.5) should beat domain mismatch (0.1): universal={universal_score}, mismatch={mismatch_score}"
        );
    }

    #[test]
    fn low_confidence_reduces_score() {
        let now = Utc::now();
        let high_conf = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let low_conf = make_knowledge("site_interaction", Some("example.com"), 0.1, 10);

        let high_score = relevance_score(
            &high_conf,
            Some("example.com"),
            Some("site_interaction"),
            now,
        );
        let low_score = relevance_score(
            &low_conf,
            Some("example.com"),
            Some("site_interaction"),
            now,
        );

        assert!(
            high_score > low_score,
            "higher confidence should yield higher score: high={high_score}, low={low_score}"
        );
    }

    // --- Tests for relevance_score_hybrid() ---

    #[test]
    fn relevance_score_hybrid_with_semantic_boost() {
        let mut entry = make_knowledge("tool_optimization", Some("example.com"), 0.9, 10);
        entry.embedding = Some(vec![1.0, 0.0, 0.0]);

        let query_emb = vec![1.0, 0.0, 0.0]; // identical = cosine 1.0

        let score_semantic = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("tool_optimization"),
            Some(&query_emb),
            Utc::now(),
        );

        let score_no_semantic = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("tool_optimization"),
            None,
            Utc::now(),
        );

        assert!(
            score_semantic > score_no_semantic,
            "Semantic match should boost score: {} vs {}",
            score_semantic,
            score_no_semantic
        );
    }

    #[test]
    fn relevance_score_hybrid_low_similarity_reduces_score() {
        let mut entry = make_knowledge("tool_optimization", Some("example.com"), 0.9, 10);
        entry.embedding = Some(vec![1.0, 0.0, 0.0]);

        // Orthogonal vector = 0 similarity
        let query_emb = vec![0.0, 1.0, 0.0];

        let score = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("tool_optimization"),
            Some(&query_emb),
            Utc::now(),
        );

        // With 0 semantic similarity, the 0.40*0 drags score down
        assert!(
            score < 0.7,
            "Low semantic match should reduce score: {}",
            score
        );
    }

    #[test]
    fn relevance_score_hybrid_degrades_to_structural() {
        let entry = make_knowledge("tool_optimization", Some("example.com"), 0.9, 10);
        // No embedding on entry

        let score_with_query = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("tool_optimization"),
            Some(&[1.0, 0.0, 0.0]),
            Utc::now(),
        );
        let score_without = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("tool_optimization"),
            None,
            Utc::now(),
        );

        assert!(
            (score_with_query - score_without).abs() < 0.01,
            "Without entry embedding, should use structural formula regardless of query embedding: {} vs {}",
            score_with_query,
            score_without
        );
    }

    #[test]
    fn relevance_score_hybrid_structural_only_is_positive() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let score = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("site_interaction"),
            None,
            Utc::now(),
        );
        assert!(
            score > 0.5,
            "structural-only exact match should score > 0.5, got {score}"
        );
    }

    #[test]
    fn relevance_score_hybrid_domain_mismatch() {
        let entry = make_knowledge("site_interaction", Some("example.com"), 0.9, 10);
        let now = Utc::now();
        let exact = relevance_score_hybrid(
            &entry,
            Some("example.com"),
            Some("site_interaction"),
            None,
            now,
        );
        let mismatch = relevance_score_hybrid(
            &entry,
            Some("other.com"),
            Some("site_interaction"),
            None,
            now,
        );
        assert!(
            exact > mismatch,
            "exact domain should score higher in hybrid: exact={exact}, mismatch={mismatch}"
        );
    }

    // -----------------------------------------------------------------------
    // E2E: hot knowledge retrieval
    // -----------------------------------------------------------------------

    /// E2E test: promoted hot knowledge entries are retrievable and ranked
    /// above non-hot entries, while archived entries are excluded.
    #[tokio::test]
    async fn e2e_hot_knowledge_appears_in_retrieval() {
        use rusqlite::params;

        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let cache = make_cache();

        // Create a hot (promoted) entry for github.com
        let hot_entry = create_test_entry(
            &storage,
            "site_interaction",
            Some("github.com"),
            "Use data-testid selectors on GitHub",
        );
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET status = 'promoted', hot = 1, \
                     confidence = 0.95, hit_count = 20, \
                     hot_summary = '[github.com] Use data-testid selectors' \
                     WHERE id = ?1",
                    params![hot_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Create a non-hot validated entry for github.com (lower confidence)
        let normal_entry = create_test_entry(
            &storage,
            "site_interaction",
            Some("github.com"),
            "GitHub has a search bar",
        );
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET status = 'validated', \
                     confidence = 0.5, hit_count = 3 \
                     WHERE id = ?1",
                    params![normal_entry.id],
                )?;
                Ok(())
            })
            .unwrap();

        // Create an archived entry (should NOT appear)
        let archived_entry = create_test_entry(
            &storage,
            "site_interaction",
            Some("github.com"),
            "Outdated GitHub selector pattern",
        );
        storage
            .knowledge()
            .update_status(&archived_entry.id, "archived")
            .unwrap();

        // Retrieve
        let retriever = KnowledgeRetriever::new(cache, Arc::clone(&storage));
        let result = retriever
            .retrieve("selectors", Some("github.com"), Some("site_interaction"))
            .await
            .unwrap();

        // Should have at least 2 entries (hot + normal), but NOT the archived one
        assert!(
            result.entries.len() >= 2,
            "Should retrieve hot and validated entries, got {}",
            result.entries.len()
        );

        // Archived entry must not appear
        let has_archived = result
            .entries
            .iter()
            .any(|sk| sk.entry.id == archived_entry.id);
        assert!(
            !has_archived,
            "Archived entries must be excluded from retrieval"
        );

        // Hot entry should be present
        let has_hot = result.entries.iter().any(|sk| sk.entry.id == hot_entry.id);
        assert!(has_hot, "Hot (promoted) entry should appear in results");

        // The hot entry should rank higher (higher confidence → higher score)
        let hot_pos = result
            .entries
            .iter()
            .position(|sk| sk.entry.id == hot_entry.id);
        let normal_pos = result
            .entries
            .iter()
            .position(|sk| sk.entry.id == normal_entry.id);
        if let (Some(hp), Some(np)) = (hot_pos, normal_pos) {
            assert!(
                hp < np,
                "Hot entry (confidence=0.95) should rank above normal entry (confidence=0.5)"
            );
        }
    }
}
