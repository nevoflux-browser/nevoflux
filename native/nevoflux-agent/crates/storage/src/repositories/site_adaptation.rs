//! Repository for site adaptation persistence.

use rusqlite::params;

use crate::connection::Database;
use crate::error::Result;
use crate::models::site_adaptation::{CreateSiteAdaptationParams, SiteAdaptation};

/// Generate a unique ID with the SA- prefix.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = (ts as u64).wrapping_mul(6364136223846793005);
    format!("SA-{:06x}", hash & 0xFFFFFF)
}

/// Get the current timestamp as an RFC 3339 string.
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO 8601 format without chrono dependency
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since epoch
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

/// Repository for site adaptation CRUD operations.
pub struct SiteAdaptationRepository<'a> {
    db: &'a Database,
}

impl<'a> SiteAdaptationRepository<'a> {
    /// Create a new site adaptation repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new site adaptation record.
    pub fn create(&self, params: CreateSiteAdaptationParams) -> Result<SiteAdaptation> {
        let id = params.id.unwrap_or_else(generate_id);
        let now = now_rfc3339();

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO site_adaptations (id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    params.domain,
                    params.url_pattern,
                    params.adaptation_type,
                    params.content,
                    params.verified,
                    Option::<String>::None,
                    0.0f64,
                    0i64,
                    now,
                    now,
                ],
            )?;

            Ok(SiteAdaptation {
                id,
                domain: params.domain,
                url_pattern: params.url_pattern,
                adaptation_type: params.adaptation_type,
                content: params.content,
                verified: params.verified,
                last_verified_at: None,
                success_rate: 0.0,
                sample_count: 0,
                created_at: now.clone(),
                updated_at: now,
            })
        })
    }

    /// Get a site adaptation by ID.
    pub fn get(&self, id: &str) -> Result<Option<SiteAdaptation>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at
                 FROM site_adaptations WHERE id = ?1",
            )?;

            let mut rows = stmt.query_map(params![id], row_to_site_adaptation)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// Query site adaptations by domain, ordered by success_rate descending.
    pub fn query_by_domain(&self, domain: &str, limit: u32) -> Result<Vec<SiteAdaptation>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at
                 FROM site_adaptations WHERE domain = ?1
                 ORDER BY success_rate DESC
                 LIMIT ?2",
            )?;

            let rows = stmt
                .query_map(params![domain, limit], row_to_site_adaptation)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Update the statistics for a site adaptation.
    pub fn update_stats(&self, id: &str, success_rate: f64, sample_count: i64) -> Result<()> {
        let now = now_rfc3339();
        self.db.with_connection(|conn| {
            let rows = conn.execute(
                "UPDATE site_adaptations SET success_rate = ?1, sample_count = ?2, last_verified_at = ?3, updated_at = ?3
                 WHERE id = ?4",
                params![success_rate, sample_count, now, id],
            )?;

            if rows == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "site_adaptation".to_string(),
                    id: id.to_string(),
                });
            }
            Ok(())
        })
    }

    /// Query site adaptations with success rate below a threshold that have
    /// at least `min_samples` observations, ordered by success rate ascending
    /// (worst first).
    pub fn query_low_success_rate(
        &self,
        max_success_rate: f64,
        min_samples: i64,
        limit: u32,
    ) -> Result<Vec<SiteAdaptation>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at
                 FROM site_adaptations
                 WHERE success_rate < ?1 AND sample_count >= ?2
                 ORDER BY success_rate ASC
                 LIMIT ?3",
            )?;

            let rows = stmt
                .query_map(params![max_success_rate, min_samples, limit], row_to_site_adaptation)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Find a site adaptation by domain and CSS selector.
    ///
    /// Looks for records where `adaptation_type = 'selector_result'` and
    /// `json_extract(content, '$.selector')` matches the given selector.
    pub fn find_by_domain_and_selector(
        &self,
        domain: &str,
        selector: &str,
    ) -> Result<Option<SiteAdaptation>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at
                 FROM site_adaptations
                 WHERE domain = ?1 AND adaptation_type = 'selector_result'
                   AND json_extract(content, '$.selector') = ?2
                 LIMIT 1",
            )?;

            let mut rows = stmt.query_map(params![domain, selector], row_to_site_adaptation)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// Find a site adaptation by domain and element ID.
    ///
    /// Looks for records where `adaptation_type = 'selector_result'` and
    /// `json_extract(content, '$.element_id')` matches the given element ID.
    pub fn find_by_domain_and_element_id(
        &self,
        domain: &str,
        element_id: &str,
    ) -> Result<Option<SiteAdaptation>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, domain, url_pattern, adaptation_type, content, verified, last_verified_at, success_rate, sample_count, created_at, updated_at
                 FROM site_adaptations
                 WHERE domain = ?1 AND adaptation_type = 'selector_result'
                   AND json_extract(content, '$.element_id') = ?2
                 LIMIT 1",
            )?;

            let mut rows =
                stmt.query_map(params![domain, element_id], row_to_site_adaptation)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// Delete a site adaptation by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows = conn.execute("DELETE FROM site_adaptations WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        })
    }
}

/// Convert a database row to a SiteAdaptation.
fn row_to_site_adaptation(row: &rusqlite::Row<'_>) -> rusqlite::Result<SiteAdaptation> {
    Ok(SiteAdaptation {
        id: row.get(0)?,
        domain: row.get(1)?,
        url_pattern: row.get(2)?,
        adaptation_type: row.get(3)?,
        content: row.get(4)?,
        verified: row.get(5)?,
        last_verified_at: row.get(6)?,
        success_rate: row.get(7)?,
        sample_count: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    #[test]
    fn test_create_and_get_site_adaptation() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        let params = CreateSiteAdaptationParams::new(
            "example.com",
            "selector_result",
            r#"{"selector": ".main-content"}"#,
        )
        .with_id("SA-test01");

        let record = repo.create(params).unwrap();
        assert_eq!(record.id, "SA-test01");
        assert_eq!(record.domain, "example.com");
        assert_eq!(record.adaptation_type, "selector_result");
        assert!(!record.verified);
        assert!((record.success_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(record.sample_count, 0);

        // Get it back
        let fetched = repo.get("SA-test01").unwrap().unwrap();
        assert_eq!(fetched.id, "SA-test01");
        assert_eq!(fetched.domain, "example.com");
        assert_eq!(fetched.content, r#"{"selector": ".main-content"}"#);
    }

    #[test]
    fn test_get_nonexistent() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_query_by_domain() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        // Create adaptations for two domains
        repo.create(
            CreateSiteAdaptationParams::new("example.com", "selector_result", r#"{"s": "a"}"#)
                .with_id("SA-001"),
        )
        .unwrap();
        repo.create(
            CreateSiteAdaptationParams::new("example.com", "spa_behavior", r#"{"s": "b"}"#)
                .with_id("SA-002"),
        )
        .unwrap();
        repo.create(
            CreateSiteAdaptationParams::new("other.com", "api_pattern", r#"{"s": "c"}"#)
                .with_id("SA-003"),
        )
        .unwrap();

        let results = repo.query_by_domain("example.com", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = repo.query_by_domain("other.com", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "SA-003");

        let results = repo.query_by_domain("nonexistent.com", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_by_domain_respects_limit() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        for i in 0..5 {
            repo.create(
                CreateSiteAdaptationParams::new("example.com", "selector_result", r#"{}"#)
                    .with_id(&format!("SA-{:03}", i)),
            )
            .unwrap();
        }

        let results = repo.query_by_domain("example.com", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_update_stats() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new("example.com", "selector_result", r#"{}"#)
                .with_id("SA-stats"),
        )
        .unwrap();

        repo.update_stats("SA-stats", 0.95, 100).unwrap();

        let fetched = repo.get("SA-stats").unwrap().unwrap();
        assert!((fetched.success_rate - 0.95).abs() < f64::EPSILON);
        assert_eq!(fetched.sample_count, 100);
        assert!(fetched.last_verified_at.is_some());
    }

    #[test]
    fn test_update_stats_not_found() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        let result = repo.update_stats("nonexistent", 0.5, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new("example.com", "selector_result", r#"{}"#)
                .with_id("SA-del"),
        )
        .unwrap();

        let deleted = repo.delete("SA-del").unwrap();
        assert!(deleted);

        let fetched = repo.get("SA-del").unwrap();
        assert!(fetched.is_none());

        // Deleting again returns false
        let deleted_again = repo.delete("SA-del").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_create_with_url_pattern() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        let params = CreateSiteAdaptationParams::new(
            "shop.example.com",
            "spa_behavior",
            r#"{"wait": 2000}"#,
        )
        .with_id("SA-url")
        .with_url_pattern("/products/*");

        let record = repo.create(params).unwrap();
        assert_eq!(record.url_pattern, Some("/products/*".to_string()));

        let fetched = repo.get("SA-url").unwrap().unwrap();
        assert_eq!(fetched.url_pattern, Some("/products/*".to_string()));
    }

    #[test]
    fn test_auto_generated_id() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        let record = repo
            .create(CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r#"{}"#,
            ))
            .unwrap();

        assert!(record.id.starts_with("SA-"));
        assert!(record.id.len() > 3);
    }

    #[test]
    fn test_find_by_domain_and_selector_found() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r##"{"selector": "#submit-btn", "action": "click"}"##,
            )
            .with_id("SA-sel01"),
        )
        .unwrap();
        repo.update_stats("SA-sel01", 0.8, 5).unwrap();

        let found = repo
            .find_by_domain_and_selector("example.com", "#submit-btn")
            .unwrap();
        assert!(found.is_some());
        let record = found.unwrap();
        assert_eq!(record.id, "SA-sel01");
        assert!((record.success_rate - 0.8).abs() < f64::EPSILON);
        assert_eq!(record.sample_count, 5);
    }

    #[test]
    fn test_find_by_domain_and_selector_not_found() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r##"{"selector": "#submit-btn", "action": "click"}"##,
            )
            .with_id("SA-sel02"),
        )
        .unwrap();

        let found = repo
            .find_by_domain_and_selector("example.com", "#login-btn")
            .unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_find_by_domain_and_element_id_found() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r#"{"element_id": "login-form", "action": "fill_by_id"}"#,
            )
            .with_id("SA-eid01"),
        )
        .unwrap();
        repo.update_stats("SA-eid01", 0.9, 10).unwrap();

        let found = repo
            .find_by_domain_and_element_id("example.com", "login-form")
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "SA-eid01");
    }

    #[test]
    fn test_find_by_domain_and_selector_wrong_domain() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = SiteAdaptationRepository::new(storage.database());

        repo.create(
            CreateSiteAdaptationParams::new(
                "example.com",
                "selector_result",
                r##"{"selector": "#submit-btn", "action": "click"}"##,
            )
            .with_id("SA-sel03"),
        )
        .unwrap();

        let found = repo
            .find_by_domain_and_selector("other.com", "#submit-btn")
            .unwrap();
        assert!(found.is_none());
    }
}
