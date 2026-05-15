//! Repository for tool statistics persistence.

use rusqlite::params;

use crate::connection::Database;
use crate::error::Result;
use crate::models::tool_stat::{CreateToolStatParams, ToolStat};

/// Generate a unique ID with the TS- prefix.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = (ts as u64).wrapping_mul(6364136223846793005);
    format!("TS-{:06x}", hash & 0xFFFFFF)
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

/// Repository for tool statistics CRUD operations.
pub struct ToolStatsRepository<'a> {
    db: &'a Database,
}

impl<'a> ToolStatsRepository<'a> {
    /// Create a new tool stats repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new tool stat record.
    pub fn create(&self, params: CreateToolStatParams) -> Result<ToolStat> {
        let id = params.id.unwrap_or_else(generate_id);
        let now = now_rfc3339();

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO tool_stats (id, tool_name, intent_category, call_count, success_count, avg_latency_ms, avg_token_cost, common_params, failure_patterns, best_combinations, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    params.tool_name,
                    params.intent_category,
                    0i64,
                    0i64,
                    Option::<f64>::None,
                    Option::<f64>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    now,
                ],
            )?;

            Ok(ToolStat {
                id,
                tool_name: params.tool_name,
                intent_category: params.intent_category,
                call_count: 0,
                success_count: 0,
                avg_latency_ms: None,
                avg_token_cost: None,
                common_params: None,
                failure_patterns: None,
                best_combinations: None,
                updated_at: now,
            })
        })
    }

    /// Get a tool stat by ID.
    pub fn get(&self, id: &str) -> Result<Option<ToolStat>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, tool_name, intent_category, call_count, success_count, avg_latency_ms, avg_token_cost, common_params, failure_patterns, best_combinations, updated_at
                 FROM tool_stats WHERE id = ?1",
            )?;

            let mut rows = stmt.query_map(params![id], row_to_tool_stat)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// Get a tool stat by tool name.
    pub fn get_by_tool(&self, tool_name: &str) -> Result<Option<ToolStat>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, tool_name, intent_category, call_count, success_count, avg_latency_ms, avg_token_cost, common_params, failure_patterns, best_combinations, updated_at
                 FROM tool_stats WHERE tool_name = ?1",
            )?;

            let mut rows = stmt.query_map(params![tool_name], row_to_tool_stat)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// Increment the call count for a tool, updating running average latency.
    ///
    /// If no record exists for the tool, this creates one automatically.
    pub fn increment_call(
        &self,
        tool_name: &str,
        success: bool,
        latency_ms: Option<f64>,
    ) -> Result<ToolStat> {
        let now = now_rfc3339();

        self.db.with_connection(|conn| {
            // Try to get the existing record
            let existing = {
                let mut stmt = conn.prepare(
                    "SELECT id, tool_name, intent_category, call_count, success_count, avg_latency_ms, avg_token_cost, common_params, failure_patterns, best_combinations, updated_at
                     FROM tool_stats WHERE tool_name = ?1",
                )?;
                let mut rows = stmt.query_map(params![tool_name], row_to_tool_stat)?;
                match rows.next() {
                    Some(row) => Some(row?),
                    None => None,
                }
            };

            match existing {
                Some(stat) => {
                    let new_call_count = stat.call_count + 1;
                    let new_success_count = if success {
                        stat.success_count + 1
                    } else {
                        stat.success_count
                    };

                    // Calculate running average for latency
                    let new_avg_latency = match (stat.avg_latency_ms, latency_ms) {
                        (Some(avg), Some(new_lat)) => {
                            // Running average: new_avg = old_avg + (new_val - old_avg) / n
                            Some(avg + (new_lat - avg) / new_call_count as f64)
                        }
                        (None, Some(new_lat)) => Some(new_lat),
                        (Some(avg), None) => Some(avg),
                        (None, None) => None,
                    };

                    conn.execute(
                        "UPDATE tool_stats SET call_count = ?1, success_count = ?2, avg_latency_ms = ?3, updated_at = ?4
                         WHERE id = ?5",
                        params![new_call_count, new_success_count, new_avg_latency, now, stat.id],
                    )?;

                    Ok(ToolStat {
                        call_count: new_call_count,
                        success_count: new_success_count,
                        avg_latency_ms: new_avg_latency,
                        updated_at: now,
                        ..stat
                    })
                }
                None => {
                    // Auto-create a new record
                    let id = generate_id();
                    let success_count: i64 = if success { 1 } else { 0 };

                    conn.execute(
                        "INSERT INTO tool_stats (id, tool_name, intent_category, call_count, success_count, avg_latency_ms, avg_token_cost, common_params, failure_patterns, best_combinations, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        params![
                            id,
                            tool_name,
                            Option::<String>::None,
                            1i64,
                            success_count,
                            latency_ms,
                            Option::<f64>::None,
                            Option::<String>::None,
                            Option::<String>::None,
                            Option::<String>::None,
                            now,
                        ],
                    )?;

                    Ok(ToolStat {
                        id,
                        tool_name: tool_name.to_string(),
                        intent_category: None,
                        call_count: 1,
                        success_count,
                        avg_latency_ms: latency_ms,
                        avg_token_cost: None,
                        common_params: None,
                        failure_patterns: None,
                        best_combinations: None,
                        updated_at: now,
                    })
                }
            }
        })
    }

    /// Delete a tool stat by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows = conn.execute("DELETE FROM tool_stats WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        })
    }
}

/// Convert a database row to a ToolStat.
fn row_to_tool_stat(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolStat> {
    Ok(ToolStat {
        id: row.get(0)?,
        tool_name: row.get(1)?,
        intent_category: row.get(2)?,
        call_count: row.get(3)?,
        success_count: row.get(4)?,
        avg_latency_ms: row.get(5)?,
        avg_token_cost: row.get(6)?,
        common_params: row.get(7)?,
        failure_patterns: row.get(8)?,
        best_combinations: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    #[test]
    fn test_create_and_get_tool_stat() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        let params = CreateToolStatParams::new("browser_click").with_id("TS-test01");

        let record = repo.create(params).unwrap();
        assert_eq!(record.id, "TS-test01");
        assert_eq!(record.tool_name, "browser_click");
        assert_eq!(record.call_count, 0);
        assert_eq!(record.success_count, 0);
        assert!(record.avg_latency_ms.is_none());

        // Get it back
        let fetched = repo.get("TS-test01").unwrap().unwrap();
        assert_eq!(fetched.id, "TS-test01");
        assert_eq!(fetched.tool_name, "browser_click");
    }

    #[test]
    fn test_get_nonexistent() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_by_tool() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        repo.create(
            CreateToolStatParams::new("browser_navigate")
                .with_id("TS-nav")
                .with_intent_category("navigation"),
        )
        .unwrap();

        let fetched = repo.get_by_tool("browser_navigate").unwrap().unwrap();
        assert_eq!(fetched.id, "TS-nav");
        assert_eq!(fetched.intent_category, Some("navigation".to_string()));

        let missing = repo.get_by_tool("nonexistent_tool").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_increment_call_existing() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        repo.create(CreateToolStatParams::new("browser_click").with_id("TS-inc"))
            .unwrap();

        // First call: success with 100ms latency
        let stat = repo
            .increment_call("browser_click", true, Some(100.0))
            .unwrap();
        assert_eq!(stat.call_count, 1);
        assert_eq!(stat.success_count, 1);
        assert!((stat.avg_latency_ms.unwrap() - 100.0).abs() < f64::EPSILON);

        // Second call: success with 200ms latency
        let stat = repo
            .increment_call("browser_click", true, Some(200.0))
            .unwrap();
        assert_eq!(stat.call_count, 2);
        assert_eq!(stat.success_count, 2);
        assert!((stat.avg_latency_ms.unwrap() - 150.0).abs() < f64::EPSILON);

        // Third call: failure with 300ms latency
        let stat = repo
            .increment_call("browser_click", false, Some(300.0))
            .unwrap();
        assert_eq!(stat.call_count, 3);
        assert_eq!(stat.success_count, 2);
        assert!((stat.avg_latency_ms.unwrap() - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_increment_call_auto_creates() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        // No pre-existing record
        let stat = repo.increment_call("new_tool", true, Some(50.0)).unwrap();
        assert!(stat.id.starts_with("TS-"));
        assert_eq!(stat.tool_name, "new_tool");
        assert_eq!(stat.call_count, 1);
        assert_eq!(stat.success_count, 1);
        assert!((stat.avg_latency_ms.unwrap() - 50.0).abs() < f64::EPSILON);

        // Verify it persisted
        let fetched = repo.get_by_tool("new_tool").unwrap().unwrap();
        assert_eq!(fetched.call_count, 1);
    }

    #[test]
    fn test_increment_call_no_latency() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        let stat = repo.increment_call("fast_tool", true, None).unwrap();
        assert_eq!(stat.call_count, 1);
        assert!(stat.avg_latency_ms.is_none());

        // Second call still no latency
        let stat = repo.increment_call("fast_tool", true, None).unwrap();
        assert_eq!(stat.call_count, 2);
        assert!(stat.avg_latency_ms.is_none());
    }

    #[test]
    fn test_delete() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        repo.create(CreateToolStatParams::new("browser_click").with_id("TS-del"))
            .unwrap();

        let deleted = repo.delete("TS-del").unwrap();
        assert!(deleted);

        let fetched = repo.get("TS-del").unwrap();
        assert!(fetched.is_none());

        let deleted_again = repo.delete("TS-del").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_auto_generated_id() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ToolStatsRepository::new(storage.database());

        let record = repo.create(CreateToolStatParams::new("some_tool")).unwrap();

        assert!(record.id.starts_with("TS-"));
        assert!(record.id.len() > 3);
    }
}
