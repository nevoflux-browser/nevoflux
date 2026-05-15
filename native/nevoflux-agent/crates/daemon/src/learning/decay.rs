// crates/daemon/src/learning/decay.rs
//
// Lazy decay calculation for learning entries. Determines how quickly a learning
// entry loses relevance over time based on its category, effectiveness, and usage
// frequency. Used by the retriever to prioritize fresh, effective knowledge.

use chrono::{DateTime, Utc};

/// Decay state derived from a decay score, controlling how entries are treated
/// during retrieval and lifecycle management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecayState {
    /// Score >= 0.5: Normal use, fully included in queries.
    Active,
    /// Score in [0.2, 0.5): Lower priority in retrieval results.
    Decaying,
    /// Score in [0.05, 0.2): Pending archive; a new hit can resurrect the entry.
    NearArchive,
    /// Score < 0.05: Removed from active queries.
    Archived,
}

/// Map a decay score to its corresponding `DecayState`.
///
/// Thresholds (from design Section 10.2):
/// - `[0.5, 1.0]` -> `Active`
/// - `[0.2, 0.5)` -> `Decaying`
/// - `[0.05, 0.2)` -> `NearArchive`
/// - `[0.0, 0.05)` -> `Archived`
pub fn decay_state(score: f64) -> DecayState {
    if score >= 0.5 {
        DecayState::Active
    } else if score >= 0.2 {
        DecayState::Decaying
    } else if score >= 0.05 {
        DecayState::NearArchive
    } else {
        DecayState::Archived
    }
}

/// Calculate the decay score for a learning entry.
///
/// The formula implements exponential decay with a category-specific half-life
/// that is adjusted by effectiveness and usage frequency:
///
/// ```text
/// adjusted_halflife = base_halflife * (1 + clamp(effectiveness, 0, 1))
///                                   * (1 + ln(max(hit_count, 1)) / 10)
/// score = exp(-0.693 * days_since_hit / adjusted_halflife)
/// ```
///
/// # Arguments
///
/// - `last_hit_at` - When the entry was last accessed/hit.
/// - `category` - Category string: `"site_interaction"`, `"tool_optimization"`,
///   `"user_preference"`, or any other value (defaults to 60-day half-life).
/// - `effectiveness` - How effective this entry has been (0.0 to 1.0+; clamped to 1.0).
/// - `hit_count` - Number of times this entry has been accessed. Zero is safe
///   (treated as 1 for the `ln` calculation).
/// - `now` - Current time. Passed explicitly for testability.
///
/// # Returns
///
/// A decay score in `[0.0, 1.0]` where 1.0 means fully fresh and 0.0 means
/// completely decayed.
pub fn calculate_decay(
    last_hit_at: DateTime<Utc>,
    category: &str,
    effectiveness: f64,
    hit_count: u32,
    now: DateTime<Utc>,
) -> f64 {
    let days_since_hit = (now - last_hit_at).num_days() as f64;
    // Clamp to 0 if last_hit_at is in the future.
    let days_since_hit = days_since_hit.max(0.0);

    let base_halflife = match category {
        "site_interaction" => 30.0,
        "tool_optimization" => 90.0,
        "user_preference" => 180.0,
        _ => 60.0,
    };

    // Guard against hit_count == 0: use max(hit_count, 1) so ln(1) = 0 instead
    // of ln(0) = -inf.
    let safe_hit_count = (hit_count.max(1)) as f64;

    let adjusted_halflife =
        base_halflife * (1.0 + effectiveness.min(1.0)) * (1.0 + safe_hit_count.ln() / 10.0);

    (-0.693 * days_since_hit / adjusted_halflife)
        .exp()
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use proptest::prelude::*;

    #[test]
    fn fresh_entry_has_full_decay_score() {
        let now = Utc::now();
        let score = calculate_decay(now, "site_interaction", 0.8, 10, now);
        assert!(
            (score - 1.0).abs() < 0.01,
            "Fresh entry should have score ~1.0, got {score}"
        );
    }

    #[test]
    fn old_entry_decays_significantly() {
        let now = Utc::now();
        let old = now - Duration::days(150);
        let score = calculate_decay(old, "site_interaction", 0.5, 5, now);
        assert!(
            score < 0.2,
            "150-day-old site_interaction entry should decay below 0.2, got {score}"
        );
    }

    #[test]
    fn high_effectiveness_extends_halflife() {
        let now = Utc::now();
        let date = now - Duration::days(30);
        let low_eff = calculate_decay(date, "site_interaction", 0.1, 1, now);
        let high_eff = calculate_decay(date, "site_interaction", 0.9, 50, now);
        assert!(
            high_eff > low_eff,
            "High effectiveness/usage should decay slower: high_eff={high_eff}, low_eff={low_eff}"
        );
    }

    #[test]
    fn zero_hit_count_does_not_panic() {
        let now = Utc::now();
        let date = now - Duration::days(10);
        let score = calculate_decay(date, "site_interaction", 0.5, 0, now);
        assert!(score >= 0.0 && score <= 1.0, "Score out of range: {score}");
    }

    #[test]
    fn future_date_clamps_to_zero_days() {
        let now = Utc::now();
        let future = now + Duration::days(10);
        let score = calculate_decay(future, "site_interaction", 0.5, 5, now);
        assert!(
            (score - 1.0).abs() < 0.001,
            "Future last_hit_at should yield score ~1.0, got {score}"
        );
    }

    #[test]
    fn effectiveness_above_one_is_clamped() {
        let now = Utc::now();
        let date = now - Duration::days(30);
        let clamped = calculate_decay(date, "site_interaction", 5.0, 10, now);
        let at_one = calculate_decay(date, "site_interaction", 1.0, 10, now);
        assert!(
            (clamped - at_one).abs() < 0.001,
            "effectiveness > 1.0 should be clamped to 1.0: clamped={clamped}, at_one={at_one}"
        );
    }

    #[test]
    fn decay_state_active() {
        assert_eq!(decay_state(1.0), DecayState::Active);
        assert_eq!(decay_state(0.75), DecayState::Active);
        assert_eq!(decay_state(0.5), DecayState::Active);
    }

    #[test]
    fn decay_state_decaying() {
        assert_eq!(decay_state(0.49), DecayState::Decaying);
        assert_eq!(decay_state(0.3), DecayState::Decaying);
        assert_eq!(decay_state(0.2), DecayState::Decaying);
    }

    #[test]
    fn decay_state_near_archive() {
        assert_eq!(decay_state(0.19), DecayState::NearArchive);
        assert_eq!(decay_state(0.1), DecayState::NearArchive);
        assert_eq!(decay_state(0.05), DecayState::NearArchive);
    }

    #[test]
    fn decay_state_archived() {
        assert_eq!(decay_state(0.049), DecayState::Archived);
        assert_eq!(decay_state(0.01), DecayState::Archived);
        assert_eq!(decay_state(0.0), DecayState::Archived);
    }

    #[test]
    fn category_specific_halflives() {
        let now = Utc::now();
        let date = now - Duration::days(60);
        let eff = 0.5;
        let hits = 5;

        let site = calculate_decay(date, "site_interaction", eff, hits, now);
        let tool = calculate_decay(date, "tool_optimization", eff, hits, now);
        let user = calculate_decay(date, "user_preference", eff, hits, now);
        let other = calculate_decay(date, "unknown_category", eff, hits, now);

        // site_interaction (30-day halflife) should decay fastest
        // user_preference (180-day halflife) should decay slowest
        assert!(
            user > tool,
            "user_preference should decay slower than tool_optimization: user={user}, tool={tool}"
        );
        assert!(
            tool > site,
            "tool_optimization should decay slower than site_interaction: tool={tool}, site={site}"
        );
        // unknown category (60-day halflife) falls between site (30) and tool (90)
        assert!(
            other > site,
            "default (60d) should decay slower than site_interaction (30d): other={other}, site={site}"
        );
        assert!(
            tool > other,
            "tool_optimization (90d) should decay slower than default (60d): tool={tool}, other={other}"
        );
    }

    #[test]
    fn user_preference_decays_slower_than_site_interaction() {
        let now = Utc::now();
        let date = now - Duration::days(90);
        let site = calculate_decay(date, "site_interaction", 0.5, 5, now);
        let user = calculate_decay(date, "user_preference", 0.5, 5, now);
        assert!(
            user > site,
            "user_preference should decay slower: user={user}, site={site}"
        );
    }

    #[test]
    fn higher_hit_count_extends_halflife() {
        let now = Utc::now();
        let date = now - Duration::days(30);
        let few_hits = calculate_decay(date, "site_interaction", 0.5, 1, now);
        let many_hits = calculate_decay(date, "site_interaction", 0.5, 1000, now);
        assert!(
            many_hits > few_hits,
            "More hits should extend half-life: many={many_hits}, few={few_hits}"
        );
    }

    #[test]
    fn very_old_entry_is_archived() {
        let now = Utc::now();
        let ancient = now - Duration::days(365 * 3);
        let score = calculate_decay(ancient, "site_interaction", 0.5, 5, now);
        assert_eq!(
            decay_state(score),
            DecayState::Archived,
            "3-year-old entry should be archived, score={score}"
        );
    }

    proptest! {
        #[test]
        fn decay_always_between_0_and_1(
            days in 0u32..3650,
            effectiveness in 0.0f64..2.0,
            hit_count in 0u32..10000,
        ) {
            let now = Utc::now();
            let date = now - Duration::days(days as i64);
            let score = calculate_decay(date, "site_interaction", effectiveness, hit_count, now);
            prop_assert!(score >= 0.0, "score should be >= 0.0, got {}", score);
            prop_assert!(score <= 1.0, "score should be <= 1.0, got {}", score);
        }

        #[test]
        fn decay_monotonically_decreases_with_time(
            days1 in 0u32..1825,
            days2 in 0u32..1825,
            effectiveness in 0.0f64..1.0,
            hit_count in 1u32..10000,
        ) {
            let now = Utc::now();
            let (earlier, later) = if days1 <= days2 {
                (days1, days2)
            } else {
                (days2, days1)
            };
            let score_recent = calculate_decay(
                now - Duration::days(earlier as i64),
                "tool_optimization", effectiveness, hit_count, now
            );
            let score_old = calculate_decay(
                now - Duration::days(later as i64),
                "tool_optimization", effectiveness, hit_count, now
            );
            prop_assert!(
                score_recent >= score_old,
                "More recent should have higher score: recent={}, old={}, days_recent={}, days_old={}",
                score_recent, score_old, earlier, later
            );
        }

        #[test]
        fn decay_state_covers_all_scores(score in 0.0f64..=1.0) {
            let state = decay_state(score);
            match state {
                DecayState::Active => prop_assert!(score >= 0.5),
                DecayState::Decaying => {
                    prop_assert!(score >= 0.2);
                    prop_assert!(score < 0.5);
                }
                DecayState::NearArchive => {
                    prop_assert!(score >= 0.05);
                    prop_assert!(score < 0.2);
                }
                DecayState::Archived => prop_assert!(score < 0.05),
            }
        }
    }
}
