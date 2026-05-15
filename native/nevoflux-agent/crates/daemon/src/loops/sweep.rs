//! Startup sweep — handles loops left in `running` state by an unclean
//! shutdown (spec §15).
//!
//! Behaviour:
//! - Any `loop_iterations` row with `status = 'running'` is updated to
//!   `cancelled` with `error_message = "orphaned by crash"`.
//! - Any `loops` row with `state = 'running'` is updated to `state = failed`
//!   and its `consecutive_failures` is incremented.
//! - MVP does NOT auto-resume any loops; user must re-create.

use nevoflux_storage::connection::Database;
use nevoflux_storage::models::current_timestamp;

/// Run the sweep against the given database. Idempotent — safe to call
/// multiple times (subsequent calls find nothing to mark).
pub fn run_startup_sweep(db: &Database) -> Result<(), nevoflux_storage::error::StorageError> {
    let now = current_timestamp();
    db.with_connection(|conn| {
        conn.execute(
            "UPDATE loop_iterations
             SET status = 'cancelled',
                 ended_at = ?1,
                 error_message = 'orphaned by crash'
             WHERE status = 'running'",
            rusqlite::params![now],
        )?;
        conn.execute(
            "UPDATE loops
             SET state = 'failed',
                 consecutive_failures = consecutive_failures + 1,
                 updated_at = ?1
             WHERE state = 'running'",
            rusqlite::params![now],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::models::{
        CreateSessionParams, IterationStatus, LoopRecord, LoopState,
    };
    use nevoflux_storage::Storage;

    #[test]
    fn sweep_marks_orphaned_running_as_failed() {
        let storage = Storage::open_in_memory().unwrap();
        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("s1").with_title("t"))
            .unwrap();

        let repo = storage.loops();
        repo.create(&LoopRecord {
            id: "x".into(),
            session_id: "s1".into(),
            trigger_expr: "time:5m".into(),
            prompt_text: Some("p".into()),
            wrapped_skill: None,
            mode: "chat".into(),
            scratchpad: String::new(),
            state: LoopState::Running,
            consecutive_failures: 0,
            skipped_triggers: 0,
            iteration_count: 0,
            created_at: 0,
            updated_at: 0,
        })
        .unwrap();
        repo.insert_iteration("x", 1, 0, IterationStatus::Running).unwrap();

        run_startup_sweep(storage.database()).unwrap();

        let rec = repo.get("x").unwrap().unwrap();
        assert_eq!(rec.state, LoopState::Failed);
        assert_eq!(rec.consecutive_failures, 1);

        // Verify the iteration row was also cancelled.
        let row: (String, Option<String>) = storage
            .database()
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT status, error_message FROM loop_iterations WHERE loop_id = ?1",
                    rusqlite::params!["x"],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                        ))
                    },
                )
                .map_err(nevoflux_storage::error::StorageError::from)
            })
            .unwrap();
        assert_eq!(row.0, "cancelled");
        assert_eq!(row.1.as_deref(), Some("orphaned by crash"));
    }

    #[test]
    fn sweep_is_idempotent() {
        let storage = Storage::open_in_memory().unwrap();
        run_startup_sweep(storage.database()).unwrap();
        run_startup_sweep(storage.database()).unwrap();
    }
}
