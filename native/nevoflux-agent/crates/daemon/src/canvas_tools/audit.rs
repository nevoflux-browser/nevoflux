//! Audit logger for Canvas Tool invocations.
//!
//! Records every whitelisted-tool invocation into the `canvas_tool_invocations`
//! SQLite table (created by migration 011).  The logger supports a two-phase
//! pattern:
//!
//! 1. **`log_start`** -- inserts a record with `status = "started"` and returns
//!    a UUID that identifies the invocation.
//! 2. **`log_complete`** -- updates the record with the outcome once the tool
//!    finishes (or fails).
//!
//! An additional **`query_by_session`** method retrieves recent invocations for
//! display or debugging.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use uuid::Uuid;

use nevoflux_storage::Storage;

/// A single invocation record persisted in `canvas_tool_invocations`.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    /// Primary key (UUID v4).
    pub id: String,
    /// The session that triggered the invocation.
    pub session_id: String,
    /// Fully-qualified tool name (e.g. `"ffmpeg.trim"`).
    pub tool_name: String,
    /// Backend kind: `"command"` or `"internal"`.
    pub backend_kind: String,
    /// JSON-encoded argument list.
    pub args_json: String,
    /// Working directory at execution time (may be `None`).
    pub cwd: Option<String>,
    /// Status string: `"started"`, `"success"`, `"error"`, `"timeout"`, etc.
    pub status: String,
    /// Process exit code (populated on completion).
    pub exit_code: Option<i32>,
    /// Number of bytes captured from stdout.
    pub stdout_len: Option<i64>,
    /// Number of bytes captured from stderr.
    pub stderr_len: Option<i64>,
    /// Human-readable error message, if any.
    pub error_msg: Option<String>,
    /// Wall-clock execution time in milliseconds.
    pub duration_ms: Option<i64>,
    /// Unix epoch seconds when the invocation started.
    pub started_at: i64,
    /// Unix epoch seconds when the invocation finished (may be `None`).
    pub finished_at: Option<i64>,
}

/// Async-friendly audit logger backed by SQLite via [`nevoflux_storage::Storage`].
///
/// The struct is cheaply cloneable (`Arc`-based) and safe to share across
/// async tasks.
#[derive(Clone)]
pub struct AuditLogger {
    storage: Arc<Storage>,
}

impl AuditLogger {
    /// Create a new audit logger that writes to the given storage backend.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Record the start of a tool invocation.
    ///
    /// Inserts a row with `status = "started"` and returns the generated UUID
    /// so the caller can later call [`Self::log_complete`].
    pub fn log_start(
        &self,
        session_id: &str,
        tool_name: &str,
        backend_kind: &str,
        args_json: &str,
        cwd: Option<&str>,
    ) -> nevoflux_storage::Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.storage.database().with_connection(|conn| {
            conn.execute(
                "INSERT INTO canvas_tool_invocations \
                 (id, session_id, tool_name, backend_kind, args_json, cwd, status, started_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'started', ?7)",
                params![id, session_id, tool_name, backend_kind, args_json, cwd, now],
            )?;
            Ok(id)
        })
    }

    /// Record the completion of a previously-started invocation.
    ///
    /// Updates the row identified by `id` with outcome fields and sets
    /// `finished_at` to the current epoch second.
    pub fn log_complete(
        &self,
        id: &str,
        status: &str,
        exit_code: Option<i32>,
        stdout_len: Option<i64>,
        stderr_len: Option<i64>,
        error_msg: Option<&str>,
        duration_ms: Option<i64>,
    ) -> nevoflux_storage::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.storage.database().with_connection(|conn| {
            conn.execute(
                "UPDATE canvas_tool_invocations \
                 SET status = ?1, exit_code = ?2, stdout_len = ?3, stderr_len = ?4, \
                     error_msg = ?5, duration_ms = ?6, finished_at = ?7 \
                 WHERE id = ?8",
                params![
                    status,
                    exit_code,
                    stdout_len,
                    stderr_len,
                    error_msg,
                    duration_ms,
                    now,
                    id
                ],
            )?;
            Ok(())
        })
    }

    /// Query recent invocations for a session, ordered most-recent first.
    ///
    /// Returns at most `limit` records.
    pub fn query_by_session(
        &self,
        session_id: &str,
        limit: u32,
    ) -> nevoflux_storage::Result<Vec<AuditRecord>> {
        self.storage.database().with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, tool_name, backend_kind, args_json, cwd, \
                        status, exit_code, stdout_len, stderr_len, error_msg, \
                        duration_ms, started_at, finished_at \
                 FROM canvas_tool_invocations \
                 WHERE session_id = ?1 \
                 ORDER BY started_at DESC \
                 LIMIT ?2",
            )?;

            let rows = stmt
                .query_map(params![session_id, limit], row_to_audit_record)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }
}

/// Map a database row to an [`AuditRecord`].
fn row_to_audit_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRecord> {
    Ok(AuditRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        tool_name: row.get(2)?,
        backend_kind: row.get(3)?,
        args_json: row.get(4)?,
        cwd: row.get(5)?,
        status: row.get(6)?,
        exit_code: row.get(7)?,
        stdout_len: row.get(8)?,
        stderr_len: row.get(9)?,
        error_msg: row.get(10)?,
        duration_ms: row.get(11)?,
        started_at: row.get(12)?,
        finished_at: row.get(13)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_logger() -> AuditLogger {
        let storage = Storage::open_in_memory().unwrap();
        AuditLogger::new(Arc::new(storage))
    }

    #[test]
    fn test_log_start_inserts_record() {
        let logger = test_logger();

        let id = logger
            .log_start(
                "sess-1",
                "ffmpeg.trim",
                "command",
                r#"["--ss","10"]"#,
                Some("/tmp"),
            )
            .unwrap();

        // id should be a valid UUID
        assert!(
            Uuid::parse_str(&id).is_ok(),
            "returned id should be a valid UUID"
        );

        // Query the record back
        let records = logger.query_by_session("sess-1", 10).unwrap();
        assert_eq!(records.len(), 1);

        let rec = &records[0];
        assert_eq!(rec.id, id);
        assert_eq!(rec.session_id, "sess-1");
        assert_eq!(rec.tool_name, "ffmpeg.trim");
        assert_eq!(rec.backend_kind, "command");
        assert_eq!(rec.args_json, r#"["--ss","10"]"#);
        assert_eq!(rec.cwd, Some("/tmp".to_string()));
        assert_eq!(rec.status, "started");
        assert!(rec.exit_code.is_none());
        assert!(rec.stdout_len.is_none());
        assert!(rec.stderr_len.is_none());
        assert!(rec.error_msg.is_none());
        assert!(rec.duration_ms.is_none());
        assert!(rec.started_at > 0);
        assert!(rec.finished_at.is_none());
    }

    #[test]
    fn test_log_complete_updates_record() {
        let logger = test_logger();

        let id = logger
            .log_start("sess-1", "ripgrep", "command", "[]", None)
            .unwrap();

        logger
            .log_complete(
                &id,
                "success",
                Some(0),
                Some(1024),
                Some(0),
                None,
                Some(150),
            )
            .unwrap();

        let records = logger.query_by_session("sess-1", 10).unwrap();
        assert_eq!(records.len(), 1);

        let rec = &records[0];
        assert_eq!(rec.status, "success");
        assert_eq!(rec.exit_code, Some(0));
        assert_eq!(rec.stdout_len, Some(1024));
        assert_eq!(rec.stderr_len, Some(0));
        assert!(rec.error_msg.is_none());
        assert_eq!(rec.duration_ms, Some(150));
        assert!(rec.finished_at.is_some());
    }

    #[test]
    fn test_query_by_session() {
        let logger = test_logger();

        // Insert records in two sessions
        logger
            .log_start("sess-A", "tool1", "command", "[]", None)
            .unwrap();
        logger
            .log_start("sess-A", "tool2", "command", "[]", None)
            .unwrap();
        logger
            .log_start("sess-B", "tool3", "internal", "[]", None)
            .unwrap();

        // Query session A
        let a_records = logger.query_by_session("sess-A", 10).unwrap();
        assert_eq!(a_records.len(), 2);

        // Query session B
        let b_records = logger.query_by_session("sess-B", 10).unwrap();
        assert_eq!(b_records.len(), 1);
        assert_eq!(b_records[0].tool_name, "tool3");

        // Limit works
        let limited = logger.query_by_session("sess-A", 1).unwrap();
        assert_eq!(limited.len(), 1);

        // Non-existent session returns empty
        let empty = logger.query_by_session("sess-Z", 10).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_audit_logger_clone() {
        let logger = test_logger();
        let cloned = logger.clone();

        // Write through original, read through clone
        let id = logger
            .log_start("sess-1", "echo", "command", "[]", None)
            .unwrap();

        let records = cloned.query_by_session("sess-1", 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
    }
}
