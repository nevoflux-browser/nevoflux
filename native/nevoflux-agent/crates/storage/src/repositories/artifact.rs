//! Repository for artifact persistence.

use rusqlite::params;

use crate::connection::Database;
use crate::error::Result;
use crate::models::artifact::{ArtifactRecord, CreateArtifactParams};

/// Repository for artifact CRUD operations.
pub struct ArtifactRepository<'a> {
    db: &'a Database,
}

impl<'a> ArtifactRepository<'a> {
    /// Create a new artifact repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Idempotent upsert for an artifact.
    ///
    /// On INSERT: writes all mutable fields plus `created_at` and `updated_at` (both set to now).
    /// `is_persistent`, `persisted_at`, `imported_from_*` are left at their column defaults (0 / NULL).
    ///
    /// On CONFLICT (same `id`): updates only the content-carrying fields
    /// (`session_id`, `title`, `description`, `content_type`, `content`, `files`, `entry`,
    /// `updated_at`). The persistence fields (`is_persistent`, `persisted_at`, `created_at`,
    /// `imported_from_url`, `imported_from_share_id`, `imported_at`) are intentionally
    /// EXCLUDED from the DO UPDATE clause so they are never overwritten by a re-render.
    ///
    /// Returns the authoritative row re-read from the database so that the caller always
    /// sees the preserved field values on update.
    pub fn create(&self, params: CreateArtifactParams) -> Result<ArtifactRecord> {
        // Storage-layer invariant for ALL artifacts (Phase C):
        //   * `entry` is always set (defaults to "main.html" for legacy callers)
        //   * `files` always contains at least one entry (synthesized as
        //     `{entry: content}` for legacy single-file callers)
        //   * `content` := `files[entry]` (mirror, not independent source)
        //
        // This eliminates the historical dual-write hazard between content
        // and files[entry]. Legacy callers using `with_content("...")` only
        // continue to work — the storage layer synthesizes the multi-file
        // shape for them.
        //
        // See architecture_artifact_files_invariant.md and migration 015.
        let entry = params
            .entry
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "main.html".to_string());
        let mut files_map = params.files.clone().unwrap_or_default();
        if files_map.is_empty() {
            files_map.insert(entry.clone(), params.content.clone());
        } else if !files_map.contains_key(&entry) {
            // Multi-file artifact whose entry doesn't point into the map —
            // shouldn't happen for canvas_video, but be defensive: insert
            // params.content under the chosen entry key.
            files_map.insert(entry.clone(), params.content.clone());
        }
        let content = files_map
            .get(&entry)
            .cloned()
            .unwrap_or_else(|| params.content.clone());
        let files_json = serde_json::to_string(&files_map).unwrap_or_else(|_| "{}".to_string());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO artifacts (id, session_id, title, description, content_type, content, files, entry, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                     session_id   = excluded.session_id,
                     title        = excluded.title,
                     description  = excluded.description,
                     content_type = excluded.content_type,
                     content      = excluded.content,
                     files        = excluded.files,
                     entry        = excluded.entry,
                     updated_at   = excluded.updated_at",
                params![
                    params.id,
                    params.session_id,
                    params.title,
                    params.description,
                    params.content_type,
                    content,
                    files_json,
                    entry,
                    now,
                ],
            )?;

            // Re-read the authoritative row so preserved fields are visible on update.
            let record = conn.query_row(
                "SELECT id, session_id, title, description, content_type, content, files, entry,
                        created_at, imported_from_url, imported_from_share_id, imported_at,
                        is_persistent, persisted_at, updated_at
                 FROM artifacts WHERE id = ?1",
                params![params.id],
                row_to_artifact,
            )?;

            Ok(record)
        })
    }

    /// Update only the `files` map of an existing artifact, with `content`
    /// auto-derived from `files[entry]` (the row's existing entry value).
    /// Leaves title / session_id / entry / description / content_type /
    /// persistence flags untouched.
    ///
    /// The `content` parameter is now ignored except as a fallback when
    /// the row's `entry` field doesn't resolve in the new files map (which
    /// shouldn't happen post-migration 015 since `entry` is NOT NULL and
    /// must point into `files`). Storage layer enforces the invariant
    /// `content := files[entry]` so no caller can produce drift.
    ///
    /// Returns `Ok(false)` if no row matched the id.
    pub fn update_files(
        &self,
        id: &str,
        files: &std::collections::HashMap<String, String>,
        content: &str,
    ) -> Result<bool> {
        let files_json = serde_json::to_string(files)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        self.db.with_connection(|conn| {
            // Look up the row's `entry` so we can derive content := files[entry].
            // Falls back to caller-supplied content if entry doesn't resolve
            // (defensive against pre-migration data).
            let entry: Option<String> = conn
                .query_row(
                    "SELECT entry FROM artifacts WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .ok();
            let derived_content = entry
                .as_deref()
                .and_then(|e| files.get(e).cloned())
                .unwrap_or_else(|| content.to_string());

            let rows = conn.execute(
                "UPDATE artifacts
                 SET files = ?1, content = ?2, updated_at = ?3
                 WHERE id = ?4",
                params![files_json, derived_content, now, id],
            )?;
            Ok(rows > 0)
        })
    }

    /// Get a full artifact by ID.
    pub fn get(&self, id: &str) -> Result<Option<ArtifactRecord>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, title, description, content_type, content, files, entry,
                        created_at, imported_from_url, imported_from_share_id, imported_at,
                        is_persistent, persisted_at, updated_at
                 FROM artifacts WHERE id = ?1",
            )?;

            let mut rows = stmt.query_map(params![id], row_to_artifact)?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
    }

    /// List artifacts for a session (summaries only — empty content, no files).
    pub fn list_by_session(&self, session_id: &str) -> Result<Vec<ArtifactRecord>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, title, description, content_type, created_at,
                        is_persistent, persisted_at, updated_at
                 FROM artifacts WHERE session_id = ?1 ORDER BY created_at ASC",
            )?;

            let rows = stmt
                .query_map(params![session_id], |row| {
                    let updated_at: Option<i64> = row.get(8)?;
                    let created_at: i64 = row.get(5)?;
                    Ok(ArtifactRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        title: row.get(2)?,
                        description: row.get(3)?,
                        content_type: row.get(4)?,
                        content: String::new(),
                        files: None,
                        entry: None,
                        created_at,
                        imported_from_url: None,
                        imported_from_share_id: None,
                        imported_at: None,
                        is_persistent: row.get::<_, i64>(6)? != 0,
                        persisted_at: row.get(7)?,
                        updated_at: updated_at.unwrap_or(created_at),
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Delete only non-persistent artifacts for a session.
    ///
    /// Returns the IDs of the deleted rows so the caller can clean any
    /// corresponding ContentStore mirror entries.  Returns an empty Vec
    /// (never panics / never None) when there are no matching rows.
    pub fn delete_non_persistent_by_session(&self, session_id: &str) -> Result<Vec<String>> {
        self.db.with_connection_mut(|conn| {
            let tx = conn.transaction()?;
            let ids: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT id FROM artifacts \
                     WHERE session_id = ?1 AND is_persistent = 0",
                )?;
                let collected = stmt
                    .query_map(params![session_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                collected
            };
            if !ids.is_empty() {
                tx.execute(
                    "DELETE FROM artifacts \
                     WHERE session_id = ?1 AND is_persistent = 0",
                    params![session_id],
                )?;
            }
            tx.commit()?;
            Ok(ids)
        })
    }

    /// Delete all artifacts for a session.
    ///
    /// # Deprecated
    ///
    /// Use [`delete_non_persistent_by_session`] for session cleanup after migration 014.
    /// Persistent artifacts now survive session deletion via the FK `ON DELETE SET NULL`
    /// rule; calling this method would incorrectly remove them.
    /// The method is kept for the existing `test_delete_by_session` test and any callers
    /// that have not yet been migrated.  It will be removed when Task 11 lands.
    #[deprecated(
        note = "Use `delete_non_persistent_by_session` + session FK SET NULL for persistent rows (see migration 014)"
    )]
    pub fn delete_by_session(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "DELETE FROM artifacts WHERE session_id = ?1",
                params![session_id],
            )?;
            Ok(rows_affected as u32)
        })
    }
}

/// Convert a full database row to an ArtifactRecord.
///
/// Column order must match the SELECT lists in [`ArtifactRepository::create`] and
/// [`ArtifactRepository::get`]:
///
/// 0  id
/// 1  session_id
/// 2  title
/// 3  description
/// 4  content_type
/// 5  content
/// 6  files
/// 7  entry
/// 8  created_at
/// 9  imported_from_url
/// 10 imported_from_share_id
/// 11 imported_at
/// 12 is_persistent
/// 13 persisted_at
/// 14 updated_at
fn row_to_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRecord> {
    let id: String = row.get(0)?;
    let session_id: Option<String> = row.get(1)?;
    let title: String = row.get(2)?;
    let description: Option<String> = row.get(3)?;
    let content_type: String = row.get(4)?;
    let content: String = row.get(5)?;
    let files_json: Option<String> = row.get(6)?;
    let entry: Option<String> = row.get(7)?;
    let created_at: i64 = row.get(8)?;
    let imported_from_url: Option<String> = row.get(9)?;
    let imported_from_share_id: Option<String> = row.get(10)?;
    let imported_at: Option<i64> = row.get(11)?;
    let is_persistent_raw: i64 = row.get(12)?;
    let persisted_at: Option<i64> = row.get(13)?;
    let updated_at_opt: Option<i64> = row.get(14)?;

    let files = files_json.and_then(|j| serde_json::from_str(&j).ok());
    let is_persistent = is_persistent_raw != 0;
    let updated_at = updated_at_opt.unwrap_or(created_at);

    Ok(ArtifactRecord {
        id,
        session_id,
        title,
        description,
        content_type,
        content,
        files,
        entry,
        created_at,
        imported_from_url,
        imported_from_share_id,
        imported_at,
        is_persistent,
        persisted_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;
    use std::collections::HashMap;

    #[test]
    fn test_create_and_get_artifact() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        // Create a session first (artifacts reference sessions)
        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-1"))
            .unwrap();

        let params = CreateArtifactParams::new("art-1", "sess-1", "My Page", "text/html")
            .with_description("A simple page")
            .with_content("<h1>Hello</h1>");

        let record = repo.create(params).unwrap();
        assert_eq!(record.id, "art-1");
        assert_eq!(record.title, "My Page");
        assert_eq!(record.description, Some("A simple page".to_string()));
        assert_eq!(record.content, "<h1>Hello</h1>");

        // Get it back
        let fetched = repo.get("art-1").unwrap().unwrap();
        assert_eq!(fetched.id, "art-1");
        assert_eq!(fetched.content, "<h1>Hello</h1>");
        assert_eq!(fetched.description, Some("A simple page".to_string()));
    }

    #[test]
    fn test_get_nonexistent_artifact() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_create_artifact_with_files() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-1"))
            .unwrap();

        let mut files = HashMap::new();
        files.insert("src/App.jsx".to_string(), "export default App;".to_string());
        files.insert(
            "src/index.jsx".to_string(),
            "import App from './App';".to_string(),
        );

        let params = CreateArtifactParams::new("art-2", "sess-1", "React App", "project")
            .with_files(files.clone())
            .with_entry("src/index.jsx");

        let record = repo.create(params).unwrap();
        assert_eq!(record.files.as_ref().unwrap().len(), 2);
        assert_eq!(record.entry, Some("src/index.jsx".to_string()));

        // Verify files survive round-trip
        let fetched = repo.get("art-2").unwrap().unwrap();
        let fetched_files = fetched.files.unwrap();
        assert_eq!(fetched_files.len(), 2);
        assert_eq!(fetched_files["src/App.jsx"], "export default App;");
    }

    #[test]
    fn test_list_by_session() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-1"))
            .unwrap();
        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-2"))
            .unwrap();

        repo.create(
            CreateArtifactParams::new("art-1", "sess-1", "Page 1", "text/html")
                .with_content("<h1>1</h1>"),
        )
        .unwrap();
        repo.create(
            CreateArtifactParams::new("art-2", "sess-1", "Page 2", "text/html")
                .with_content("<h1>2</h1>"),
        )
        .unwrap();
        repo.create(
            CreateArtifactParams::new("art-3", "sess-2", "Other", "text/html")
                .with_content("<h1>3</h1>"),
        )
        .unwrap();

        let list = repo.list_by_session("sess-1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "art-1");
        assert_eq!(list[1].id, "art-2");
        // Summaries: content should be empty, files should be None
        assert!(list[0].content.is_empty());
        assert!(list[0].files.is_none());

        let list2 = repo.list_by_session("sess-2").unwrap();
        assert_eq!(list2.len(), 1);
    }

    #[allow(deprecated)]
    #[test]
    fn test_delete_by_session() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-1"))
            .unwrap();

        repo.create(CreateArtifactParams::new(
            "art-1",
            "sess-1",
            "Page 1",
            "text/html",
        ))
        .unwrap();
        repo.create(CreateArtifactParams::new(
            "art-2",
            "sess-1",
            "Page 2",
            "text/html",
        ))
        .unwrap();

        let deleted = repo.delete_by_session("sess-1").unwrap();
        assert_eq!(deleted, 2);

        let list = repo.list_by_session("sess-1").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_upsert_artifact() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("sess-1"))
            .unwrap();

        repo.create(
            CreateArtifactParams::new("art-1", "sess-1", "V1", "text/html")
                .with_content("<h1>V1</h1>"),
        )
        .unwrap();

        // Upsert with new content
        repo.create(
            CreateArtifactParams::new("art-1", "sess-1", "V2", "text/html")
                .with_content("<h1>V2</h1>"),
        )
        .unwrap();

        let fetched = repo.get("art-1").unwrap().unwrap();
        assert_eq!(fetched.title, "V2");
        assert_eq!(fetched.content, "<h1>V2</h1>");

        // Should still be only one artifact
        let list = repo.list_by_session("sess-1").unwrap();
        assert_eq!(list.len(), 1);
    }

    // ----- Task 2+3 TDD tests -----

    /// Verifies that `create()` upsert does NOT overwrite `is_persistent` / `persisted_at`
    /// when the artifact already exists and those fields have been set externally.
    #[test]
    fn create_upsert_preserves_is_persistent_and_persisted_at() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("s1"))
            .unwrap();

        // First create — should be non-persistent
        repo.create(
            CreateArtifactParams::new("art-p", "s1", "Title", "text/html").with_content("v1"),
        )
        .unwrap();

        let initial = repo.get("art-p").unwrap().unwrap();
        assert!(
            !initial.is_persistent,
            "newly created artifact must not be persistent"
        );

        // Simulate the persist service flipping the flag directly in SQL.
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE artifacts SET is_persistent = 1, persisted_at = 999 WHERE id = 'art-p'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        // Upsert with updated content — persistence fields must be preserved.
        repo.create(
            CreateArtifactParams::new("art-p", "s1", "Title", "text/html").with_content("v2"),
        )
        .unwrap();

        let after = repo.get("art-p").unwrap().unwrap();
        assert_eq!(after.content, "v2", "content must be updated");
        assert!(
            after.is_persistent,
            "is_persistent must be preserved across upsert"
        );
        assert_eq!(
            after.persisted_at,
            Some(999),
            "persisted_at must be preserved across upsert"
        );
    }

    /// Verifies that `delete_non_persistent_by_session` removes only non-persistent rows
    /// and returns the deleted IDs.
    #[test]
    fn delete_non_persistent_by_session_returns_ids_and_leaves_persistent() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        storage
            .sessions()
            .create(crate::CreateSessionParams::new().with_id("s1"))
            .unwrap();

        // Create two artifacts.
        repo.create(
            CreateArtifactParams::new("p", "s1", "Persistent", "text/html").with_content("keep"),
        )
        .unwrap();
        repo.create(
            CreateArtifactParams::new("n", "s1", "Non-persistent", "text/html")
                .with_content("gone"),
        )
        .unwrap();

        // Flip "p" to persistent via direct SQL.
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE artifacts SET is_persistent = 1, persisted_at = 1000 WHERE id = 'p'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let deleted = repo.delete_non_persistent_by_session("s1").unwrap();
        assert_eq!(
            deleted,
            vec!["n".to_string()],
            "only the non-persistent ID must be returned"
        );

        // Persistent row must survive.
        assert!(
            repo.get("p").unwrap().is_some(),
            "persistent artifact must not be deleted"
        );
        // Non-persistent row must be gone.
        assert!(
            repo.get("n").unwrap().is_none(),
            "non-persistent artifact must be deleted"
        );
    }

    /// Verifies that creating an artifact with `session_id = None` succeeds and round-trips correctly.
    #[test]
    fn create_with_none_session_id_succeeds() {
        let storage = Storage::open_in_memory().unwrap();
        let repo = ArtifactRepository::new(storage.database());

        // No session needed — session_id IS NULL.
        let params = CreateArtifactParams::new_orphan("orphan-1", "Orphan Artifact", "text/html")
            .with_content("<p>orphan</p>");

        repo.create(params).unwrap();

        let fetched = repo.get("orphan-1").unwrap().unwrap();
        assert_eq!(
            fetched.session_id, None,
            "session_id must be None for an orphan artifact"
        );
        assert_eq!(fetched.content, "<p>orphan</p>");
    }
}
