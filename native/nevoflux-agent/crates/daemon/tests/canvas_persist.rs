//! Integration tests for the My Canvas persistence feature.

use nevoflux_daemon::session::SessionManager;
use rusqlite::params;

/// Verify that `delete_session` removes non-persistent artifacts and leaves
/// persistent artifacts alive (with `session_id` set to NULL via the FK
/// `ON DELETE SET NULL` rule introduced in migration 014).
#[tokio::test]
async fn delete_session_keeps_persistent_artifacts_and_drops_non_persistent() {
    let manager = SessionManager::in_memory().unwrap();

    // Create a session via the manager.
    let session = manager.create_session(None, None).await.unwrap();

    // Seed two artifacts directly via SQL:
    //   - "p": persistent (is_persistent = 1)
    //   - "n": non-persistent (is_persistent = 0)
    manager
        .storage()
        .database()
        .with_connection_mut(|conn| {
            conn.execute(
                "INSERT INTO artifacts
                     (id, session_id, title, content_type, content, created_at,
                      is_persistent, persisted_at, updated_at)
                 VALUES ('p', ?1, 'p', 'text/html', '', 1, 1, 1, 1)",
                params![session.id],
            )?;
            conn.execute(
                "INSERT INTO artifacts
                     (id, session_id, title, content_type, content, created_at,
                      is_persistent, updated_at)
                 VALUES ('n', ?1, 'n', 'text/html', '', 1, 0, 1)",
                params![session.id],
            )?;
            Ok(())
        })
        .unwrap();

    manager.delete_session(&session.id).await.unwrap();

    // Persistent row must still exist; session_id must be NULL (FK SET NULL).
    let p_sid: Option<String> = manager
        .storage()
        .database()
        .with_connection(|c| {
            Ok(
                c.query_row("SELECT session_id FROM artifacts WHERE id = 'p'", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(
        p_sid, None,
        "persistent artifact should survive with session_id = NULL"
    );

    // Non-persistent row must be gone.
    let n_exists: i64 = manager
        .storage()
        .database()
        .with_connection(|c| {
            Ok(
                c.query_row("SELECT COUNT(*) FROM artifacts WHERE id = 'n'", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(n_exists, 0, "non-persistent artifact should be deleted");
}
