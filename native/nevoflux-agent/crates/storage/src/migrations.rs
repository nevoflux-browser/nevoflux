//! Database migrations for the storage layer.

use rusqlite::Connection;

use crate::error::{Result, StorageError};

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", include_str!("migrations/001_initial.sql")),
    ("002_memory", include_str!("migrations/002_memory.sql")),
    ("003_traces", include_str!("migrations/003_traces.sql")),
    (
        "004_artifacts",
        include_str!("migrations/004_artifacts.sql"),
    ),
    ("005_learning", include_str!("migrations/005_learning.sql")),
    (
        "006_knowledge_embedding",
        include_str!("migrations/006_knowledge_embedding.sql"),
    ),
    (
        "007_knowledge_hot",
        include_str!("migrations/007_knowledge_hot.sql"),
    ),
    (
        "008_message_index",
        include_str!("migrations/008_message_index.sql"),
    ),
    (
        "009_fts_trigram",
        include_str!("migrations/009_fts_trigram.sql"),
    ),
    (
        "010_event_bus",
        include_str!("migrations/010_event_bus.sql"),
    ),
    (
        "011_canvas_tool_audit",
        include_str!("migrations/011_canvas_tool_audit.sql"),
    ),
    (
        "012_canvas_share",
        include_str!("migrations/012_canvas_share.sql"),
    ),
    // Note: 013 was reserved and abandoned; 014 follows 012 intentionally.
    (
        "014_artifacts_persistent",
        include_str!("migrations/014_artifacts_persistent.sql"),
    ),
    (
        "015_artifacts_files_canonical",
        include_str!("migrations/015_artifacts_files_canonical.sql"),
    ),
    (
        "016_composition_assets",
        include_str!("migrations/016_composition_assets.sql"),
    ),
    ("017_loops", include_str!("migrations/017_loops.sql")),
    (
        "018_loops_mode",
        include_str!("migrations/018_loops_mode.sql"),
    ),
];

/// Run all pending migrations on the given connection.
pub fn run_all(conn: &mut Connection) -> Result<()> {
    // Create migrations tracking table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Run each migration if not already applied
    for (name, sql) in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)",
            [name],
            |row| row.get(0),
        )?;

        if !already_applied {
            // Some migrations (e.g. table rebuilds) toggle foreign_keys OFF;
            // reassert ON after every migration to recover from any failure path.
            let result = conn
                .execute_batch(sql)
                .map_err(|e| StorageError::Migration(format!("Migration {} failed: {}", name, e)));
            conn.execute_batch("PRAGMA foreign_keys = ON;")
                .map_err(|e| {
                    StorageError::Migration(format!(
                        "Migration {} failed to restore foreign_keys: {}",
                        name, e
                    ))
                })?;
            result?;

            conn.execute(
                "INSERT INTO _migrations (name, applied_at) VALUES (?, strftime('%s', 'now'))",
                [name],
            )?;
        }
    }

    // Post-SQL data migrations. Each function tracks its own applied
    // state in `_migrations` (typically under a name like
    // "<XXX>b_<description>"), so re-running run_all is a no-op once
    // the data move has completed.
    crate::repositories::composition_asset::migrate_assets_to_composition_assets_table(conn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        run_all(&mut conn).unwrap();
        run_all(&mut conn).unwrap();

        // Should still work
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        // 16 SQL migrations (001-012, 014, 015, 016, 017, 018) + 1 Rust post-migration
        // marker (016b_composition_assets_data) = 18.
        assert_eq!(count, 18);
    }

    #[test]
    fn test_trace_spans_table_created() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO trace_spans (session_id, iteration, span_type, success)
             VALUES ('sess-1', 0, 'tool_exec', 0)",
            [],
        )
        .unwrap();

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM trace_spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_trace_spans_cleanup_by_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO trace_spans (session_id, iteration, span_type, success)
             VALUES ('sess-1', 0, 'llm_call', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trace_spans (session_id, iteration, span_type, success)
             VALUES ('sess-2', 0, 'tool_exec', 0)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM trace_spans WHERE session_id = 'sess-1'", [])
            .unwrap();

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM trace_spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_005_creates_learning_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        // Verify tables exist
        for table in &[
            "knowledge",
            "site_adaptations",
            "tool_stats",
            "learning_metrics",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Table {} should exist", table);
        }

        // Verify view exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='knowledge_health'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "View knowledge_health should exist");
    }

    #[test]
    fn migration_010_creates_event_bus_persistent_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        // Verify table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='event_bus_persistent'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "Table event_bus_persistent should exist");

        // Verify indexes exist
        for idx in &["idx_ebp_topic", "idx_ebp_expires_at", "idx_ebp_created_at"] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='{}'",
                        idx
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Index {} should exist", idx);
        }

        // Verify we can INSERT and SELECT
        conn.execute(
            "INSERT INTO event_bus_persistent (id, topic, payload, publisher_kind, publisher_id, created_at, expires_at)
             VALUES ('evt-001', 'task:status', '{\"status\":\"done\"}', 'agent', 'agent-1', strftime('%s','now'), strftime('%s','now') + 3600)",
            [],
        )
        .unwrap();

        let payload: String = conn
            .query_row(
                "SELECT payload FROM event_bus_persistent WHERE id = 'evt-001'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(payload.contains("done"));
    }

    #[test]
    fn migration_011_creates_canvas_tool_invocations_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='canvas_tool_invocations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "Table canvas_tool_invocations should exist");

        // Verify INSERT works
        conn.execute(
            "INSERT INTO canvas_tool_invocations (id, session_id, tool_name, backend_kind, args_json, status, started_at)
             VALUES ('inv-001', 'sess-001', 'ffmpeg.trim', 'command', '[]', 'started', strftime('%s','now'))",
            [],
        )
        .unwrap();

        let row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM canvas_tool_invocations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn migration_011_indexes() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        for idx in &[
            "idx_canvas_invocations_session",
            "idx_canvas_invocations_tool",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='{}'",
                        idx
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Index {} should exist", idx);
        }
    }

    /// Run migrations up to (but not including) the migration named `stop_before`.
    /// Uses name-based lookup so that inserting a new migration before the stop
    /// point does not silently slice at the wrong index.
    fn run_migrations_up_to(conn: &mut Connection, stop_before: &str) -> Result<()> {
        let count = MIGRATIONS
            .iter()
            .position(|(name, _)| *name == stop_before)
            .unwrap_or_else(|| panic!("stop_before migration {stop_before:?} not found"));

        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL
            )",
            [],
        )?;

        for (name, sql) in &MIGRATIONS[..count] {
            let already_applied: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)",
                [name],
                |row| row.get(0),
            )?;

            if !already_applied {
                // Some migrations (e.g. table rebuilds) toggle foreign_keys OFF;
                // reassert ON after every migration to recover from any failure path.
                let result = conn.execute_batch(sql).map_err(|e| {
                    StorageError::Migration(format!("Migration {} failed: {}", name, e))
                });
                conn.execute_batch("PRAGMA foreign_keys = ON;")
                    .map_err(|e| {
                        StorageError::Migration(format!(
                            "Migration {} failed to restore foreign_keys: {}",
                            name, e
                        ))
                    })?;
                result?;

                conn.execute(
                    "INSERT INTO _migrations (name, applied_at) VALUES (?, strftime('%s', 'now'))",
                    [name],
                )?;
            }
        }

        Ok(())
    }

    #[test]
    fn migration_014_artifacts_persistent() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Run all migrations up to (but not including) 014 to set up the pre-014 schema.
        run_migrations_up_to(&mut conn, "014_artifacts_persistent").unwrap();

        // Insert a session to attach artifacts to.
        conn.execute(
            "INSERT INTO sessions (id, created_at, updated_at) VALUES ('sess-a', 1000, 1000)",
            [],
        )
        .unwrap();

        // Insert an imported artifact (has imported_from_share_id, so should become persistent).
        conn.execute(
            "INSERT INTO artifacts
             (id, session_id, title, content_type, content, imported_from_share_id, imported_at, created_at)
             VALUES ('art-imported', 'sess-a', 'Imported', 'text/html', '<h1/>', 'share-001', 2000, 1500)",
            [],
        )
        .unwrap();

        // Insert a regular (non-imported) artifact.
        conn.execute(
            "INSERT INTO artifacts
             (id, session_id, title, content_type, content, created_at)
             VALUES ('art-regular', 'sess-a', 'Regular', 'text/html', '<p/>', 3000)",
            [],
        )
        .unwrap();

        // Apply migration 014 directly by running its SQL — this exercises the backfill.
        let sql_014 = include_str!("migrations/014_artifacts_persistent.sql");
        conn.execute_batch(sql_014).unwrap();

        // --- (a) imported artifact should be persistent ---
        let (is_persistent, persisted_at, updated_at): (i32, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT is_persistent, persisted_at, updated_at FROM artifacts WHERE id = 'art-imported'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(
            is_persistent, 1,
            "imported artifact should have is_persistent=1"
        );
        assert_eq!(
            persisted_at,
            Some(2000),
            "persisted_at should equal imported_at (2000)"
        );
        assert_eq!(
            updated_at,
            Some(2000),
            "updated_at should equal imported_at (2000)"
        );

        // --- (b) regular artifact should NOT be persistent ---
        let (is_persistent2, persisted_at2, updated_at2): (i32, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT is_persistent, persisted_at, updated_at FROM artifacts WHERE id = 'art-regular'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(
            is_persistent2, 0,
            "regular artifact should have is_persistent=0"
        );
        assert!(
            persisted_at2.is_none(),
            "regular artifact should have persisted_at IS NULL"
        );
        assert_eq!(
            updated_at2,
            Some(3000),
            "regular artifact updated_at should equal created_at (3000)"
        );

        // --- (c) session_id becomes nullable; FK is now SET NULL ---
        // After migration 014, deleting a session should set artifact.session_id to NULL
        // rather than cascade-delete the artifact.
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("DELETE FROM sessions WHERE id = 'sess-a'", [])
            .unwrap();

        // The imported persistent artifact should still exist with session_id = NULL.
        let session_id: Option<String> = conn
            .query_row(
                "SELECT session_id FROM artifacts WHERE id = 'art-imported'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(
            session_id.is_none(),
            "persistent artifact should survive session deletion with session_id = NULL"
        );

        // --- (d) indexes exist ---
        for idx in &[
            "idx_artifacts_persistent",
            "idx_artifacts_session",
            "idx_artifacts_imported",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='{}'",
                        idx
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Index {} should exist", idx);
        }
    }

    #[test]
    fn migration_012_creates_canvas_share_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_all(&mut conn).unwrap();

        // Verify tables
        for table in &["artifact_shares", "artifact_share_history"] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Table {} should exist", table);
        }

        // Verify new columns on artifacts - INSERT should work
        conn.execute(
            "INSERT INTO sessions (id, created_at, updated_at) VALUES ('test-sess', strftime('%s','now'), strftime('%s','now'))",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, session_id, title, content_type, content, imported_from_url, imported_from_share_id, imported_at)
             VALUES ('art-test', 'test-sess', 'Test', 'text/html', '<h1>Hi</h1>', 'https://share.nevoflux.com/abc', 'abc', strftime('%s','now'))",
            [],
        ).unwrap();

        // Verify INSERT into artifact_shares works
        conn.execute(
            "INSERT INTO artifact_shares (artifact_id, share_id, share_url, encrypted_password, encrypted_owner_token, expires_at, view_count, created_at)
             VALUES ('art-test', 'share-001', 'https://share.nevoflux.com/share-001', 'enc-pw', 'enc-tok', strftime('%s','now') + 2592000, 0, strftime('%s','now'))",
            [],
        ).unwrap();

        // Verify indexes
        for idx in &[
            "idx_artifact_shares_artifact_id",
            "idx_artifact_shares_share_id",
            "idx_artifact_shares_expires_at",
            "idx_artifact_share_history_artifact_id",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='{}'",
                        idx
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Index {} should exist", idx);
        }
    }
}
