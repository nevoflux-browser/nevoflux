//! Repository for trace span records.

use rusqlite::params;

use crate::connection::Database;
use crate::error::Result;

/// Parameters for creating a trace span.
pub struct CreateTraceSpanParams {
    /// The session this span belongs to.
    pub session_id: String,
    /// The iteration number within the session.
    pub iteration: u32,
    /// The type of span (e.g., "llm_call", "tool_exec").
    pub span_type: String,
    /// The name of the tool, if this is a tool execution span.
    pub tool_name: Option<String>,
    /// Serialized tool parameters, if applicable.
    pub tool_params: Option<String>,
    /// Whether the operation succeeded.
    pub success: bool,
    /// Error code, if the operation failed.
    pub error_code: Option<String>,
    /// Error message, if the operation failed.
    pub error_msg: Option<String>,
    /// Duration of the operation in milliseconds.
    pub duration_ms: Option<u64>,
}

/// A trace span record from the database.
#[derive(Debug, Clone)]
pub struct TraceSpanRecord {
    /// Auto-incremented row ID.
    pub id: i64,
    /// The session this span belongs to.
    pub session_id: String,
    /// The iteration number within the session.
    pub iteration: u32,
    /// The type of span (e.g., "llm_call", "tool_exec").
    pub span_type: String,
    /// The name of the tool, if this is a tool execution span.
    pub tool_name: Option<String>,
    /// Serialized tool parameters, if applicable.
    pub tool_params: Option<String>,
    /// Whether the operation succeeded.
    pub success: bool,
    /// Error code, if the operation failed.
    pub error_code: Option<String>,
    /// Error message, if the operation failed.
    pub error_msg: Option<String>,
    /// Duration of the operation in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Repository for trace span CRUD operations.
pub struct TraceRepository<'a> {
    db: &'a Database,
}

impl<'a> TraceRepository<'a> {
    /// Create a new trace repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new trace span record.
    ///
    /// Returns the auto-generated row ID.
    pub fn create(&self, params: CreateTraceSpanParams) -> Result<i64> {
        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO trace_spans (session_id, iteration, span_type, tool_name, tool_params, success, error_code, error_msg, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    params.session_id,
                    params.iteration,
                    params.span_type,
                    params.tool_name,
                    params.tool_params,
                    params.success as i32,
                    params.error_code,
                    params.error_msg,
                    params.duration_ms.map(|v| v as i64),
                ],
            )?;

            let id = conn.last_insert_rowid();
            Ok(id)
        })
    }

    /// List all trace spans for a session, ordered by iteration then id.
    pub fn list_by_session(&self, session_id: &str) -> Result<Vec<TraceSpanRecord>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, iteration, span_type, tool_name, tool_params, success, error_code, error_msg, duration_ms
                 FROM trace_spans WHERE session_id = ?1 ORDER BY iteration, id",
            )?;

            let rows = stmt
                .query_map(params![session_id], row_to_trace_span)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Get the most recent tool execution spans for a session in chronological order.
    ///
    /// Fetches the last `limit` spans where span_type = 'tool_exec', then
    /// reverses the result so they are in chronological (ascending) order.
    pub fn recent_tool_spans(&self, session_id: &str, limit: u32) -> Result<Vec<TraceSpanRecord>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, iteration, span_type, tool_name, tool_params, success, error_code, error_msg, duration_ms
                 FROM trace_spans WHERE session_id = ?1 AND span_type = 'tool_exec' ORDER BY id DESC LIMIT ?2",
            )?;

            let mut rows: Vec<TraceSpanRecord> = stmt
                .query_map(params![session_id, limit], row_to_trace_span)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            rows.reverse();
            Ok(rows)
        })
    }

    /// Get tool execution spans across all sessions with id > `after_id`.
    ///
    /// Returns up to `limit` spans in ascending id order. Used by the
    /// learning pipeline to process new traces incrementally.
    pub fn tool_spans_since(&self, after_id: i64, limit: u32) -> Result<Vec<TraceSpanRecord>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, iteration, span_type, tool_name, tool_params, success, error_code, error_msg, duration_ms
                 FROM trace_spans WHERE id > ?1 AND span_type = 'tool_exec' ORDER BY id ASC LIMIT ?2",
            )?;

            let rows: Vec<TraceSpanRecord> = stmt
                .query_map(params![after_id, limit], row_to_trace_span)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Delete all trace spans for a session.
    ///
    /// Returns the number of deleted rows.
    pub fn delete_by_session(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "DELETE FROM trace_spans WHERE session_id = ?1",
                params![session_id],
            )?;
            Ok(rows_affected as u32)
        })
    }

    /// Count all trace spans for a session.
    pub fn count_by_session(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM trace_spans WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?;
            Ok(count as u32)
        })
    }
}

/// Convert a database row to a TraceSpanRecord.
fn row_to_trace_span(row: &rusqlite::Row<'_>) -> rusqlite::Result<TraceSpanRecord> {
    let id: i64 = row.get(0)?;
    let session_id: String = row.get(1)?;
    let iteration: u32 = row.get(2)?;
    let span_type: String = row.get(3)?;
    let tool_name: Option<String> = row.get(4)?;
    let tool_params: Option<String> = row.get(5)?;
    let success_int: i32 = row.get(6)?;
    let error_code: Option<String> = row.get(7)?;
    let error_msg: Option<String> = row.get(8)?;
    let duration_ms: Option<i64> = row.get(9)?;

    Ok(TraceSpanRecord {
        id,
        session_id,
        iteration,
        span_type,
        tool_name,
        tool_params,
        success: success_int != 0,
        error_code,
        error_msg,
        duration_ms: duration_ms.map(|v| v as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    #[test]
    fn test_create_and_list_spans() {
        let storage = Storage::open_in_memory().unwrap();

        let repo = TraceRepository::new(storage.database());

        // Create an LLM call span
        let id1 = repo
            .create(CreateTraceSpanParams {
                session_id: "sess-1".to_string(),
                iteration: 1,
                span_type: "llm_call".to_string(),
                tool_name: None,
                tool_params: None,
                success: true,
                error_code: None,
                error_msg: None,
                duration_ms: Some(150),
            })
            .unwrap();
        assert!(id1 > 0);

        // Create a tool execution span
        let id2 = repo
            .create(CreateTraceSpanParams {
                session_id: "sess-1".to_string(),
                iteration: 1,
                span_type: "tool_exec".to_string(),
                tool_name: Some("bash".to_string()),
                tool_params: Some("{\"cmd\":\"ls\"}".to_string()),
                success: false,
                error_code: Some("TIMEOUT".to_string()),
                error_msg: Some("Command timed out".to_string()),
                duration_ms: Some(5000),
            })
            .unwrap();
        assert!(id2 > id1);

        // List spans for the session
        let spans = repo.list_by_session("sess-1").unwrap();
        assert_eq!(spans.len(), 2);

        // Verify order: by iteration then id
        assert_eq!(spans[0].id, id1);
        assert_eq!(spans[1].id, id2);

        // Verify first span fields
        assert_eq!(spans[0].session_id, "sess-1");
        assert_eq!(spans[0].iteration, 1);
        assert_eq!(spans[0].span_type, "llm_call");
        assert!(spans[0].tool_name.is_none());
        assert!(spans[0].tool_params.is_none());
        assert!(spans[0].success);
        assert!(spans[0].error_code.is_none());
        assert!(spans[0].error_msg.is_none());
        assert_eq!(spans[0].duration_ms, Some(150));

        // Verify second span fields
        assert_eq!(spans[1].span_type, "tool_exec");
        assert_eq!(spans[1].tool_name, Some("bash".to_string()));
        assert_eq!(spans[1].tool_params, Some("{\"cmd\":\"ls\"}".to_string()));
        assert!(!spans[1].success);
        assert_eq!(spans[1].error_code, Some("TIMEOUT".to_string()));
        assert_eq!(spans[1].error_msg, Some("Command timed out".to_string()));
        assert_eq!(spans[1].duration_ms, Some(5000));
    }

    #[test]
    fn test_recent_tool_spans() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = TraceRepository::new(storage.database());

        // Create 5 tool_exec spans
        for i in 1..=5 {
            repo.create(CreateTraceSpanParams {
                session_id: "sess-1".to_string(),
                iteration: i,
                span_type: "tool_exec".to_string(),
                tool_name: Some(format!("tool-{}", i)),
                tool_params: None,
                success: true,
                error_code: None,
                error_msg: None,
                duration_ms: Some(i as u64 * 100),
            })
            .unwrap();
        }

        // Query last 3
        let spans = repo.recent_tool_spans("sess-1", 3).unwrap();
        assert_eq!(spans.len(), 3);

        // Verify chronological order (ascending by id after reverse)
        assert_eq!(spans[0].tool_name, Some("tool-3".to_string()));
        assert_eq!(spans[1].tool_name, Some("tool-4".to_string()));
        assert_eq!(spans[2].tool_name, Some("tool-5".to_string()));

        // Verify ordering is ascending
        assert!(spans[0].id < spans[1].id);
        assert!(spans[1].id < spans[2].id);
    }

    #[test]
    fn test_delete_by_session() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = TraceRepository::new(storage.database());

        // Create spans in two sessions
        for i in 1..=3 {
            repo.create(CreateTraceSpanParams {
                session_id: "sess-1".to_string(),
                iteration: i,
                span_type: "llm_call".to_string(),
                tool_name: None,
                tool_params: None,
                success: true,
                error_code: None,
                error_msg: None,
                duration_ms: None,
            })
            .unwrap();
        }

        for i in 1..=2 {
            repo.create(CreateTraceSpanParams {
                session_id: "sess-2".to_string(),
                iteration: i,
                span_type: "tool_exec".to_string(),
                tool_name: Some("bash".to_string()),
                tool_params: None,
                success: true,
                error_code: None,
                error_msg: None,
                duration_ms: None,
            })
            .unwrap();
        }

        // Verify initial counts
        assert_eq!(repo.count_by_session("sess-1").unwrap(), 3);
        assert_eq!(repo.count_by_session("sess-2").unwrap(), 2);

        // Delete session 1 spans
        let deleted = repo.delete_by_session("sess-1").unwrap();
        assert_eq!(deleted, 3);

        // Verify session 1 is empty, session 2 is untouched
        assert_eq!(repo.count_by_session("sess-1").unwrap(), 0);
        assert_eq!(repo.count_by_session("sess-2").unwrap(), 2);
    }
}
