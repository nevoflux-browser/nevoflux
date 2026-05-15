use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::source::LearningSource;
use super::types::{LearningEntry, PrivacyLevel};
use tracing::{debug, info};

/// Collects learning entries from registered sources,
/// deduplicates, filters, rate-limits, and outputs them.
pub struct LearningCollector {
    sources: Vec<Box<dyn LearningSource>>,
    domain_blacklist: Vec<String>,
    enabled: Option<Arc<AtomicBool>>,
    /// Maximum entries per (domain, source_event) key per hour.
    rate_limit_per_hour: u32,
    /// Sliding window of entry timestamps, keyed by (domain, source_event).
    rate_window: HashMap<(String, String), Vec<Instant>>,
}

impl Default for LearningCollector {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            domain_blacklist: Vec::new(),
            enabled: None,
            rate_limit_per_hour: 5,
            rate_window: HashMap::new(),
        }
    }
}

impl LearningCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of entries per (domain, trigger) key per hour.
    pub fn set_rate_limit(&mut self, limit: u32) {
        self.rate_limit_per_hour = limit;
    }

    /// Set the shared enabled flag from the `LearningPipeline`.
    ///
    /// When set, `collect_all()` will check this flag and return an empty
    /// `Vec` if the flag is `false`.
    pub fn set_enabled(&mut self, flag: Arc<AtomicBool>) {
        self.enabled = Some(flag);
    }

    pub fn register_source(&mut self, source: Box<dyn LearningSource>) {
        self.sources.push(source);
    }

    pub fn set_domain_blacklist(&mut self, blacklist: Vec<String>) {
        self.domain_blacklist = blacklist;
    }

    /// Collect entries from all registered sources.
    ///
    /// Returns an empty `Vec` if the pipeline is disabled.
    pub fn collect_all(&mut self) -> Vec<LearningEntry> {
        if let Some(ref enabled) = self.enabled {
            if !enabled.load(Ordering::Relaxed) {
                info!("Learning collection disabled, skipping cycle");
                return Vec::new();
            }
        }
        let mut all_entries = Vec::new();

        for source in &self.sources {
            let entries = source.collect();
            debug!(
                source = source.source_name(),
                count = entries.len(),
                "Collected learning entries"
            );
            all_entries.extend(entries);
        }

        let filtered = self.filter_privacy(all_entries);
        let filtered = self.filter_blacklisted_domains(filtered);
        let deduped = self.dedup(filtered);
        let rate_limited = self.apply_rate_limit(deduped);

        info!(
            count = rate_limited.len(),
            "Learning collector cycle complete"
        );

        rate_limited
    }

    /// Filter out private entries -- they must never be persisted.
    pub fn filter_privacy(&self, entries: Vec<LearningEntry>) -> Vec<LearningEntry> {
        entries
            .into_iter()
            .filter(|e| e.privacy_level != PrivacyLevel::Private)
            .collect()
    }

    /// Filter out entries from blacklisted domains.
    fn filter_blacklisted_domains(&self, entries: Vec<LearningEntry>) -> Vec<LearningEntry> {
        if self.domain_blacklist.is_empty() {
            return entries;
        }
        entries
            .into_iter()
            .filter(|e| {
                if let Some(domain) = &e.context.domain {
                    !self.domain_blacklist.iter().any(|b| domain.contains(b))
                } else {
                    true
                }
            })
            .collect()
    }

    /// Deduplicate entries by (domain, selector, category, source_event) exact match.
    /// Merges duplicates by incrementing occurrence_count.
    pub fn dedup(&self, entries: Vec<LearningEntry>) -> Vec<LearningEntry> {
        let mut result: Vec<LearningEntry> = Vec::new();

        for entry in entries {
            if let Some(existing) = result.iter_mut().find(|e| Self::is_duplicate(e, &entry)) {
                existing.occurrence_count += 1;
                existing.last_seen_at = entry.last_seen_at;
                if entry.confidence > existing.confidence {
                    existing.confidence = entry.confidence;
                }
            } else {
                result.push(entry);
            }
        }

        result
    }

    /// Filter entries that exceed the per-(domain, trigger) hourly rate limit.
    ///
    /// Uses a sliding window: timestamps older than 1 hour are pruned, and any
    /// entry whose key already has `rate_limit_per_hour` timestamps in the
    /// window is dropped.
    fn apply_rate_limit(&mut self, entries: Vec<LearningEntry>) -> Vec<LearningEntry> {
        if self.rate_limit_per_hour == 0 {
            return entries;
        }
        let now = Instant::now();
        let one_hour = std::time::Duration::from_secs(3600);

        // Prune expired timestamps from all keys
        self.rate_window.retain(|_, timestamps| {
            timestamps.retain(|t| now.duration_since(*t) < one_hour);
            !timestamps.is_empty()
        });

        let limit = self.rate_limit_per_hour as usize;
        let mut result = Vec::with_capacity(entries.len());

        for entry in entries {
            let domain = entry.context.domain.clone().unwrap_or_default();
            let key = (domain, entry.source_event.clone());
            let timestamps = self.rate_window.entry(key).or_default();

            if timestamps.len() < limit {
                timestamps.push(now);
                result.push(entry);
            } else {
                debug!(
                    domain = entry.context.domain.as_deref().unwrap_or("none"),
                    source_event = entry.source_event,
                    "Rate-limited learning entry"
                );
            }
        }

        result
    }

    fn is_duplicate(a: &LearningEntry, b: &LearningEntry) -> bool {
        a.category == b.category
            && a.context.domain == b.context.domain
            && a.context.selector == b.context.selector
            && a.source_event == b.source_event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::source::LearningSource;
    use crate::learning::types::*;
    use std::sync::{Arc, Mutex};

    struct FakeSource {
        entries: Arc<Mutex<Vec<LearningEntry>>>,
    }

    impl LearningSource for FakeSource {
        fn source_name(&self) -> &str {
            "fake"
        }

        fn collect(&self) -> Vec<LearningEntry> {
            let mut entries = self.entries.lock().unwrap();
            entries.drain(..).collect()
        }
    }

    #[test]
    fn collector_registers_source_and_collects() {
        let entries = Arc::new(Mutex::new(vec![LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        )]));

        let source = FakeSource {
            entries: entries.clone(),
        };

        let mut collector = LearningCollector::new();
        collector.register_source(Box::new(source));

        let collected = collector.collect_all();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].source_event, "click");
    }

    #[test]
    fn collector_dedup_exact_match() {
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

        let collector = LearningCollector::new();
        let deduped = collector.dedup(vec![entry1, entry2]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].occurrence_count, 2);
    }

    #[test]
    fn collector_no_dedup_different_domains() {
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

        let collector = LearningCollector::new();
        let deduped = collector.dedup(vec![entry1, entry2]);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn collector_filters_private_entries() {
        let entry = LearningEntry::new(
            LearningCategory::UserPreference,
            "password",
            "user typed password",
        )
        .with_privacy(PrivacyLevel::Private);

        let collector = LearningCollector::new();
        let filtered = collector.filter_privacy(vec![entry]);
        assert!(filtered.is_empty());
    }

    #[test]
    fn collector_keeps_non_private_entries() {
        let entries = vec![
            LearningEntry::new(LearningCategory::SiteInteraction, "a", "s")
                .with_privacy(PrivacyLevel::Public),
            LearningEntry::new(LearningCategory::SiteInteraction, "b", "s")
                .with_privacy(PrivacyLevel::Internal),
            LearningEntry::new(LearningCategory::SiteInteraction, "c", "s")
                .with_privacy(PrivacyLevel::Sensitive),
        ];

        let collector = LearningCollector::new();
        let filtered = collector.filter_privacy(entries);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn collector_filters_blacklisted_domains() {
        let entries = Arc::new(Mutex::new(vec![
            LearningEntry::new(LearningCategory::SiteInteraction, "a", "s").with_context(
                LearningContext {
                    domain: Some("bank.com".into()),
                    ..Default::default()
                },
            ),
            LearningEntry::new(LearningCategory::SiteInteraction, "b", "s").with_context(
                LearningContext {
                    domain: Some("github.com".into()),
                    ..Default::default()
                },
            ),
            LearningEntry::new(LearningCategory::SiteInteraction, "c", "s"), // no domain
        ]));

        let source = FakeSource {
            entries: entries.clone(),
        };

        let mut collector = LearningCollector::new();
        collector.set_domain_blacklist(vec!["bank.com".into()]);
        collector.register_source(Box::new(source));

        let collected = collector.collect_all();
        // bank.com should be filtered out, github.com and no-domain should remain
        assert_eq!(collected.len(), 2);
        assert!(collected
            .iter()
            .all(|e| e.context.domain.as_deref() != Some("bank.com")));
    }

    #[test]
    fn collector_skips_when_disabled() {
        use std::sync::atomic::AtomicBool;

        let entries = Arc::new(Mutex::new(vec![LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        )]));

        let source = FakeSource {
            entries: entries.clone(),
        };

        let enabled = Arc::new(AtomicBool::new(false));
        let mut collector = LearningCollector::new();
        collector.register_source(Box::new(source));
        collector.set_enabled(enabled);

        let collected = collector.collect_all();
        assert!(collected.is_empty(), "should return empty when disabled");
    }

    #[test]
    fn collector_rate_limits_per_domain_trigger() {
        let entries: Vec<LearningEntry> = (0..10)
            .map(|_| {
                LearningEntry::new(LearningCategory::SiteInteraction, "click", "click failed")
                    .with_context(LearningContext {
                        domain: Some("example.com".into()),
                        ..Default::default()
                    })
            })
            .collect();

        let source = FakeSource {
            entries: Arc::new(Mutex::new(entries)),
        };

        let mut collector = LearningCollector::new();
        collector.set_rate_limit(5);
        collector.register_source(Box::new(source));

        let collected = collector.collect_all();
        // After dedup: 10 identical entries collapse to 1 (occurrence_count=10),
        // so the rate limit of 5 isn't hit in a single collect_all call.
        // But let's test with entries that have different IDs (distinct entries).
        assert!(collected.len() <= 5);
    }

    #[test]
    fn collector_rate_limits_distinct_entries() {
        // Create 10 entries with different contexts so they don't dedup
        let entries: Vec<LearningEntry> = (0..10)
            .map(|i| {
                LearningEntry::new(
                    LearningCategory::SiteInteraction,
                    "click",
                    &format!("click failed on element {}", i),
                )
                .with_context(LearningContext {
                    domain: Some("example.com".into()),
                    selector: Some(format!(".btn-{}", i)),
                    ..Default::default()
                })
            })
            .collect();

        let source = FakeSource {
            entries: Arc::new(Mutex::new(entries)),
        };

        let mut collector = LearningCollector::new();
        collector.set_rate_limit(5);
        collector.register_source(Box::new(source));

        let collected = collector.collect_all();
        assert_eq!(collected.len(), 5, "should be rate-limited to 5");
    }

    #[test]
    fn collector_rate_limit_different_domains_independent() {
        let mut entries = Vec::new();
        for i in 0..4 {
            entries.push(
                LearningEntry::new(
                    LearningCategory::SiteInteraction,
                    "click",
                    &format!("fail A{}", i),
                )
                .with_context(LearningContext {
                    domain: Some("site-a.com".into()),
                    selector: Some(format!(".a-{}", i)),
                    ..Default::default()
                }),
            );
        }
        for i in 0..4 {
            entries.push(
                LearningEntry::new(
                    LearningCategory::SiteInteraction,
                    "click",
                    &format!("fail B{}", i),
                )
                .with_context(LearningContext {
                    domain: Some("site-b.com".into()),
                    selector: Some(format!(".b-{}", i)),
                    ..Default::default()
                }),
            );
        }

        let source = FakeSource {
            entries: Arc::new(Mutex::new(entries)),
        };

        let mut collector = LearningCollector::new();
        collector.set_rate_limit(3);
        collector.register_source(Box::new(source));

        let collected = collector.collect_all();
        let a_count = collected
            .iter()
            .filter(|e| e.context.domain.as_deref() == Some("site-a.com"))
            .count();
        let b_count = collected
            .iter()
            .filter(|e| e.context.domain.as_deref() == Some("site-b.com"))
            .count();
        assert_eq!(a_count, 3, "site-a.com should be limited to 3");
        assert_eq!(b_count, 3, "site-b.com should be limited to 3");
    }

    #[test]
    fn collector_collects_when_enabled() {
        use std::sync::atomic::AtomicBool;

        let entries = Arc::new(Mutex::new(vec![LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click",
            "click failed",
        )]));

        let source = FakeSource {
            entries: entries.clone(),
        };

        let enabled = Arc::new(AtomicBool::new(true));
        let mut collector = LearningCollector::new();
        collector.register_source(Box::new(source));
        collector.set_enabled(enabled);

        let collected = collector.collect_all();
        assert_eq!(collected.len(), 1, "should collect normally when enabled");
    }
}
