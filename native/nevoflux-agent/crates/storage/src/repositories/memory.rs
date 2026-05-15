//! Memory repository for database operations.

use rusqlite::{params, OptionalExtension, Row};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::connection::Database;
use crate::error::{Result, StorageError};
use crate::models::MemoryChunk;

/// Sanitize a query string for FTS5 MATCH.
///
/// FTS5 treats certain characters as operators (`*`, `-`, `OR`, `AND`, `NOT`,
/// `NEAR`, `"`, `(`, `)`). Wrapping each token in double-quotes turns it into
/// an exact-match phrase token, preventing parse errors on user-supplied input.
/// Returns an empty string when the query contains no searchable terms.
fn sanitize_fts5_query(raw: &str) -> String {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(|t| {
            // Strip characters that break FTS5 even inside quotes
            let cleaned: String = t.chars().filter(|c| *c != '"').collect();
            cleaned
        })
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t))
        .collect();
    tokens.join(" ")
}

/// Get the current Unix timestamp.
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Repository for memory chunk CRUD operations.
pub struct MemoryRepository<'a> {
    db: &'a Database,
}

impl<'a> MemoryRepository<'a> {
    /// Create a new memory repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new memory chunk.
    pub fn create(&self, chunk: &MemoryChunk) -> Result<()> {
        let metadata_json = serde_json::to_string(&chunk.metadata)?;
        let embedding_blob = chunk.embedding.as_ref().map(|e| embedding_to_blob(e));

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO memory_chunks (id, content, embedding, metadata, created_at, updated_at, session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    chunk.id,
                    chunk.content,
                    embedding_blob,
                    metadata_json,
                    chunk.created_at,
                    chunk.updated_at,
                    chunk.session_id,
                ],
            )?;
            Ok(())
        })
    }

    /// Get a memory chunk by ID.
    pub fn get(&self, id: &str) -> Result<Option<MemoryChunk>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                     FROM memory_chunks WHERE id = ?1",
                    params![id],
                    row_to_memory_chunk,
                )
                .optional()?;

            match result {
                Some(chunk_result) => Ok(Some(chunk_result?)),
                None => Ok(None),
            }
        })
    }

    /// Search memory chunks using FTS5 full-text search.
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<MemoryChunk>> {
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(vec![]);
        }

        self.db.with_connection(|conn| {
            // Use FTS5 to find matching rowids, then join with memory_chunks
            let mut stmt = conn.prepare(
                "SELECT m.id, m.content, m.embedding, m.metadata, m.created_at, m.updated_at, m.session_id
                 FROM memory_chunks m
                 INNER JOIN memory_fts f ON m.rowid = f.rowid
                 WHERE memory_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )?;

            let chunks = stmt
                .query_map(params![sanitized, limit as i64], row_to_memory_chunk)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(chunks)
        })
    }

    /// Delete a memory chunk by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows_affected =
                conn.execute("DELETE FROM memory_chunks WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        })
    }

    /// Update the content of a memory chunk.
    pub fn update(&self, id: &str, content: &str) -> Result<bool> {
        let now = current_timestamp();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE memory_chunks SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![content, now, id],
            )?;
            Ok(rows_affected > 0)
        })
    }

    /// List all memory chunks with optional limit.
    pub fn list(&self, limit: Option<usize>) -> Result<Vec<MemoryChunk>> {
        self.db.with_connection(|conn| {
            let sql = match limit {
                Some(l) => format!(
                    "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                     FROM memory_chunks ORDER BY created_at DESC LIMIT {}",
                    l
                ),
                None => {
                    "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                     FROM memory_chunks ORDER BY created_at DESC"
                        .to_string()
                }
            };

            let mut stmt = conn.prepare(&sql)?;
            let chunks = stmt
                .query_map([], row_to_memory_chunk)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(chunks)
        })
    }

    /// List memory chunks by session ID.
    pub fn list_by_session(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MemoryChunk>> {
        self.db.with_connection(|conn| {
            let sql = match limit {
                Some(l) => format!(
                    "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                     FROM memory_chunks WHERE session_id = ?1 ORDER BY created_at DESC LIMIT {}",
                    l
                ),
                None => {
                    "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                     FROM memory_chunks WHERE session_id = ?1 ORDER BY created_at DESC"
                        .to_string()
                }
            };

            let mut stmt = conn.prepare(&sql)?;
            let chunks = stmt
                .query_map(params![session_id], row_to_memory_chunk)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(chunks)
        })
    }

    /// Count total memory chunks.
    pub fn count(&self) -> Result<u32> {
        self.db.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM memory_chunks", [], |row| row.get(0))?;
            Ok(count as u32)
        })
    }

    /// List memory chunks that have embeddings.
    pub fn list_with_embeddings(&self, limit: usize) -> Result<Vec<MemoryChunk>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                 FROM memory_chunks WHERE embedding IS NOT NULL ORDER BY created_at DESC LIMIT ?1",
            )?;

            let chunks = stmt
                .query_map(params![limit as i64], row_to_memory_chunk)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(chunks)
        })
    }

    /// List memory chunks that do not have embeddings.
    pub fn list_without_embeddings(&self, limit: usize) -> Result<Vec<MemoryChunk>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content, embedding, metadata, created_at, updated_at, session_id
                 FROM memory_chunks WHERE embedding IS NULL ORDER BY created_at DESC LIMIT ?1",
            )?;

            let chunks = stmt
                .query_map(params![limit as i64], row_to_memory_chunk)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(chunks)
        })
    }

    /// Update the embedding of a memory chunk.
    ///
    /// Returns `true` if the row was found and updated.
    pub fn update_embedding(&self, id: &str, embedding: &[f32]) -> Result<bool> {
        let now = current_timestamp();
        let embedding_blob = embedding_to_blob(embedding);

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE memory_chunks SET embedding = ?1, updated_at = ?2 WHERE id = ?3",
                params![embedding_blob, now, id],
            )?;
            Ok(rows_affected > 0)
        })
    }
}

/// Convert embedding vector to blob bytes (little-endian f32).
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert blob bytes back to embedding vector.
fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Convert a database row to a MemoryChunk.
fn row_to_memory_chunk(row: &Row<'_>) -> rusqlite::Result<Result<MemoryChunk>> {
    let id: String = row.get(0)?;
    let content: String = row.get(1)?;
    let embedding_blob: Option<Vec<u8>> = row.get(2)?;
    let metadata_json: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    let session_id: Option<String> = row.get(6)?;

    let embedding = embedding_blob.map(|blob| blob_to_embedding(&blob));

    let metadata: serde_json::Value = match serde_json::from_str(&metadata_json) {
        Ok(m) => m,
        Err(e) => return Ok(Err(StorageError::Json(e))),
    };

    Ok(Ok(MemoryChunk {
        id,
        content,
        embedding,
        metadata,
        created_at,
        updated_at,
        session_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CreateSessionParams;
    use crate::repositories::SessionRepository;

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    /// Helper to create a session for tests that need a valid session_id.
    fn create_session(db: &Database, id: &str) {
        let session_repo = SessionRepository::new(db);
        session_repo
            .create(CreateSessionParams::new().with_id(id))
            .unwrap();
    }

    #[test]
    fn test_create_memory_chunk() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("Hello, world!").with_id("test-1");

        repo.create(&chunk).unwrap();

        let fetched = repo.get("test-1").unwrap();
        assert!(fetched.is_some());

        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, "test-1");
        assert_eq!(fetched.content, "Hello, world!");
    }

    #[test]
    fn test_create_memory_chunk_with_embedding() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let chunk = MemoryChunk::new("Test content")
            .with_id("test-embed")
            .with_embedding(embedding.clone());

        repo.create(&chunk).unwrap();

        let fetched = repo.get("test-embed").unwrap().unwrap();
        assert_eq!(fetched.embedding, Some(embedding));
    }

    #[test]
    fn test_create_memory_chunk_with_metadata() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let metadata = serde_json::json!({"source": "test", "page": 42});
        let chunk = MemoryChunk::new("Test content")
            .with_id("test-meta")
            .with_metadata(metadata.clone());

        repo.create(&chunk).unwrap();

        let fetched = repo.get("test-meta").unwrap().unwrap();
        assert_eq!(fetched.metadata, metadata);
    }

    #[test]
    fn test_create_memory_chunk_with_session() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        // Create a valid session first
        create_session(&db, "sess-123");

        let chunk = MemoryChunk::new("Test content")
            .with_id("test-sess")
            .with_session("sess-123");

        repo.create(&chunk).unwrap();

        let fetched = repo.get("test-sess").unwrap().unwrap();
        assert_eq!(fetched.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_get_memory_chunk_not_found() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_memory_chunk() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("Delete me").with_id("test-delete");
        repo.create(&chunk).unwrap();

        let deleted = repo.delete("test-delete").unwrap();
        assert!(deleted);

        let fetched = repo.get("test-delete").unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn test_delete_memory_chunk_not_found() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let deleted = repo.delete("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_update_memory_chunk() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("Original content").with_id("test-update");
        repo.create(&chunk).unwrap();

        let updated = repo.update("test-update", "Updated content").unwrap();
        assert!(updated);

        let fetched = repo.get("test-update").unwrap().unwrap();
        assert_eq!(fetched.content, "Updated content");
        assert!(fetched.updated_at >= chunk.updated_at);
    }

    #[test]
    fn test_update_memory_chunk_not_found() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let updated = repo.update("nonexistent", "New content").unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_search_fts() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk1 =
            MemoryChunk::new("The quick brown fox jumps over the lazy dog").with_id("fts-1");
        let chunk2 = MemoryChunk::new("A fast brown cat runs across the yard").with_id("fts-2");
        let chunk3 =
            MemoryChunk::new("Something completely different about computers").with_id("fts-3");

        repo.create(&chunk1).unwrap();
        repo.create(&chunk2).unwrap();
        repo.create(&chunk3).unwrap();

        // Search for "brown" - should find chunk1 and chunk2
        let results = repo.search_fts("brown", 10).unwrap();
        assert_eq!(results.len(), 2);

        // Search for "fox" - should find only chunk1
        let results = repo.search_fts("fox", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "fts-1");

        // Search for "computers" - should find only chunk3
        let results = repo.search_fts("computers", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "fts-3");
    }

    #[test]
    fn test_search_fts_limit() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        // Create multiple chunks with the same keyword
        for i in 0..5 {
            let chunk = MemoryChunk::new(format!("Test content with keyword {}", i))
                .with_id(format!("fts-limit-{}", i));
            repo.create(&chunk).unwrap();
        }

        // Search with limit
        let results = repo.search_fts("keyword", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_fts_no_results() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("Hello world").with_id("fts-none");
        repo.create(&chunk).unwrap();

        let results = repo.search_fts("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_list_memory_chunks() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk1 = MemoryChunk::new("First").with_id("list-1");
        let chunk2 = MemoryChunk::new("Second").with_id("list-2");
        let chunk3 = MemoryChunk::new("Third").with_id("list-3");

        repo.create(&chunk1).unwrap();
        repo.create(&chunk2).unwrap();
        repo.create(&chunk3).unwrap();

        let all = repo.list(None).unwrap();
        assert_eq!(all.len(), 3);

        let limited = repo.list(Some(2)).unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_list_by_session() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        // Create valid sessions first
        create_session(&db, "session-a");
        create_session(&db, "session-b");

        let chunk1 = MemoryChunk::new("Session A chunk 1")
            .with_id("sess-a-1")
            .with_session("session-a");
        let chunk2 = MemoryChunk::new("Session A chunk 2")
            .with_id("sess-a-2")
            .with_session("session-a");
        let chunk3 = MemoryChunk::new("Session B chunk")
            .with_id("sess-b-1")
            .with_session("session-b");
        let chunk4 = MemoryChunk::new("No session chunk").with_id("no-sess");

        repo.create(&chunk1).unwrap();
        repo.create(&chunk2).unwrap();
        repo.create(&chunk3).unwrap();
        repo.create(&chunk4).unwrap();

        let session_a_chunks = repo.list_by_session("session-a", None).unwrap();
        assert_eq!(session_a_chunks.len(), 2);

        let session_b_chunks = repo.list_by_session("session-b", None).unwrap();
        assert_eq!(session_b_chunks.len(), 1);

        let empty_session = repo.list_by_session("nonexistent", None).unwrap();
        assert!(empty_session.is_empty());
    }

    #[test]
    fn test_count_memory_chunks() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        assert_eq!(repo.count().unwrap(), 0);

        repo.create(&MemoryChunk::new("One").with_id("count-1"))
            .unwrap();
        repo.create(&MemoryChunk::new("Two").with_id("count-2"))
            .unwrap();
        repo.create(&MemoryChunk::new("Three").with_id("count-3"))
            .unwrap();

        assert_eq!(repo.count().unwrap(), 3);
    }

    #[test]
    fn test_embedding_blob_conversion() {
        let original = vec![1.0f32, -2.5, 3.14159, 0.0, -0.00001];
        let blob = embedding_to_blob(&original);
        let converted = blob_to_embedding(&blob);

        assert_eq!(original.len(), converted.len());
        for (a, b) in original.iter().zip(converted.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_fts_update_sync() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        // Create chunk and verify FTS works
        let chunk = MemoryChunk::new("original searchable content").with_id("fts-sync");
        repo.create(&chunk).unwrap();

        let results = repo.search_fts("original", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Update content
        repo.update("fts-sync", "updated different text").unwrap();

        // Old term should not be found
        let results = repo.search_fts("original", 10).unwrap();
        assert_eq!(results.len(), 0);

        // New term should be found
        let results = repo.search_fts("updated", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_fts_delete_sync() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("deletable content").with_id("fts-delete");
        repo.create(&chunk).unwrap();

        // Verify it's searchable
        let results = repo.search_fts("deletable", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Delete and verify FTS is also cleaned
        repo.delete("fts-delete").unwrap();

        let results = repo.search_fts("deletable", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_list_with_embeddings() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk1 = MemoryChunk::new("no embedding");
        repo.create(&chunk1).unwrap();

        let chunk2 = MemoryChunk::new("has embedding").with_embedding(vec![0.1, 0.2, 0.3]);
        repo.create(&chunk2).unwrap();

        let results = repo.list_with_embeddings(100).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].embedding.is_some());
    }

    #[test]
    fn test_list_without_embeddings() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk1 = MemoryChunk::new("no embedding");
        repo.create(&chunk1).unwrap();

        let chunk2 = MemoryChunk::new("has embedding").with_embedding(vec![0.1, 0.2, 0.3]);
        repo.create(&chunk2).unwrap();

        let results = repo.list_without_embeddings(100).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].embedding.is_none());
    }

    #[test]
    fn test_update_embedding() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("test content");
        repo.create(&chunk).unwrap();

        let retrieved = repo.get(&chunk.id).unwrap().unwrap();
        assert!(retrieved.embedding.is_none());

        let emb = vec![0.5_f32, 0.6, 0.7];
        let updated = repo.update_embedding(&chunk.id, &emb).unwrap();
        assert!(updated);

        let result = repo.get(&chunk.id).unwrap().unwrap();
        assert_eq!(result.embedding.unwrap(), vec![0.5, 0.6, 0.7]);
    }

    #[test]
    fn test_update_embedding_not_found() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let result = repo.update_embedding("nonexistent", &[0.1, 0.2]).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_sanitize_fts5_query_empty() {
        assert_eq!(sanitize_fts5_query(""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
    }

    #[test]
    fn test_sanitize_fts5_query_normal() {
        assert_eq!(sanitize_fts5_query("hello world"), "\"hello\" \"world\"");
        assert_eq!(sanitize_fts5_query("rust"), "\"rust\"");
    }

    #[test]
    fn test_sanitize_fts5_query_special_chars() {
        // Quotes are stripped, other chars preserved inside quotes
        assert_eq!(sanitize_fts5_query("he\"llo"), "\"hello\"");
        assert_eq!(sanitize_fts5_query("NOT -bad"), "\"NOT\" \"-bad\"");
        assert_eq!(sanitize_fts5_query("foo*"), "\"foo*\"");
    }

    #[test]
    fn test_search_fts_empty_query() {
        let db = setup_db();
        let repo = MemoryRepository::new(&db);

        let chunk = MemoryChunk::new("some content").with_id("empty-q");
        repo.create(&chunk).unwrap();

        let results = repo.search_fts("", 10).unwrap();
        assert!(results.is_empty());

        let results = repo.search_fts("   ", 10).unwrap();
        assert!(results.is_empty());
    }
}
