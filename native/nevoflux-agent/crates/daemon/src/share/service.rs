//! CanvasShareService - orchestrates Canvas Share lifecycle.
//!
//! Coordinates share creation, import, extension, deletion, and listing by
//! composing the lower-level primitives in this module: ID/password/token
//! generation, encryption, binary serialization, HTTP transport, and local
//! encrypted storage of owner credentials in SQLite.

use std::sync::Arc;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use nevoflux_storage::{Storage, StorageError};
use rusqlite::OptionalExtension;

use super::binary_format::{deserialize, serialize};
use super::crypto::{decrypt_share_bundle, encrypt_share_bundle};
use super::http_client::ShareHttpClient;
use super::local_store::{
    decrypt_bytes_from_storage, encrypt_bytes_for_storage, encrypt_for_storage,
};
use super::owner_token::{generate_owner_token, hash_owner_token};
use super::password::generate_password;
use super::share_id::generate_share_id;
use super::types::{ShareBundle, ShareMetadata};
use crate::error::{DaemonError, Result};

/// Default TTL: 30 days.
pub const DEFAULT_TTL_SECS: u64 = 30 * 24 * 3600;

/// Stable id for the archive session that holds canvases imported via shared
/// links. Using a well-known id means every import lands in the same bucket
/// regardless of how many times the row is re-created or the daemon restarts.
const IMPORTED_CANVASES_SESSION_ID: &str = "imported-canvases";

/// Resolve the effective session_id for an imported artifact.
///
/// If the caller supplied a non-empty session_id that already exists in
/// `sessions`, use it as-is. Otherwise fall back to a dedicated "Imported
/// Canvases" archive session, creating that row lazily on first use. This
/// keeps the NOT NULL FK on artifacts.session_id satisfied in import flows
/// that originate from standalone share pages with no active chat session.
fn resolve_import_session_id(
    conn: &rusqlite::Connection,
    requested: &str,
    now: i64,
) -> rusqlite::Result<String> {
    if !requested.is_empty() {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                rusqlite::params![requested],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if exists {
            return Ok(requested.to_string());
        }
    }

    conn.execute(
        "INSERT OR IGNORE INTO sessions (id, title, mode, created_at, updated_at) \
         VALUES (?1, ?2, 'chat', ?3, ?3)",
        rusqlite::params![IMPORTED_CANVASES_SESSION_ID, "Imported Canvases", now,],
    )?;
    Ok(IMPORTED_CANVASES_SESSION_ID.to_string())
}

/// Parse an ISO 8601 / RFC 3339 timestamp into a Unix timestamp. On parse
/// failure returns `0` so callers can at least store a row.
fn parse_iso8601_to_unix(s: &str) -> i64 {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// Info about an active share (what `list()` returns).
#[derive(Debug, Clone)]
pub struct ShareInfo {
    pub artifact_id: String,
    pub share_id: String,
    pub share_url: String,
    pub expires_at: i64,
    pub view_count: u64,
    pub created_at: i64,
}

/// Result of a `share()` operation. The password is returned only once —
/// the raw plaintext is never persisted; only a local encrypted copy is kept.
#[derive(Debug, Clone)]
pub struct ShareResult {
    pub share_id: String,
    pub share_url: String,
    /// Shown once to the user — never stored in plaintext.
    pub password: String,
    pub expires_at: i64,
}

/// Result of an `import()` operation.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub artifact_id: String,
    pub artifact_name: String,
    pub artifact_type: String,
    pub share_id: String,
}

/// Row helper for reading an artifact from SQLite.
#[derive(Debug)]
struct ArtifactRow {
    #[allow(dead_code)]
    id: String,
    title: String,
    content_type: String,
    content: String,
}

/// Orchestrates the Canvas Share lifecycle.
pub struct CanvasShareService {
    storage: Arc<Storage>,
    http: ShareHttpClient,
    /// 32-byte master key used to encrypt passwords and owner tokens at rest.
    master_key: [u8; 32],
}

impl CanvasShareService {
    /// Create a new `CanvasShareService`.
    pub fn new(storage: Arc<Storage>, http: ShareHttpClient, master_key: [u8; 32]) -> Self {
        Self {
            storage,
            http,
            master_key,
        }
    }

    /// Share an artifact: encrypt, upload to CF Worker, record credentials locally.
    ///
    /// Returns a [`ShareResult`] with the generated share ID, URL, password
    /// (shown to the user exactly once), and expiry timestamp.
    pub async fn share(
        &self,
        session_id: &str,
        artifact_id: &str,
        ttl_secs: Option<u64>,
    ) -> Result<ShareResult> {
        let ttl = ttl_secs.unwrap_or(DEFAULT_TTL_SECS);

        // 1. Load the artifact content from SQLite.
        let artifact = self.load_artifact(session_id, artifact_id)?;

        // 2. Inline `assets/X` references into a self-contained HTML so
        //    the recipient (no daemon) sees a working file. After
        //    migration 016, assets live in `composition_assets` rather
        //    than `artifacts.files` — read them from the dedicated
        //    table, base64-encode for the inliner's expected shape,
        //    and call inline_assets.
        let shared_content = {
            use base64::{engine::general_purpose::STANDARD, Engine};
            use nevoflux_storage::repositories::CompositionAssetRepository;

            let asset_repo = CompositionAssetRepository::new(self.storage.database());
            let assets = asset_repo
                .list_all(artifact_id)
                .map_err(|e| DaemonError::InternalError(format!("share load_all: {e}")))?;

            if assets.is_empty() {
                artifact.content.clone()
            } else {
                let mut files: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();
                for a in &assets {
                    let b64 = STANDARD.encode(&a.bytes);
                    files.insert(format!("assets/{}", a.name), b64);
                }
                crate::canvas_video::asset_inline::inline_assets(&artifact.content, &files)
            }
        };

        // 3. Build the plaintext ShareBundle.
        let bundle = ShareBundle {
            artifact_id: artifact_id.to_string(),
            artifact_name: artifact.title.clone(),
            artifact_type: artifact.content_type.clone(),
            content: serde_json::Value::String(shared_content),
            metadata: ShareMetadata {
                created_at: Utc::now().to_rfc3339(),
                version: "1.0".into(),
                author: None,
            },
        };

        // 3. Generate credentials.
        let share_id = generate_share_id();
        let password = generate_password();
        let owner_token = generate_owner_token();
        let owner_token_hash = hash_owner_token(&share_id, &owner_token);

        // 4. Encrypt + serialize to NFEB binary format.
        let encrypted = encrypt_share_bundle(&bundle, &password, &share_id)?;
        let nfeb_bytes = serialize(&encrypted)?;

        // 5. Upload to the CF Worker.
        let upload_resp = self
            .http
            .upload(&share_id, &nfeb_bytes, &owner_token_hash, ttl)
            .await?;

        // The CF Worker returns `expires_at` as an ISO 8601 string; convert to
        // a Unix timestamp for internal/on-disk use.
        let expires_at_ts = parse_iso8601_to_unix(&upload_resp.expires_at);

        // 6. Store credentials locally, encrypted at rest.
        let enc_password = encrypt_for_storage(&password, &self.master_key)?;
        let enc_token = encrypt_bytes_for_storage(&owner_token, &self.master_key)?;
        let now = Utc::now().timestamp();

        let share_id_db = upload_resp.share_id.clone();
        let share_url_db = upload_resp.url.clone();
        let expires_at_db = expires_at_ts;
        let artifact_id_db = artifact_id.to_string();

        self.storage.database().with_connection(|conn| {
            conn.execute(
                "INSERT INTO artifact_shares (artifact_id, share_id, share_url, encrypted_password, encrypted_owner_token, expires_at, view_count, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
                rusqlite::params![
                    artifact_id_db,
                    share_id_db,
                    share_url_db,
                    enc_password,
                    enc_token,
                    expires_at_db,
                    now,
                ],
            )?;
            Ok(())
        })?;

        Ok(ShareResult {
            share_id: upload_resp.share_id,
            share_url: upload_resp.url,
            password,
            expires_at: expires_at_ts,
        })
    }

    /// Import a shared canvas: fetch bundle, decrypt, write as a new local artifact.
    pub async fn import(
        &self,
        session_id: &str,
        share_id: &str,
        password: &str,
    ) -> Result<ImportResult> {
        // 1. Download the encrypted bundle.
        let nfeb_bytes = self.http.fetch_bundle(share_id).await?;

        // 2. Deserialize NFEB and decrypt with the provided password.
        let encrypted = deserialize(&nfeb_bytes)?;
        let bundle = decrypt_share_bundle(&encrypted, password)?;

        // 3. Generate a new local artifact ID.
        let new_artifact_id = uuid::Uuid::new_v4().to_string();
        let share_url = format!("{}/{}", self.http.base_url(), share_id);
        let now = Utc::now().timestamp();
        let content_str = match &bundle.content {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // 4. Insert into artifacts with imported_from_* provenance columns.
        //
        // artifacts.session_id is a NOT NULL FK to sessions(id). When the
        // client invokes import from a standalone nevoflux://import/... page
        // (no active chat session), session_id arrives empty and the INSERT
        // would violate the FK. Resolve it to a stable "Imported Canvases"
        // archive session, creating it on first use.
        let session_id_db_source = session_id.to_string();
        let artifact_id_db = new_artifact_id.clone();
        let title_db = bundle.artifact_name.clone();
        let content_type_db = bundle.artifact_type.clone();
        let share_id_db = share_id.to_string();

        // Migration 015 invariant: every row must have a non-empty `files`
        // map and an `entry` pointing into it. The share bundle ships the
        // renderable payload as a single string (assets already inlined on
        // the export side), so synthesize a one-key map. Without this the
        // columns default to '{}' / 'main.html', and Canvas's
        // `_renderProject` short-circuits to "No files in project".
        let entry_name = "main.html".to_string();
        let mut files_map = std::collections::HashMap::new();
        files_map.insert(entry_name.clone(), content_str.clone());
        let files_json = serde_json::to_string(&files_map)
            .unwrap_or_else(|_| "{}".to_string());

        self.storage.database().with_connection(|conn| {
            let session_id_db =
                resolve_import_session_id(conn, &session_id_db_source, now)?;
            conn.execute(
                "INSERT INTO artifacts (id, session_id, title, content_type, content, files, entry, imported_from_url, imported_from_share_id, imported_at, is_persistent, persisted_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    artifact_id_db,
                    session_id_db,
                    title_db,
                    content_type_db,
                    content_str,
                    files_json,
                    entry_name,
                    share_url,
                    share_id_db,
                    now,
                    true,   // is_persistent
                    now,    // persisted_at
                    now,    // updated_at
                ],
            )?;
            Ok(())
        })?;

        Ok(ImportResult {
            artifact_id: new_artifact_id,
            artifact_name: bundle.artifact_name,
            artifact_type: bundle.artifact_type,
            share_id: share_id.to_string(),
        })
    }

    /// Extend a share's TTL by `extend_secs` seconds. Returns the new expiry
    /// as a Unix timestamp.
    pub async fn extend(&self, share_id: &str, extend_secs: u64) -> Result<i64> {
        // 1. Load the encrypted owner token.
        let enc_token = self.load_encrypted_owner_token(share_id)?;

        // 2. Decrypt to get the raw owner token, then base64url-encode it for
        //    the CF Worker (which expects `owner_token` as base64url-no-pad).
        let owner_token = decrypt_bytes_from_storage(&enc_token, &self.master_key)?;
        let owner_token_b64 = URL_SAFE_NO_PAD.encode(&owner_token);

        // 3. Ask the server to extend.
        let resp = self
            .http
            .extend(share_id, &owner_token_b64, extend_secs)
            .await?;

        // 4. Update the local expires_at (Unix timestamp).
        let new_expires_at = parse_iso8601_to_unix(&resp.expires_at);
        let share_id_db = share_id.to_string();
        self.storage.database().with_connection(|conn| {
            conn.execute(
                "UPDATE artifact_shares SET expires_at = ?1 WHERE share_id = ?2",
                rusqlite::params![new_expires_at, share_id_db],
            )?;
            Ok(())
        })?;

        Ok(new_expires_at)
    }

    /// Delete a share. Removes server-side and moves the local record to
    /// `artifact_share_history` with reason `deleted`.
    pub async fn delete(&self, share_id: &str) -> Result<()> {
        // 1. Load and decrypt the owner token.
        let enc_token = self.load_encrypted_owner_token(share_id)?;
        let owner_token = decrypt_bytes_from_storage(&enc_token, &self.master_key)?;
        let owner_token_b64 = URL_SAFE_NO_PAD.encode(&owner_token);

        // 2. Delete on the server (expects base64url-encoded owner token).
        self.http.delete(share_id, &owner_token_b64).await?;

        // 3. Move to history + delete the active-share row.
        let share_id_db = share_id.to_string();
        self.storage.database().with_connection(|conn| {
            let artifact_id: Option<String> = conn
                .query_row(
                    "SELECT artifact_id FROM artifact_shares WHERE share_id = ?1",
                    rusqlite::params![share_id_db],
                    |row| row.get(0),
                )
                .ok();

            if let Some(aid) = artifact_id {
                conn.execute(
                    "INSERT INTO artifact_share_history (artifact_id, share_id, reason) \
                     VALUES (?1, ?2, 'deleted')",
                    rusqlite::params![aid, share_id_db],
                )?;
            }

            conn.execute(
                "DELETE FROM artifact_shares WHERE share_id = ?1",
                rusqlite::params![share_id_db],
            )?;
            Ok(())
        })?;

        Ok(())
    }

    /// List all active shares, newest first.
    ///
    /// `_session_id` is currently unused (the `artifact_shares` table is not
    /// scoped by session), but kept for API stability.
    pub fn list(&self, _session_id: &str) -> Result<Vec<ShareInfo>> {
        let rows = self.storage.database().with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT artifact_id, share_id, share_url, expires_at, view_count, created_at \
                 FROM artifact_shares ORDER BY created_at DESC",
            )?;
            let iter = stmt.query_map([], |row| {
                Ok(ShareInfo {
                    artifact_id: row.get(0)?,
                    share_id: row.get(1)?,
                    share_url: row.get(2)?,
                    expires_at: row.get(3)?,
                    view_count: row.get::<_, i64>(4)? as u64,
                    created_at: row.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        })?;
        Ok(rows)
    }

    // ---- internal helpers ----

    fn load_artifact(&self, _session_id: &str, artifact_id: &str) -> Result<ArtifactRow> {
        let artifact_id_db = artifact_id.to_string();
        let row = self
            .storage
            .database()
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT id, title, content_type, content FROM artifacts WHERE id = ?1",
                    rusqlite::params![artifact_id_db],
                    |row| {
                        Ok(ArtifactRow {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            content_type: row.get(2)?,
                            content: row.get(3)?,
                        })
                    },
                )
                .map_err(StorageError::from)
            })
            .map_err(|e| match e {
                StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => {
                    DaemonError::InvalidRequest(format!("Artifact not found: {}", artifact_id))
                }
                other => DaemonError::from(other),
            })?;
        Ok(row)
    }

    fn load_encrypted_owner_token(&self, share_id: &str) -> Result<String> {
        let share_id_db = share_id.to_string();
        self.storage
            .database()
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT encrypted_owner_token FROM artifact_shares WHERE share_id = ?1",
                    rusqlite::params![share_id_db],
                    |row| row.get::<_, String>(0),
                )
                .map_err(StorageError::from)
            })
            .map_err(|e| match e {
                StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => {
                    DaemonError::InvalidRequest(format!("Share not found: {}", share_id))
                }
                other => DaemonError::from(other),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    fn make_service() -> CanvasShareService {
        let storage = Arc::new(Storage::open_in_memory().expect("open storage"));
        let http = ShareHttpClient::new("https://example.test").expect("http client");
        CanvasShareService::new(storage, http, test_key())
    }

    #[test]
    fn list_empty_returns_empty_vec() {
        let svc = make_service();
        let shares = svc.list("any-session").expect("list");
        assert!(shares.is_empty());
    }

    #[test]
    fn load_artifact_missing_returns_error() {
        let svc = make_service();
        let err = svc
            .load_artifact("sess-1", "nonexistent-artifact")
            .unwrap_err();
        match err {
            DaemonError::InvalidRequest(msg) => assert!(msg.contains("Artifact not found")),
            other => panic!("expected InvalidRequest, got {:?}", other),
        }
    }

    #[test]
    fn load_encrypted_owner_token_missing_returns_error() {
        let svc = make_service();
        let err = svc
            .load_encrypted_owner_token("nonexistent-share")
            .unwrap_err();
        match err {
            DaemonError::InvalidRequest(msg) => assert!(msg.contains("Share not found")),
            other => panic!("expected InvalidRequest, got {:?}", other),
        }
    }

    // Network-dependent smoke test; ignored by default.
    #[tokio::test]
    #[ignore]
    async fn share_roundtrip_requires_network() {
        // Placeholder: a full share -> import roundtrip needs a running CF
        // Worker or a stub server. Kept here to document intent.
    }

    /// Phase 2 invariant: when a multi-file artifact is shared, the
    /// `ShareBundle.content` must be self-contained — `assets/X` refs in
    /// the entry HTML are inlined as `data:` URIs. The recipient has no
    /// daemon, so the downloaded HTML must open offline.
    ///
    /// `share()` itself uploads to a Cloudflare Worker, which we don't
    /// stand up in unit tests. Instead we exercise the inline branch
    /// directly by reading the row, calling the same helper `share()`
    /// uses, and asserting on the resulting `bundle.content`.
    #[test]
    fn canvas_share_export_inlines_assets() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use nevoflux_storage::repositories::{ArtifactRepository, CompositionAssetRepository};
        use nevoflux_storage::CreateArtifactParams;

        const PNG_1X1_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

        let svc = make_service();
        // Post-migration-016 shape: text files in artifacts.files,
        // binary assets in composition_assets table.
        let mut files = std::collections::HashMap::new();
        files.insert(
            "index.html".into(),
            r#"<html><body><img src="assets/hero.png"></body></html>"#.to_string(),
        );
        let entry_html = files["index.html"].clone();

        let repo = ArtifactRepository::new(svc.storage.database());
        repo.create(CreateArtifactParams {
            id: "share-fixture".into(),
            session_id: None,
            title: "fixture".into(),
            description: None,
            content_type: "text/html".into(),
            content: entry_html,
            files: Some(files),
            entry: Some("index.html".into()),
        })
        .unwrap();

        // Asset goes in the dedicated table, not in files.
        let asset_bytes = STANDARD.decode(PNG_1X1_B64.as_bytes()).unwrap();
        CompositionAssetRepository::new(svc.storage.database())
            .upsert("share-fixture", "hero.png", &asset_bytes, Some("image/png"))
            .unwrap();

        let row = svc
            .load_artifact("any-session", "share-fixture")
            .expect("load_artifact must succeed for the fixture");
        assert!(
            row.content.contains(r#"src="assets/hero.png""#),
            "stored content must keep relative refs (C1)"
        );

        // Apply the same transform `share()` does before encryption:
        // read assets from the composition_assets table, base64-encode,
        // call inline_assets.
        let asset_repo = CompositionAssetRepository::new(svc.storage.database());
        let assets = asset_repo.list_all("share-fixture").unwrap();
        assert!(!assets.is_empty(), "fixture must have a composition asset");

        let mut files: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for a in &assets {
            files.insert(format!("assets/{}", a.name), STANDARD.encode(&a.bytes));
        }
        let inlined =
            crate::canvas_video::asset_inline::inline_assets(&row.content, &files);
        assert!(
            inlined.contains("data:image/png;base64,"),
            "shared content must inline assets as data: URIs.\n got: {inlined}"
        );
        assert!(
            !inlined.contains(r#"src="assets/hero.png""#),
            "shared content must NOT contain raw `assets/X` refs"
        );
    }
}
