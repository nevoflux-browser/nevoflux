//! Repository for composition assets (binary files attached to compositions).
//!
//! Assets live in their own table with raw BLOB bytes — distinct from
//! `artifacts.files` which carries only text files (DESIGN.md, index.html,
//! composition.meta.json). See migration 016 for the rationale.
//!
//! Writers: `canvas_video_service::attach_asset`,
//! `asset_server::handlers::upload::handle_asset`.
//!
//! Readers: `asset_server::handlers::asset` (HTTP GET),
//! `asset_server::handlers::composition` (URL list for rewrite),
//! `canvas_video::asset_inline::inline_assets` (share-export fallback).

use rusqlite::params;

use crate::connection::Database;
use crate::error::{Result, StorageError};

/// One composition asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionAsset {
    pub composition_id: String,
    pub name: String,
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub created_at: i64,
}

pub struct CompositionAssetRepository<'a> {
    db: &'a Database,
}

impl<'a> CompositionAssetRepository<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Idempotent upsert for a single asset.
    ///
    /// Repeat calls with the same (composition_id, name) overwrite bytes +
    /// mime + created_at — matching the "writer is authoritative" semantic
    /// used everywhere else in the storage crate.
    pub fn upsert(
        &self,
        composition_id: &str,
        name: &str,
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()> {
        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO composition_assets (composition_id, name, bytes, mime_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))
                 ON CONFLICT(composition_id, name) DO UPDATE SET
                     bytes      = excluded.bytes,
                     mime_type  = excluded.mime_type,
                     created_at = excluded.created_at",
                params![composition_id, name, bytes, mime_type],
            )?;
            Ok(())
        })
    }

    /// Read a single asset's bytes + mime. Returns None when missing.
    pub fn get(&self, composition_id: &str, name: &str) -> Result<Option<CompositionAsset>> {
        self.db.with_connection(|conn| {
            let row = conn
                .query_row(
                    "SELECT composition_id, name, bytes, mime_type, created_at
                     FROM composition_assets
                     WHERE composition_id = ?1 AND name = ?2",
                    params![composition_id, name],
                    |row| {
                        Ok(CompositionAsset {
                            composition_id: row.get(0)?,
                            name: row.get(1)?,
                            bytes: row.get(2)?,
                            mime_type: row.get(3)?,
                            created_at: row.get(4)?,
                        })
                    },
                )
                .ok();
            Ok(row)
        })
    }

    /// Return all asset names for a composition (sorted by name).
    /// Used by the Phase 2 composition handler to build the URL map.
    pub fn list_names(&self, composition_id: &str) -> Result<Vec<String>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT name FROM composition_assets
                 WHERE composition_id = ?1
                 ORDER BY name",
            )?;
            let rows = stmt
                .query_map(params![composition_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(StorageError::from)?;
            Ok(rows)
        })
    }

    /// Return every asset for a composition (bytes + mime). Used by the
    /// share-export inline_assets path to embed assets as data URIs.
    pub fn list_all(&self, composition_id: &str) -> Result<Vec<CompositionAsset>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT composition_id, name, bytes, mime_type, created_at
                 FROM composition_assets
                 WHERE composition_id = ?1
                 ORDER BY name",
            )?;
            let rows = stmt
                .query_map(params![composition_id], |row| {
                    Ok(CompositionAsset {
                        composition_id: row.get(0)?,
                        name: row.get(1)?,
                        bytes: row.get(2)?,
                        mime_type: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(StorageError::from)?;
            Ok(rows)
        })
    }

    /// Delete a single asset. Returns true if a row was removed.
    pub fn delete(&self, composition_id: &str, name: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let n = conn.execute(
                "DELETE FROM composition_assets
                 WHERE composition_id = ?1 AND name = ?2",
                params![composition_id, name],
            )?;
            Ok(n > 0)
        })
    }

    /// Delete every asset for a composition. Returns the number removed.
    /// Note: artifacts.files cascade FK already does this on artifact
    /// delete; this method exists for explicit cleanup paths.
    pub fn delete_all_for(&self, composition_id: &str) -> Result<u64> {
        self.db.with_connection(|conn| {
            let n = conn.execute(
                "DELETE FROM composition_assets WHERE composition_id = ?1",
                params![composition_id],
            )?;
            Ok(n as u64)
        })
    }
}

/// Migrate `assets/*` entries from `artifacts.files` JSON into the
/// dedicated `composition_assets` table. Idempotent — tracks completion
/// in `_migrations` as `016b_composition_assets_data`.
///
/// Called from `migrations::run_all` AFTER all SQL migrations have run
/// (the table must exist and the JSON parse logic needs the artifacts
/// table to be in its post-015 shape).
///
/// For each artifact whose `files` JSON contains `assets/*` keys:
///   1. Decode the base64 value into raw bytes
///   2. INSERT OR IGNORE into composition_assets
///   3. Remove the key from files JSON
///   4. Update artifact row (files + content where entry happens to
///      point at a no-longer-existing key, though that shouldn't happen
///      since text entries always lived under `index.html` etc.)
pub fn migrate_assets_to_composition_assets_table(
    conn: &mut rusqlite::Connection,
) -> Result<()> {
    const MARKER: &str = "016b_composition_assets_data";

    let applied: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?1)",
            params![MARKER],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if applied {
        return Ok(());
    }

    // Snapshot rows to migrate. Filter to candidates with assets/* keys
    // before doing the JSON parse work.
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, files, entry FROM artifacts
             WHERE files LIKE '%\"assets/%'",
        )?;
        let collected = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        collected
    };

    let tx = conn.transaction()?;
    let mut migrated_artifacts = 0u64;
    let mut migrated_assets = 0u64;
    for (artifact_id, files_json, entry) in rows {
        let mut files: std::collections::HashMap<String, String> =
            match serde_json::from_str(&files_json) {
                Ok(m) => m,
                Err(_) => continue, // malformed JSON — skip, don't fail boot
            };
        let asset_keys: Vec<String> = files
            .keys()
            .filter(|k| k.starts_with("assets/"))
            .cloned()
            .collect();
        if asset_keys.is_empty() {
            continue;
        }

        for key in &asset_keys {
            let value = match files.remove(key) {
                Some(v) => v,
                None => continue,
            };
            let name = match key.strip_prefix("assets/") {
                Some(s) => s.to_string(),
                None => continue,
            };
            let bytes = decode_payload(&value);
            let mime = mime_for_name(&name);
            tx.execute(
                "INSERT OR IGNORE INTO composition_assets
                     (composition_id, name, bytes, mime_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
                params![artifact_id, name, bytes, mime],
            )?;
            migrated_assets += 1;
        }

        // Strip assets/* keys from files JSON; re-derive content if entry
        // still resolves (it should — entries are always text files).
        let new_files = serde_json::to_string(&files)
            .map_err(|e| StorageError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
        let new_content = files.get(&entry).cloned();
        match new_content {
            Some(c) => {
                tx.execute(
                    "UPDATE artifacts SET files = ?1, content = ?2 WHERE id = ?3",
                    params![new_files, c, artifact_id],
                )?;
            }
            None => {
                // Entry doesn't resolve into the post-strip files map —
                // leave content alone (defensive against pre-015 data).
                tx.execute(
                    "UPDATE artifacts SET files = ?1 WHERE id = ?2",
                    params![new_files, artifact_id],
                )?;
            }
        }
        migrated_artifacts += 1;
    }

    tx.execute(
        "INSERT INTO _migrations (name, applied_at) VALUES (?1, strftime('%s','now'))",
        params![MARKER],
    )?;
    tx.commit()?;

    if migrated_artifacts > 0 {
        tracing::info!(
            "composition_assets migration: moved {} assets across {} artifacts \
             from artifacts.files JSON to dedicated table",
            migrated_assets,
            migrated_artifacts
        );
    }
    Ok(())
}

/// Decode a base64-shaped payload into bytes. Falls back to UTF-8 bytes
/// when the value isn't valid base64 (text assets like SVG / JSON were
/// occasionally stored as raw UTF-8 in the legacy files map).
///
/// Heuristic-free: attempt base64 decode unconditionally. STANDARD's
/// padding rules reject most natural-language strings, so an SVG body
/// or a CSS file falls through to the raw-bytes branch automatically.
/// A very small number of pathological strings could decode by accident
/// (a 4-char alphanumeric run with valid padding), but those are not
/// produced by `canvas_attach_asset` — the only writer to `assets/*`
/// in the legacy data — which always base64-encodes binary inputs.
fn decode_payload(payload: &str) -> Vec<u8> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD
        .decode(payload.as_bytes())
        .unwrap_or_else(|_| payload.as_bytes().to_vec())
}

/// Trivial extension→MIME map for the migration-side store. The HTTP
/// asset GET handler does its own magic-byte sniff at serve time, so
/// this is only the at-rest hint shown in `mime_type`.
fn mime_for_name(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let m = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "json" => "application/json",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "txt" => "text/plain",
        _ => return None,
    };
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    fn boot() -> Storage {
        Storage::open_in_memory().expect("storage")
    }

    fn create_parent(storage: &Storage, id: &str) {
        use crate::repositories::ArtifactRepository;
        use crate::CreateArtifactParams;

        let repo = ArtifactRepository::new(storage.database());
        repo.create(CreateArtifactParams {
            id: id.into(),
            session_id: None,
            title: "fixture".into(),
            description: None,
            content_type: "text/html".into(),
            content: "<html/>".into(),
            files: None,
            entry: Some("index.html".into()),
        })
        .unwrap();
    }

    #[test]
    fn upsert_then_get_round_trips_bytes_and_mime() {
        let s = boot();
        create_parent(&s, "art-1");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-1", "hero.png", &[1, 2, 3, 4], Some("image/png"))
            .unwrap();
        let got = repo.get("art-1", "hero.png").unwrap().unwrap();
        assert_eq!(got.bytes, vec![1, 2, 3, 4]);
        assert_eq!(got.mime_type.as_deref(), Some("image/png"));
        assert_eq!(got.composition_id, "art-1");
        assert_eq!(got.name, "hero.png");
    }

    #[test]
    fn upsert_overwrites_existing_row() {
        let s = boot();
        create_parent(&s, "art-2");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-2", "hero.png", &[1, 2, 3], None).unwrap();
        repo.upsert("art-2", "hero.png", &[9, 9, 9, 9], Some("image/png"))
            .unwrap();
        let got = repo.get("art-2", "hero.png").unwrap().unwrap();
        assert_eq!(got.bytes, vec![9, 9, 9, 9]);
        assert_eq!(got.mime_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn list_names_returns_sorted_keys_for_composition() {
        let s = boot();
        create_parent(&s, "art-3");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-3", "z.png", &[0], None).unwrap();
        repo.upsert("art-3", "a.png", &[0], None).unwrap();
        repo.upsert("art-3", "m.png", &[0], None).unwrap();
        let names = repo.list_names("art-3").unwrap();
        assert_eq!(names, vec!["a.png", "m.png", "z.png"]);
    }

    #[test]
    fn list_all_returns_full_records() {
        let s = boot();
        create_parent(&s, "art-list");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-list", "a.png", b"AAA", Some("image/png"))
            .unwrap();
        repo.upsert("art-list", "b.svg", b"<svg/>", Some("image/svg+xml"))
            .unwrap();
        let all = repo.list_all("art-list").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "a.png");
        assert_eq!(all[0].bytes, b"AAA");
        assert_eq!(all[1].name, "b.svg");
        assert_eq!(all[1].bytes, b"<svg/>");
    }

    #[test]
    fn delete_returns_true_only_when_row_existed() {
        let s = boot();
        create_parent(&s, "art-del");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-del", "hero.png", &[1], None).unwrap();
        assert!(repo.delete("art-del", "hero.png").unwrap());
        assert!(!repo.delete("art-del", "hero.png").unwrap());
        assert!(repo.get("art-del", "hero.png").unwrap().is_none());
    }

    #[test]
    fn delete_all_for_clears_only_target_composition() {
        let s = boot();
        create_parent(&s, "art-A");
        create_parent(&s, "art-B");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-A", "x.png", &[1], None).unwrap();
        repo.upsert("art-A", "y.png", &[2], None).unwrap();
        repo.upsert("art-B", "z.png", &[3], None).unwrap();
        let n = repo.delete_all_for("art-A").unwrap();
        assert_eq!(n, 2);
        assert_eq!(repo.list_names("art-A").unwrap().len(), 0);
        assert_eq!(repo.list_names("art-B").unwrap(), vec!["z.png"]);
    }

    #[test]
    fn cascade_delete_when_artifact_removed() {
        // FK CASCADE: dropping the parent artifact row drops its assets.
        use crate::repositories::ArtifactRepository;
        let s = boot();
        create_parent(&s, "art-cascade");
        let repo = CompositionAssetRepository::new(s.database());
        repo.upsert("art-cascade", "hero.png", &[1, 2, 3], None)
            .unwrap();
        // Delete via raw SQL so we exercise the FK cascade rather than
        // any per-asset cleanup the artifact repository might add.
        s.database()
            .with_connection(|conn| {
                conn.execute("DELETE FROM artifacts WHERE id = ?1", params!["art-cascade"])?;
                Ok(())
            })
            .unwrap();
        assert!(repo.get("art-cascade", "hero.png").unwrap().is_none());

        // Sanity: ArtifactRepository::get also reports the parent gone.
        assert!(ArtifactRepository::new(s.database())
            .get("art-cascade")
            .unwrap()
            .is_none());
    }

    #[test]
    fn migration_moves_legacy_assets_into_dedicated_table() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        // Simulate a legacy artifact whose `files` map carries assets/*
        // entries (base64 strings). The migration should move them into
        // composition_assets and strip them from the JSON.
        let s = boot();
        let mut files: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        files.insert("index.html".into(), "<body/>".to_string());
        files.insert("DESIGN.md".into(), "# design".to_string());
        let png_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        files.insert("assets/hero.png".into(), STANDARD.encode(&png_bytes));
        files.insert("assets/note.txt".into(), "hello text asset".to_string());

        // Direct INSERT, bypassing ArtifactRepository::create to keep the
        // legacy shape (assets in files JSON) intact.
        s.database()
            .with_connection(|conn| {
                let json = serde_json::to_string(&files).unwrap();
                conn.execute(
                    "INSERT INTO artifacts (id, title, content_type, content, files, entry, created_at)
                     VALUES (?1, 'fixture', 'text/html', '<body/>', ?2, 'index.html', strftime('%s','now'))",
                    params!["art-legacy", json],
                )?;
                // Wipe the marker so the migration runs.
                conn.execute(
                    "DELETE FROM _migrations WHERE name = '016b_composition_assets_data'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        // Run the migration through the public entry point so the
        // _migrations marker insert path is exercised end-to-end.
        s.database()
            .with_connection_mut(|conn| migrate_assets_to_composition_assets_table(conn))
            .unwrap();

        // Both assets landed in the dedicated table.
        let repo = CompositionAssetRepository::new(s.database());
        let names = repo.list_names("art-legacy").unwrap();
        assert_eq!(names, vec!["hero.png", "note.txt"]);
        let hero = repo.get("art-legacy", "hero.png").unwrap().unwrap();
        assert_eq!(hero.bytes, png_bytes);
        let note = repo.get("art-legacy", "note.txt").unwrap().unwrap();
        assert_eq!(note.bytes, b"hello text asset");

        // artifacts.files now has ONLY text files; assets/* are stripped.
        let stripped: String = s
            .database()
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT files FROM artifacts WHERE id = 'art-legacy'",
                    [],
                    |row| row.get::<_, String>(0),
                )?)
            })
            .unwrap();
        let parsed: std::collections::HashMap<String, String> =
            serde_json::from_str(&stripped).unwrap();
        assert!(parsed.contains_key("index.html"));
        assert!(parsed.contains_key("DESIGN.md"));
        assert!(!parsed.keys().any(|k| k.starts_with("assets/")));

        // Idempotent: running again does nothing (and doesn't double-
        // insert assets — INSERT OR IGNORE on PK).
        s.database()
            .with_connection_mut(|conn| migrate_assets_to_composition_assets_table(conn))
            .unwrap();
        assert_eq!(repo.list_names("art-legacy").unwrap().len(), 2);
    }
}
