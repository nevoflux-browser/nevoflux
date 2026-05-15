//! Repository for learning metrics persistence.

use rusqlite::params;

use crate::connection::Database;
use crate::error::Result;
use crate::models::learning_metrics::{CreateLearningMetricParams, LearningMetric};

/// Generate a unique ID with the LM- prefix.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = (ts as u64).wrapping_mul(6364136223846793005);
    format!("LM-{:06x}", hash & 0xFFFFFF)
}

/// Get the current timestamp as an RFC 3339 string.
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut y = 1970i64;
    let mut remaining_days = days as i64;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md {
            m = i;
            break;
        }
        remaining_days -= md;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Repository for learning metrics CRUD operations.
pub struct LearningMetricsRepository<'a> {
    db: &'a Database,
}

impl<'a> LearningMetricsRepository<'a> {
    /// Create a new learning metrics repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new learning metric record.
    pub fn create(&self, params: CreateLearningMetricParams) -> Result<LearningMetric> {
        let id = params.id.unwrap_or_else(generate_id);
        let now = now_rfc3339();

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO learning_metrics (id, metric_type, domain, period, value, sample_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    params.metric_type,
                    params.domain,
                    params.period,
                    params.value,
                    params.sample_count,
                    now,
                ],
            )?;

            Ok(LearningMetric {
                id,
                metric_type: params.metric_type,
                domain: params.domain,
                period: params.period,
                value: params.value,
                sample_count: params.sample_count,
                created_at: now,
            })
        })
    }

    /// Query learning metrics by type, ordered by period descending.
    pub fn query_by_type(&self, metric_type: &str, limit: u32) -> Result<Vec<LearningMetric>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, metric_type, domain, period, value, sample_count, created_at
                 FROM learning_metrics WHERE metric_type = ?1
                 ORDER BY period DESC
                 LIMIT ?2",
            )?;

            let rows = stmt
                .query_map(params![metric_type, limit], row_to_learning_metric)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Query learning metrics by period, ordered by metric_type.
    pub fn query_by_period(&self, period: &str) -> Result<Vec<LearningMetric>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, metric_type, domain, period, value, sample_count, created_at
                 FROM learning_metrics WHERE period = ?1
                 ORDER BY metric_type",
            )?;

            let rows = stmt
                .query_map(params![period], row_to_learning_metric)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Delete a learning metric by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows = conn.execute("DELETE FROM learning_metrics WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        })
    }

    /// Delete all learning metrics.
    ///
    /// Returns the number of deleted rows.
    pub fn delete_all(&self) -> Result<usize> {
        self.db.with_connection(|conn| {
            let count = conn.execute("DELETE FROM learning_metrics", [])?;
            Ok(count)
        })
    }
}

/// Convert a database row to a LearningMetric.
fn row_to_learning_metric(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningMetric> {
    Ok(LearningMetric {
        id: row.get(0)?,
        metric_type: row.get(1)?,
        domain: row.get(2)?,
        period: row.get(3)?,
        value: row.get(4)?,
        sample_count: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    #[test]
    fn test_create_and_query_by_type() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        repo.create(
            CreateLearningMetricParams::new("success_rate", "2026-02-17", 0.85)
                .with_id("LM-001")
                .with_sample_count(100),
        )
        .unwrap();

        repo.create(
            CreateLearningMetricParams::new("success_rate", "2026-02-16", 0.80)
                .with_id("LM-002")
                .with_sample_count(90),
        )
        .unwrap();

        repo.create(
            CreateLearningMetricParams::new("retry_rate", "2026-02-17", 0.12)
                .with_id("LM-003")
                .with_sample_count(50),
        )
        .unwrap();

        let results = repo.query_by_type("success_rate", 10).unwrap();
        assert_eq!(results.len(), 2);
        // Should be ordered by period DESC
        assert_eq!(results[0].period, "2026-02-17");
        assert_eq!(results[1].period, "2026-02-16");

        let results = repo.query_by_type("retry_rate", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "LM-003");
    }

    #[test]
    fn test_query_by_type_respects_limit() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        for i in 0..5 {
            repo.create(
                CreateLearningMetricParams::new(
                    "success_rate",
                    &format!("2026-02-{:02}", 10 + i),
                    0.5,
                )
                .with_id(&format!("LM-{:03}", i)),
            )
            .unwrap();
        }

        let results = repo.query_by_type("success_rate", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_by_period() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        repo.create(
            CreateLearningMetricParams::new("success_rate", "2026-02-17", 0.85).with_id("LM-p01"),
        )
        .unwrap();

        repo.create(
            CreateLearningMetricParams::new("retry_rate", "2026-02-17", 0.12).with_id("LM-p02"),
        )
        .unwrap();

        repo.create(
            CreateLearningMetricParams::new("success_rate", "2026-02-16", 0.80).with_id("LM-p03"),
        )
        .unwrap();

        let results = repo.query_by_period("2026-02-17").unwrap();
        assert_eq!(results.len(), 2);
        // Should be ordered by metric_type
        assert_eq!(results[0].metric_type, "retry_rate");
        assert_eq!(results[1].metric_type, "success_rate");

        let results = repo.query_by_period("2026-02-16").unwrap();
        assert_eq!(results.len(), 1);

        let results = repo.query_by_period("2026-02-15").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_create_with_domain() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        let record = repo
            .create(
                CreateLearningMetricParams::new("success_rate", "2026-02-17", 0.90)
                    .with_id("LM-dom")
                    .with_domain("example.com"),
            )
            .unwrap();

        assert_eq!(record.domain, Some("example.com".to_string()));

        let results = repo.query_by_type("success_rate", 10).unwrap();
        assert_eq!(results[0].domain, Some("example.com".to_string()));
    }

    #[test]
    fn test_delete() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        repo.create(
            CreateLearningMetricParams::new("success_rate", "2026-02-17", 0.85).with_id("LM-del"),
        )
        .unwrap();

        let deleted = repo.delete("LM-del").unwrap();
        assert!(deleted);

        let results = repo.query_by_type("success_rate", 10).unwrap();
        assert!(results.is_empty());

        let deleted_again = repo.delete("LM-del").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_auto_generated_id() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = LearningMetricsRepository::new(storage.database());

        let record = repo
            .create(CreateLearningMetricParams::new(
                "success_rate",
                "2026-02-17",
                0.85,
            ))
            .unwrap();

        assert!(record.id.starts_with("LM-"));
        assert!(record.id.len() > 3);
    }
}
