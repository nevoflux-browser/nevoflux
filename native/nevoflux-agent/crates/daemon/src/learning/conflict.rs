// crates/daemon/src/learning/conflict.rs
//
// Conflict detection and resolution for the learning system. When new knowledge
// is ingested, it may contradict or overlap with existing entries. This module
// provides pure-logic functions to detect such conflicts and recommend
// resolutions, without performing any I/O.
//
// Design reference: Section 11 (Conflict Resolution).

use chrono::{DateTime, Utc};
use nevoflux_storage::Knowledge;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Types of conflicts that can occur between knowledge entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// Same subject (selector/domain), contradicting conclusions.
    DirectContradiction,
    /// Same scenario, different approaches.
    StrategyConflict,
    /// Older knowledge possibly outdated by newer entry.
    TemporalConflict,
    /// General vs specific scope (e.g., `*.example.com` vs `shop.example.com`).
    ScopeConflict,
}

/// The recommended resolution for a detected conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// New entry should overwrite the old one.
    NewOverwritesOld,
    /// Both entries should be kept (e.g., strategy conflict), ranked by
    /// effectiveness.
    KeepBoth,
    /// Conflict requires user arbitration (e.g., high-confidence old entry).
    RequiresArbitration,
    /// Existing entry is manual-edit protected; new entry is rejected.
    ManualEditProtected,
    /// Specific entry takes priority over general.
    SpecificOverridesGeneral,
}

/// A detected conflict between two knowledge entries.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The type of conflict.
    pub conflict_type: ConflictType,
    /// The ID of the existing entry.
    pub existing_id: String,
    /// The ID of the incoming entry.
    pub incoming_id: String,
    /// The recommended resolution.
    pub resolution: Resolution,
    /// Explanation of why this resolution was chosen.
    pub reason: String,
}

/// Actions that the caller should take to resolve a conflict.
#[derive(Debug, Clone)]
pub enum ConflictAction {
    /// Archive the specified entry (it lost the conflict).
    Archive(String),
    /// Keep both entries (strategy conflict).
    Keep,
    /// Flag for user review.
    FlagForUser(Conflict),
    /// Reject the incoming entry (manual-edit protection).
    RejectIncoming(String),
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect whether a conflict exists between two knowledge entries.
///
/// Returns `Some(Conflict)` if a conflict is detected, `None` if the entries
/// are compatible (different subject matter, no overlap).
pub fn detect_conflict(existing: &Knowledge, incoming: &Knowledge) -> Option<Conflict> {
    // 1. Manual-edit priority check (Section 11.2).
    //    Manual edits always win over system promotions.
    if existing.source_type == "manual" && incoming.source_type == "system" {
        if is_same_subject(existing, incoming) {
            return Some(Conflict {
                conflict_type: ConflictType::DirectContradiction,
                existing_id: existing.id.clone(),
                incoming_id: incoming.id.clone(),
                resolution: Resolution::ManualEditProtected,
                reason: "Existing entry was manually edited; system entry is rejected".into(),
            });
        }
    }

    // 2. Scope conflict: same category + subcategory, one is domain-specific
    //    and the other is universal.
    if is_scope_conflict(existing, incoming) {
        return Some(Conflict {
            conflict_type: ConflictType::ScopeConflict,
            existing_id: existing.id.clone(),
            incoming_id: incoming.id.clone(),
            resolution: Resolution::SpecificOverridesGeneral,
            reason: "Specific entry takes priority over general; general gets exception".into(),
        });
    }

    // The remaining checks require the entries to address the same subject.
    if !is_same_subject(existing, incoming) {
        return None;
    }

    // 3. Direct contradiction: same subject with contradicting conclusions.
    if is_contradicting(existing, incoming) {
        let old_score = existing.confidence * existing.hit_count as f64;
        let new_score = incoming.confidence * incoming.hit_count as f64;

        // Safeguard: if old confidence*hits > new*2, flag for user arbitration.
        // Exception: auto-generated entries (system, auto_extraction, consolidation)
        // should always let new data win — the latest observation is more relevant.
        // Only manually created entries ("manual") warrant user protection.
        let is_manual = existing.source_type == "manual";
        if is_manual && old_score > new_score * 2.0 {
            return Some(Conflict {
                conflict_type: ConflictType::DirectContradiction,
                existing_id: existing.id.clone(),
                incoming_id: incoming.id.clone(),
                resolution: Resolution::RequiresArbitration,
                reason: format!(
                    "Old manual entry score ({:.2}) significantly exceeds new ({:.2}); requires user review",
                    old_score, new_score
                ),
            });
        }

        return Some(Conflict {
            conflict_type: ConflictType::DirectContradiction,
            existing_id: existing.id.clone(),
            incoming_id: incoming.id.clone(),
            resolution: Resolution::NewOverwritesOld,
            reason: "Same subject with contradicting details; new overwrites old".into(),
        });
    }

    // 4. Temporal conflict: similar entries with a significant age gap.
    if is_temporal_gap(existing, incoming) {
        return Some(Conflict {
            conflict_type: ConflictType::TemporalConflict,
            existing_id: existing.id.clone(),
            incoming_id: incoming.id.clone(),
            resolution: Resolution::NewOverwritesOld,
            reason: "Significant temporal gap; newer entry supersedes older one".into(),
        });
    }

    // 5. Strategy conflict: same subject, not contradicting, but different
    //    details/approach (kept as alternative strategies).
    if existing.details != incoming.details {
        return Some(Conflict {
            conflict_type: ConflictType::StrategyConflict,
            existing_id: existing.id.clone(),
            incoming_id: incoming.id.clone(),
            resolution: Resolution::KeepBoth,
            reason: "Same scenario with different approach; both kept, ranked by effectiveness"
                .into(),
        });
    }

    None
}

/// Check an incoming entry against a list of existing entries.
/// Returns the first conflict found, skipping entries that share the same ID
/// as the incoming entry and entries with "archived" status.
pub fn detect_conflict_against(incoming: &Knowledge, existing: &[Knowledge]) -> Option<Conflict> {
    existing
        .iter()
        .filter(|e| e.id != incoming.id && e.status != "archived")
        .find_map(|e| detect_conflict(e, incoming))
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Map a conflict to the action the caller should take.
///
/// This function describes what actions should be taken but does NOT perform
/// I/O. The caller is responsible for executing the resolution.
pub fn resolve_conflict(conflict: &Conflict) -> ConflictAction {
    match &conflict.resolution {
        Resolution::NewOverwritesOld => ConflictAction::Archive(conflict.existing_id.clone()),
        Resolution::KeepBoth => ConflictAction::Keep,
        Resolution::RequiresArbitration => ConflictAction::FlagForUser(conflict.clone()),
        Resolution::ManualEditProtected => {
            ConflictAction::RejectIncoming(conflict.incoming_id.clone())
        }
        Resolution::SpecificOverridesGeneral => {
            ConflictAction::Archive(conflict.existing_id.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if two entries address the same subject (category + domain).
fn is_same_subject(a: &Knowledge, b: &Knowledge) -> bool {
    a.category == b.category && a.domain == b.domain
}

/// Check if two entries have contradicting conclusions.
///
/// Heuristic: same subcategory but different details or resolution.
fn is_contradicting(a: &Knowledge, b: &Knowledge) -> bool {
    a.subcategory == b.subcategory && a.details != b.details
}

/// Check if there is a scope conflict (one is domain-specific, the other
/// universal, with the same category + subcategory).
fn is_scope_conflict(a: &Knowledge, b: &Knowledge) -> bool {
    a.category == b.category
        && a.subcategory == b.subcategory
        && ((a.domain.is_some() && b.domain.is_none())
            || (a.domain.is_none() && b.domain.is_some()))
}

/// Check if there is a significant temporal gap between entries (> 30 days).
fn is_temporal_gap(a: &Knowledge, b: &Knowledge) -> bool {
    let parse = |s: &str| -> Option<DateTime<Utc>> { s.parse::<DateTime<Utc>>().ok() };

    let a_created = parse(&a.created_at);
    let b_created = parse(&b.created_at);

    match (a_created, b_created) {
        (Some(ta), Some(tb)) => {
            let gap = (ta - tb).num_days().unsigned_abs();
            gap > 30
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal `Knowledge` entry for testing.
    fn make_knowledge(overrides: KnowledgeOverrides) -> Knowledge {
        Knowledge {
            id: overrides.id.unwrap_or_else(|| "K-20260101-aaa111".into()),
            category: overrides
                .category
                .unwrap_or_else(|| "site_interaction".into()),
            subcategory: overrides.subcategory.flatten(),
            domain: overrides.domain.flatten(),
            summary: overrides.summary.unwrap_or_else(|| "test summary".into()),
            details: overrides.details.unwrap_or_else(|| "test details".into()),
            resolution: None,
            confidence: overrides.confidence.unwrap_or(0.5),
            hit_count: overrides.hit_count.unwrap_or(1),
            success_count: 0,
            fail_count: 0,
            effectiveness: 0.0,
            priority: "medium".into(),
            status: "validated".into(),
            source_ids: None,
            related_ids: None,
            tags: None,
            privacy_level: "internal".into(),
            promotion_target: None,
            promoted_section: None,
            source_type: overrides.source_type.unwrap_or_else(|| "system".into()),
            created_at: overrides
                .created_at
                .unwrap_or_else(|| "2026-01-15T12:00:00Z".into()),
            updated_at: "2026-01-15T12:00:00Z".into(),
            last_hit_at: None,
            promoted_at: None,
            embedding: None,
            hot: false,
            hot_summary: None,
        }
    }

    /// Overrides for the `make_knowledge` helper.
    #[derive(Default)]
    struct KnowledgeOverrides {
        id: Option<String>,
        category: Option<String>,
        subcategory: Option<Option<String>>,
        domain: Option<Option<String>>,
        summary: Option<String>,
        details: Option<String>,
        confidence: Option<f64>,
        hit_count: Option<i64>,
        source_type: Option<String>,
        created_at: Option<String>,
    }

    // ------------------------------------------------------------------
    // 1. Manual-edit priority blocks system entry
    // ------------------------------------------------------------------
    #[test]
    fn manual_edit_blocks_system_entry() {
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing".into()),
            source_type: Some("manual".into()),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            source_type: Some("system".into()),
            ..Default::default()
        });

        let conflict = detect_conflict(&existing, &incoming);
        assert!(conflict.is_some(), "Should detect conflict");
        let c = conflict.unwrap();
        assert_eq!(c.resolution, Resolution::ManualEditProtected);
        assert_eq!(c.conflict_type, ConflictType::DirectContradiction);
    }

    // ------------------------------------------------------------------
    // 2. Direct contradiction: new overwrites old
    // ------------------------------------------------------------------
    #[test]
    fn direct_contradiction_new_overwrites_old() {
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-old".into()),
            confidence: Some(0.5),
            hit_count: Some(2),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-new".into()),
            confidence: Some(0.6),
            hit_count: Some(3),
            ..Default::default()
        });

        let conflict = detect_conflict(&existing, &incoming);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::DirectContradiction);
        assert_eq!(c.resolution, Resolution::NewOverwritesOld);
    }

    // ------------------------------------------------------------------
    // 3. Direct contradiction safeguard: high-value manual entry triggers arbitration
    // ------------------------------------------------------------------
    #[test]
    fn high_value_old_entry_triggers_arbitration() {
        // Both entries must be manual for arbitration to trigger
        // (manual vs system is caught earlier by ManualEditProtected)
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-old".into()),
            confidence: Some(0.9),
            hit_count: Some(100),
            source_type: Some("manual".into()),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-new".into()),
            confidence: Some(0.5),
            hit_count: Some(1),
            source_type: Some("manual".into()),
            ..Default::default()
        });

        let conflict = detect_conflict(&existing, &incoming);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::DirectContradiction);
        assert_eq!(c.resolution, Resolution::RequiresArbitration);
    }

    #[test]
    fn high_value_system_entry_does_not_trigger_arbitration() {
        // Auto-generated (system) entries should let new data win
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-old".into()),
            confidence: Some(0.9),
            hit_count: Some(100),
            source_type: Some("system".into()),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use selector .btn-new".into()),
            confidence: Some(0.5),
            hit_count: Some(1),
            ..Default::default()
        });

        let conflict = detect_conflict(&existing, &incoming);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::DirectContradiction);
        assert_eq!(c.resolution, Resolution::NewOverwritesOld);
    }

    // ------------------------------------------------------------------
    // 4. Strategy conflict: both kept
    // ------------------------------------------------------------------
    #[test]
    fn strategy_conflict_keeps_both() {
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing".into()),
            subcategory: Some(Some("login".into())),
            details: Some("Use approach A".into()),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            // Different subcategory -> not a direct contradiction, but same subject
            subcategory: Some(Some("checkout".into())),
            details: Some("Use approach B".into()),
            ..Default::default()
        });

        let conflict = detect_conflict(&existing, &incoming);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::StrategyConflict);
        assert_eq!(c.resolution, Resolution::KeepBoth);
    }

    // ------------------------------------------------------------------
    // 5. Scope conflict: specific overrides general
    // ------------------------------------------------------------------
    #[test]
    fn scope_conflict_specific_overrides_general() {
        let general = make_knowledge(KnowledgeOverrides {
            id: Some("K-general".into()),
            domain: Some(None), // universal
            subcategory: Some(Some("nav".into())),
            ..Default::default()
        });
        let specific = make_knowledge(KnowledgeOverrides {
            id: Some("K-specific".into()),
            domain: Some(Some("shop.example.com".into())),
            subcategory: Some(Some("nav".into())),
            ..Default::default()
        });

        let conflict = detect_conflict(&general, &specific);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::ScopeConflict);
        assert_eq!(c.resolution, Resolution::SpecificOverridesGeneral);
    }

    // ------------------------------------------------------------------
    // 6. Temporal conflict: new overwrites old
    // ------------------------------------------------------------------
    #[test]
    fn temporal_conflict_new_overwrites_old() {
        let old = make_knowledge(KnowledgeOverrides {
            id: Some("K-old".into()),
            subcategory: Some(Some("login".into())),
            created_at: Some("2025-06-01T00:00:00Z".into()),
            ..Default::default()
        });
        let fresh = make_knowledge(KnowledgeOverrides {
            id: Some("K-fresh".into()),
            subcategory: Some(Some("login".into())),
            created_at: Some("2026-01-15T00:00:00Z".into()),
            ..Default::default()
        });

        let conflict = detect_conflict(&old, &fresh);
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert_eq!(c.conflict_type, ConflictType::TemporalConflict);
        assert_eq!(c.resolution, Resolution::NewOverwritesOld);
    }

    // ------------------------------------------------------------------
    // 7. Compatible entries return None
    // ------------------------------------------------------------------
    #[test]
    fn compatible_entries_return_none() {
        let a = make_knowledge(KnowledgeOverrides {
            id: Some("K-a".into()),
            category: Some("site_interaction".into()),
            domain: Some(Some("example.com".into())),
            ..Default::default()
        });
        let b = make_knowledge(KnowledgeOverrides {
            id: Some("K-b".into()),
            category: Some("tool_optimization".into()),
            domain: Some(Some("other.com".into())),
            ..Default::default()
        });

        assert!(detect_conflict(&a, &b).is_none());
    }

    // ------------------------------------------------------------------
    // 8. resolve_conflict maps to correct actions
    // ------------------------------------------------------------------
    #[test]
    fn resolve_conflict_maps_correctly() {
        // NewOverwritesOld -> Archive existing
        let c1 = Conflict {
            conflict_type: ConflictType::DirectContradiction,
            existing_id: "K-old".into(),
            incoming_id: "K-new".into(),
            resolution: Resolution::NewOverwritesOld,
            reason: "test".into(),
        };
        assert!(matches!(
            resolve_conflict(&c1),
            ConflictAction::Archive(id) if id == "K-old"
        ));

        // KeepBoth -> Keep
        let c2 = Conflict {
            conflict_type: ConflictType::StrategyConflict,
            existing_id: "K-a".into(),
            incoming_id: "K-b".into(),
            resolution: Resolution::KeepBoth,
            reason: "test".into(),
        };
        assert!(matches!(resolve_conflict(&c2), ConflictAction::Keep));

        // RequiresArbitration -> FlagForUser
        let c3 = Conflict {
            conflict_type: ConflictType::DirectContradiction,
            existing_id: "K-old".into(),
            incoming_id: "K-new".into(),
            resolution: Resolution::RequiresArbitration,
            reason: "test".into(),
        };
        assert!(matches!(
            resolve_conflict(&c3),
            ConflictAction::FlagForUser(_)
        ));

        // ManualEditProtected -> RejectIncoming
        let c4 = Conflict {
            conflict_type: ConflictType::DirectContradiction,
            existing_id: "K-manual".into(),
            incoming_id: "K-sys".into(),
            resolution: Resolution::ManualEditProtected,
            reason: "test".into(),
        };
        assert!(matches!(
            resolve_conflict(&c4),
            ConflictAction::RejectIncoming(id) if id == "K-sys"
        ));

        // SpecificOverridesGeneral -> Archive existing
        let c5 = Conflict {
            conflict_type: ConflictType::ScopeConflict,
            existing_id: "K-general".into(),
            incoming_id: "K-specific".into(),
            resolution: Resolution::SpecificOverridesGeneral,
            reason: "test".into(),
        };
        assert!(matches!(
            resolve_conflict(&c5),
            ConflictAction::Archive(id) if id == "K-general"
        ));
    }

    // ------------------------------------------------------------------
    // 9. Manual-edit protection does not trigger for non-same-subject
    // ------------------------------------------------------------------
    #[test]
    fn manual_edit_not_triggered_for_different_subject() {
        let existing = make_knowledge(KnowledgeOverrides {
            id: Some("K-manual".into()),
            source_type: Some("manual".into()),
            category: Some("site_interaction".into()),
            domain: Some(Some("a.com".into())),
            ..Default::default()
        });
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-sys".into()),
            source_type: Some("system".into()),
            category: Some("tool_optimization".into()),
            domain: Some(Some("b.com".into())),
            ..Default::default()
        });

        // Different category + domain: no conflict expected.
        assert!(detect_conflict(&existing, &incoming).is_none());
    }

    // ------------------------------------------------------------------
    // 10. Temporal gap under 30 days does not trigger temporal conflict
    // ------------------------------------------------------------------
    #[test]
    fn no_temporal_conflict_within_30_days() {
        let a = make_knowledge(KnowledgeOverrides {
            id: Some("K-a".into()),
            subcategory: Some(Some("login".into())),
            created_at: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        });
        let b = make_knowledge(KnowledgeOverrides {
            id: Some("K-b".into()),
            subcategory: Some(Some("login".into())),
            created_at: Some("2026-01-20T00:00:00Z".into()),
            ..Default::default()
        });

        // Same subject, same details, 19-day gap: should NOT detect temporal
        // conflict (gap <= 30).
        let conflict = detect_conflict(&a, &b);
        assert!(conflict.is_none());
    }

    // ------------------------------------------------------------------
    // 11. Scope conflict detected symmetrically
    // ------------------------------------------------------------------
    #[test]
    fn scope_conflict_detected_both_directions() {
        let general = make_knowledge(KnowledgeOverrides {
            id: Some("K-general".into()),
            domain: Some(None),
            subcategory: Some(Some("nav".into())),
            ..Default::default()
        });
        let specific = make_knowledge(KnowledgeOverrides {
            id: Some("K-specific".into()),
            domain: Some(Some("shop.example.com".into())),
            subcategory: Some(Some("nav".into())),
            ..Default::default()
        });

        // general vs specific -> scope conflict
        assert!(detect_conflict(&general, &specific).is_some());
        // specific vs general -> also scope conflict
        assert!(detect_conflict(&specific, &general).is_some());
    }

    // ------------------------------------------------------------------
    // 12. Identical entries (same details) on same subject return None
    // ------------------------------------------------------------------
    #[test]
    fn identical_entries_return_none() {
        let a = make_knowledge(KnowledgeOverrides {
            id: Some("K-a".into()),
            subcategory: Some(Some("login".into())),
            details: Some("same details".into()),
            created_at: Some("2026-01-15T00:00:00Z".into()),
            ..Default::default()
        });
        let b = make_knowledge(KnowledgeOverrides {
            id: Some("K-b".into()),
            subcategory: Some(Some("login".into())),
            details: Some("same details".into()),
            created_at: Some("2026-01-16T00:00:00Z".into()),
            ..Default::default()
        });

        // Same subject, same subcategory, same details, 1-day gap: no conflict.
        assert!(detect_conflict(&a, &b).is_none());
    }

    // ------------------------------------------------------------------
    // 13. detect_conflict_against finds first conflict
    // ------------------------------------------------------------------
    #[test]
    fn detect_conflict_against_finds_first_conflict() {
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            category: Some("tooloptimization".into()),
            subcategory: Some(Some("timeout".into())),
            domain: Some(Some("example.com".into())),
            details: Some("Use approach B".into()),
            ..Default::default()
        });

        let existing = vec![
            make_knowledge(KnowledgeOverrides {
                id: Some("K-existing-1".into()),
                category: Some("tooloptimization".into()),
                subcategory: Some(Some("timeout".into())),
                domain: Some(Some("example.com".into())),
                details: Some("Use approach A".into()),
                ..Default::default()
            }),
            make_knowledge(KnowledgeOverrides {
                id: Some("K-existing-2".into()),
                category: Some("tooloptimization".into()),
                subcategory: Some(Some("timeout".into())),
                domain: Some(Some("example.com".into())),
                details: Some("Use approach C".into()),
                ..Default::default()
            }),
        ];

        let conflict = detect_conflict_against(&incoming, &existing);
        assert!(conflict.is_some(), "Should detect a conflict");
        let c = conflict.unwrap();
        assert_eq!(c.existing_id, "K-existing-1");
        assert_eq!(c.incoming_id, "K-incoming");
    }

    // ------------------------------------------------------------------
    // 14. detect_conflict_against returns None when no conflict
    // ------------------------------------------------------------------
    #[test]
    fn detect_conflict_against_returns_none_when_no_conflict() {
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            category: Some("tooloptimization".into()),
            domain: Some(Some("example.com".into())),
            ..Default::default()
        });

        let existing = vec![
            make_knowledge(KnowledgeOverrides {
                id: Some("K-existing-1".into()),
                category: Some("siteinteraction".into()),
                domain: Some(Some("other.com".into())),
                ..Default::default()
            }),
            make_knowledge(KnowledgeOverrides {
                id: Some("K-existing-2".into()),
                category: Some("userpreference".into()),
                domain: Some(Some("another.com".into())),
                ..Default::default()
            }),
        ];

        let conflict = detect_conflict_against(&incoming, &existing);
        assert!(conflict.is_none(), "Should not detect a conflict");
    }

    // ------------------------------------------------------------------
    // 15. detect_conflict_against skips archived entries
    // ------------------------------------------------------------------
    #[test]
    fn detect_conflict_against_skips_archived() {
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-incoming".into()),
            category: Some("tooloptimization".into()),
            subcategory: Some(Some("timeout".into())),
            domain: Some(Some("example.com".into())),
            details: Some("Use approach B".into()),
            ..Default::default()
        });

        // The only existing entry has the same subject but is archived
        let mut archived_entry = make_knowledge(KnowledgeOverrides {
            id: Some("K-existing-archived".into()),
            category: Some("tooloptimization".into()),
            subcategory: Some(Some("timeout".into())),
            domain: Some(Some("example.com".into())),
            details: Some("Use approach A".into()),
            ..Default::default()
        });
        archived_entry.status = "archived".into();

        let existing = vec![archived_entry];

        let conflict = detect_conflict_against(&incoming, &existing);
        assert!(
            conflict.is_none(),
            "Should skip archived entries and find no conflict"
        );
    }

    // ------------------------------------------------------------------
    // 16. detect_conflict_against skips entries with same ID
    // ------------------------------------------------------------------
    #[test]
    fn detect_conflict_against_skips_same_id() {
        let incoming = make_knowledge(KnowledgeOverrides {
            id: Some("K-same".into()),
            category: Some("tooloptimization".into()),
            subcategory: Some(Some("timeout".into())),
            domain: Some(Some("example.com".into())),
            details: Some("Use approach B".into()),
            ..Default::default()
        });

        // The existing entry has the same ID as the incoming entry
        let existing = vec![make_knowledge(KnowledgeOverrides {
            id: Some("K-same".into()),
            category: Some("tooloptimization".into()),
            subcategory: Some(Some("timeout".into())),
            domain: Some(Some("example.com".into())),
            details: Some("Use approach A".into()),
            ..Default::default()
        })];

        let conflict = detect_conflict_against(&incoming, &existing);
        assert!(conflict.is_none(), "Should skip entries with the same ID");
    }
}
