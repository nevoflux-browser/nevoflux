//! Knowledge repository for database operations.

use rusqlite::{params, OptionalExtension, Row};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::connection::Database;
use crate::error::Result;
use crate::models::knowledge::{CreateKnowledgeParams, Knowledge};

/// Repository for knowledge CRUD operations.
pub struct KnowledgeRepository<'a> {
    db: &'a Database,
}

impl<'a> KnowledgeRepository<'a> {
    /// Create a new knowledge repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new knowledge entry.
    pub fn create(&self, params: CreateKnowledgeParams) -> Result<Knowledge> {
        let id = generate_knowledge_id();
        let now = rfc3339_now();
        let priority = params.priority.unwrap_or_else(|| "medium".to_string());
        let privacy_level = params
            .privacy_level
            .unwrap_or_else(|| "internal".to_string());
        let source_type = params.source_type.unwrap_or_else(|| "system".to_string());
        let embedding_blob = params.embedding.as_ref().map(|e| embedding_to_blob(e));

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO knowledge (
                    id, category, subcategory, domain, summary, details,
                    resolution, confidence, hit_count, success_count, fail_count,
                    priority, status, source_ids, related_ids, tags,
                    privacy_level, promotion_target, promoted_section,
                    source_type, created_at, updated_at, last_hit_at, promoted_at,
                    embedding
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, 0.5, 1, 0, 0,
                    ?8, 'pending', ?9, NULL, ?10,
                    ?11, ?12, NULL,
                    ?13, ?14, ?15, NULL, NULL,
                    ?16
                )",
                params![
                    id,
                    params.category,
                    params.subcategory,
                    params.domain,
                    params.summary,
                    params.details,
                    params.resolution,
                    priority,
                    params.source_ids,
                    params.tags,
                    privacy_level,
                    params.promotion_target,
                    source_type,
                    now,
                    now,
                    embedding_blob,
                ],
            )?;

            // Read back to get the computed effectiveness column
            Ok(conn.query_row(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE id = ?1",
                params![id],
                row_to_knowledge,
            )?)
        })
    }

    /// Get a knowledge entry by ID.
    pub fn get(&self, id: &str) -> Result<Option<Knowledge>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, category, subcategory, domain, summary, details,
                            resolution, confidence, hit_count, success_count, fail_count,
                            effectiveness, priority, status, source_ids, related_ids, tags,
                            privacy_level, promotion_target, promoted_section,
                            source_type, created_at, updated_at, last_hit_at, promoted_at,
                            embedding, hot, hot_summary
                     FROM knowledge WHERE id = ?1",
                    params![id],
                    row_to_knowledge,
                )
                .optional()?;

            match result {
                Some(k) => Ok(Some(k)),
                None => Ok(None),
            }
        })
    }

    /// Update the content (details, summary, hot_summary) of a knowledge entry.
    pub fn update_content(&self, id: &str, details: &str, summary: &str) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET details = ?1, summary = ?2, hot_summary = ?3, updated_at = ?4 WHERE id = ?5",
                params![details, summary, summary, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Update the status of a knowledge entry.
    pub fn update_status(&self, id: &str, status: &str) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Query knowledge entries by domain, limited to a maximum number of results.
    pub fn query_by_domain(&self, domain: &str, limit: u32) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE domain = ?1
                 ORDER BY confidence DESC, updated_at DESC
                 LIMIT ?2",
            )?;

            let rows = stmt
                .query_map(params![domain, limit], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Query all pending knowledge entries, ordered by creation time (oldest first).
    ///
    /// Returns up to `limit` entries with status = 'pending'.
    pub fn query_pending(&self, limit: usize) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE status = 'pending'
                 ORDER BY created_at ASC
                 LIMIT ?1",
            )?;

            let rows = stmt
                .query_map(params![limit as i64], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Query all validated knowledge entries, ordered by creation time (oldest first).
    ///
    /// Returns up to `limit` entries with status = 'validated'.
    pub fn query_validated(&self, limit: usize) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE status = 'validated'
                 ORDER BY created_at ASC
                 LIMIT ?1",
            )?;

            let rows = stmt
                .query_map(params![limit as i64], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Mark a knowledge entry as promoted.
    ///
    /// Sets the status to 'promoted', records the current time as `promoted_at`,
    /// and stores the target section name in `promoted_section`.
    pub fn mark_promoted(&self, id: &str, promoted_section: &str) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET status = 'promoted', promoted_at = ?1, promoted_section = ?2, updated_at = ?3 WHERE id = ?4",
                params![now, promoted_section, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Resurrect an archived knowledge entry.
    ///
    /// Sets status back to 'validated', updates `last_hit_at` and `updated_at`
    /// to the current time, and increments `hit_count` by 1.
    /// Returns an error if the entry does not exist.
    pub fn resurrect_entry(&self, id: &str) -> Result<()> {
        let now = rfc3339_now();
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET status = 'validated', last_hit_at = ?1, updated_at = ?2, hit_count = hit_count + 1 WHERE id = ?3",
                params![now, now, id],
            )?;
            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }
            Ok(())
        })
    }

    /// Delete a knowledge entry by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute("DELETE FROM knowledge WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        })
    }

    /// Query all knowledge entries regardless of status, ordered by creation time (newest first).
    ///
    /// Returns up to `limit` entries.
    pub fn query_all(&self, limit: usize) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )?;

            let rows = stmt
                .query_map(params![limit as i64], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Find a duplicate entry by category + domain + summary (exact match).
    /// Find a hot knowledge entry for a specific tool name within a category.
    ///
    /// Used to deduplicate tool_optimization entries: when a newer stat for
    /// the same tool arrives, we update the existing hot entry instead of
    /// creating a duplicate.
    pub fn find_hot_by_tool_name(
        &self,
        category: &str,
        tool_name: &str,
    ) -> Result<Option<Knowledge>> {
        let pattern = format!("Tool '{}'%", tool_name);
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, category, subcategory, domain, summary, details,
                            resolution, confidence, hit_count, success_count, fail_count,
                            effectiveness, priority, status, source_ids, related_ids, tags,
                            privacy_level, promotion_target, promoted_section,
                            source_type, created_at, updated_at, last_hit_at, promoted_at,
                            embedding, hot, hot_summary
                     FROM knowledge
                     WHERE category = ?1 AND hot = 1 AND summary LIKE ?2
                     ORDER BY updated_at DESC
                     LIMIT 1",
                    params![category, pattern],
                    row_to_knowledge,
                )
                .optional()?;
            Ok(result)
        })
    }

    pub fn find_duplicate(
        &self,
        category: &str,
        domain: Option<&str>,
        summary: &str,
    ) -> Result<Option<Knowledge>> {
        self.db.with_connection(|conn| {
            let result = match domain {
                Some(d) => conn
                    .query_row(
                        "SELECT id, category, subcategory, domain, summary, details,
                                resolution, confidence, hit_count, success_count, fail_count,
                                effectiveness, priority, status, source_ids, related_ids, tags,
                                privacy_level, promotion_target, promoted_section,
                                source_type, created_at, updated_at, last_hit_at, promoted_at,
                                embedding, hot, hot_summary
                         FROM knowledge
                         WHERE category = ?1 AND domain = ?2 AND summary = ?3
                         LIMIT 1",
                        params![category, d, summary],
                        row_to_knowledge,
                    )
                    .optional()?,
                None => conn
                    .query_row(
                        "SELECT id, category, subcategory, domain, summary, details,
                                resolution, confidence, hit_count, success_count, fail_count,
                                effectiveness, priority, status, source_ids, related_ids, tags,
                                privacy_level, promotion_target, promoted_section,
                                source_type, created_at, updated_at, last_hit_at, promoted_at,
                                embedding, hot, hot_summary
                         FROM knowledge
                         WHERE category = ?1 AND domain IS NULL AND summary = ?2
                         LIMIT 1",
                        params![category, summary],
                        row_to_knowledge,
                    )
                    .optional()?,
            };

            match result {
                Some(k) => Ok(Some(k)),
                None => Ok(None),
            }
        })
    }

    /// Merge into existing entry: add hits, take max confidence, update timestamps.
    pub fn merge_entry(&self, id: &str, add_hits: u32, new_confidence: f64) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge
                 SET hit_count = hit_count + ?1,
                     confidence = MAX(confidence, ?2),
                     last_hit_at = ?3,
                     updated_at = ?4
                 WHERE id = ?5",
                params![add_hits, new_confidence, now, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Query entries with same category and domain (for conflict detection).
    ///
    /// When `domain` is `Some`, returns entries matching both the given domain
    /// and universal entries (domain IS NULL). When `domain` is `None`, returns
    /// only universal entries.
    pub fn query_by_subject(
        &self,
        category: &str,
        domain: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            if let Some(d) = domain {
                let mut stmt = conn.prepare(
                    "SELECT id, category, subcategory, domain, summary, details,
                            resolution, confidence, hit_count, success_count, fail_count,
                            effectiveness, priority, status, source_ids, related_ids, tags,
                            privacy_level, promotion_target, promoted_section,
                            source_type, created_at, updated_at, last_hit_at, promoted_at,
                            embedding, hot, hot_summary
                     FROM knowledge
                     WHERE category = ?1 AND (domain = ?2 OR domain IS NULL)
                     ORDER BY confidence DESC
                     LIMIT ?3",
                )?;

                let rows = stmt
                    .query_map(params![category, d, limit as i64], row_to_knowledge)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, category, subcategory, domain, summary, details,
                            resolution, confidence, hit_count, success_count, fail_count,
                            effectiveness, priority, status, source_ids, related_ids, tags,
                            privacy_level, promotion_target, promoted_section,
                            source_type, created_at, updated_at, last_hit_at, promoted_at,
                            embedding, hot, hot_summary
                     FROM knowledge
                     WHERE category = ?1 AND domain IS NULL
                     ORDER BY confidence DESC
                     LIMIT ?2",
                )?;

                let rows = stmt
                    .query_map(params![category, limit as i64], row_to_knowledge)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        })
    }

    /// Query entries with no embedding (for backfill).
    pub fn list_without_embeddings(&self, limit: usize) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE embedding IS NULL
                 ORDER BY created_at ASC
                 LIMIT ?1",
            )?;

            let rows = stmt
                .query_map(params![limit as i64], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Update only the embedding field.
    pub fn update_embedding(&self, id: &str, embedding: &[f32]) -> Result<()> {
        let now = rfc3339_now();
        let embedding_blob = embedding_to_blob(embedding);

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET embedding = ?1, updated_at = ?2 WHERE id = ?3",
                params![embedding_blob, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Mark a knowledge entry as hot (included in system prompt Layer 1).
    ///
    /// Sets `hot = 1`, stores the one-line `hot_summary`, and updates the
    /// status to "promoted".
    pub fn mark_hot(&self, id: &str, hot_summary: &str) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET hot = 1, hot_summary = ?1, status = 'promoted', promoted_at = ?2, updated_at = ?3 WHERE id = ?4",
                params![hot_summary, now, now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// Remove the hot flag from a knowledge entry.
    ///
    /// Sets `hot = 0` and clears `hot_summary`.
    pub fn unmark_hot(&self, id: &str) -> Result<()> {
        let now = rfc3339_now();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "UPDATE knowledge SET hot = 0, hot_summary = NULL, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;

            if rows_affected == 0 {
                return Err(crate::error::StorageError::NotFound {
                    entity: "knowledge".to_string(),
                    id: id.to_string(),
                });
            }

            Ok(())
        })
    }

    /// List all hot knowledge entries, ordered by confidence descending.
    pub fn list_hot(&self) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE hot = 1
                 ORDER BY confidence DESC",
            )?;

            let rows = stmt
                .query_map([], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// List hot knowledge entries filtered by category.
    pub fn list_hot_by_category(&self, category: &str) -> Result<Vec<Knowledge>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, category, subcategory, domain, summary, details,
                        resolution, confidence, hit_count, success_count, fail_count,
                        effectiveness, priority, status, source_ids, related_ids, tags,
                        privacy_level, promotion_target, promoted_section,
                        source_type, created_at, updated_at, last_hit_at, promoted_at,
                        embedding, hot, hot_summary
                 FROM knowledge WHERE hot = 1 AND category = ?1
                 ORDER BY confidence DESC",
            )?;

            let rows = stmt
                .query_map(params![category], row_to_knowledge)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(rows)
        })
    }

    /// Count hot knowledge entries for a given category.
    pub fn count_hot_by_category(&self, category: &str) -> Result<usize> {
        self.db.with_connection(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM knowledge WHERE hot = 1 AND category = ?1",
                params![category],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
    }

    /// Delete all knowledge entries.
    ///
    /// Returns the number of deleted rows.
    pub fn delete_all(&self) -> Result<usize> {
        self.db.with_connection(|conn| {
            let count = conn.execute("DELETE FROM knowledge", [])?;
            Ok(count)
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

/// Convert a database row to a Knowledge struct.
fn row_to_knowledge(row: &Row<'_>) -> rusqlite::Result<Knowledge> {
    let embedding_blob: Option<Vec<u8>> = row.get(25)?;
    let embedding = embedding_blob.map(|blob| blob_to_embedding(&blob));
    let hot_int: i64 = row.get(26)?;

    Ok(Knowledge {
        id: row.get(0)?,
        category: row.get(1)?,
        subcategory: row.get(2)?,
        domain: row.get(3)?,
        summary: row.get(4)?,
        details: row.get(5)?,
        resolution: row.get(6)?,
        confidence: row.get(7)?,
        hit_count: row.get(8)?,
        success_count: row.get(9)?,
        fail_count: row.get(10)?,
        effectiveness: row.get(11)?,
        priority: row.get(12)?,
        status: row.get(13)?,
        source_ids: row.get(14)?,
        related_ids: row.get(15)?,
        tags: row.get(16)?,
        privacy_level: row.get(17)?,
        promotion_target: row.get(18)?,
        promoted_section: row.get(19)?,
        source_type: row.get(20)?,
        created_at: row.get(21)?,
        updated_at: row.get(22)?,
        last_hit_at: row.get(23)?,
        promoted_at: row.get(24)?,
        embedding,
        hot: hot_int != 0,
        hot_summary: row.get(27)?,
    })
}

/// Generate a knowledge ID in the format K-{YYYYMMDD}-{6hex}.
fn generate_knowledge_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Convert Unix timestamp to date components
    // Days since epoch
    let days = secs / 86400;
    // Algorithm to convert days since epoch to Y-M-D
    let (year, month, day) = days_to_date(days);

    // Generate 6 hex chars from nanoseconds + wrapping multiply for uniqueness
    let nanos = now.as_nanos();
    let random_part = (nanos as u64).wrapping_mul(6364136223846793005);

    format!(
        "K-{:04}{:02}{:02}-{:06x}",
        year,
        month,
        day,
        random_part & 0xFFFFFF
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm from Howard Hinnant
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Get the current time as an RFC3339 string.
fn rfc3339_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let (year, month, day) = days_to_date(days);
    let rem = secs % 86400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    #[test]
    fn test_generate_knowledge_id_format() {
        let id = generate_knowledge_id();
        assert!(id.starts_with("K-"));
        // K-YYYYMMDD-6hex = 2 + 8 + 1 + 6 = 17 chars
        assert_eq!(id.len(), 17, "ID should be 17 chars: {}", id);
    }

    #[test]
    fn test_rfc3339_now_format() {
        let ts = rfc3339_now();
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
        assert_eq!(ts.len(), 20);
    }

    #[test]
    fn knowledge_crud_lifecycle() {
        let storage = Storage::open_in_memory().unwrap();

        // Create
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                subcategory: Some("selector_result".into()),
                domain: Some("github.com".into()),
                summary: "github.com uses data-testid selectors".into(),
                details: "Verified across 10 pages".into(),
                ..Default::default()
            })
            .unwrap();

        assert!(created.id.starts_with("K-"));
        assert_eq!(created.status, "pending");
        assert_eq!(created.confidence, 0.5);

        // Read
        let found = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(found.summary, created.summary);

        // Update status
        storage
            .knowledge()
            .update_status(&created.id, "validated")
            .unwrap();
        let updated = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(updated.status, "validated");

        // Query by domain
        let results = storage
            .knowledge()
            .query_by_domain("github.com", 5)
            .unwrap();
        assert_eq!(results.len(), 1);

        // Delete
        storage.knowledge().delete(&created.id).unwrap();
        assert!(storage.knowledge().get(&created.id).unwrap().is_none());
    }

    #[test]
    fn test_get_nonexistent() {
        let storage = Storage::open_in_memory().unwrap();
        let result = storage.knowledge().get("K-00000000-000000").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_status_not_found() {
        let storage = Storage::open_in_memory().unwrap();
        let result = storage
            .knowledge()
            .update_status("K-00000000-000000", "validated");
        assert!(result.is_err());
    }

    #[test]
    fn test_query_by_domain_empty() {
        let storage = Storage::open_in_memory().unwrap();
        let results = storage
            .knowledge()
            .query_by_domain("nonexistent.com", 10)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_delete_returns_false_for_nonexistent() {
        let storage = Storage::open_in_memory().unwrap();
        let deleted = storage.knowledge().delete("K-00000000-000000").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_effectiveness_computation() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "test".into(),
                summary: "test effectiveness".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Default: 0 success + 0 fail => effectiveness = 0.5
        assert_eq!(created.effectiveness, 0.5);

        // Manually update counts to test the GENERATED ALWAYS column
        storage
            .database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE knowledge SET success_count = 8, fail_count = 2 WHERE id = ?1",
                    params![created.id],
                )?;
                Ok(())
            })
            .unwrap();

        let updated = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert!((updated.effectiveness - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_query_validated_returns_only_validated() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a pending entry
        let _pending = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "pending entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Create and validate an entry
        let validated = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                summary: "validated entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();
        storage
            .knowledge()
            .update_status(&validated.id, "validated")
            .unwrap();

        let results = storage.knowledge().query_validated(10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, validated.id);
        assert_eq!(results[0].status, "validated");
    }

    #[test]
    fn test_query_validated_empty() {
        let storage = Storage::open_in_memory().unwrap();
        let results = storage.knowledge().query_validated(10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_validated_respects_limit() {
        let storage = Storage::open_in_memory().unwrap();

        for i in 0..5 {
            let entry = storage
                .knowledge()
                .create(CreateKnowledgeParams {
                    category: "site_interaction".into(),
                    summary: format!("entry {}", i),
                    details: "details".into(),
                    ..Default::default()
                })
                .unwrap();
            storage
                .knowledge()
                .update_status(&entry.id, "validated")
                .unwrap();
        }

        let results = storage.knowledge().query_validated(3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_mark_promoted_sets_status_and_timestamp() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "to be promoted".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .update_status(&created.id, "validated")
            .unwrap();

        storage
            .knowledge()
            .mark_promoted(&created.id, "Site Adaptation Graph")
            .unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "promoted");
        assert!(entry.promoted_at.is_some());
        assert_eq!(
            entry.promoted_section,
            Some("Site Adaptation Graph".to_string())
        );
    }

    #[test]
    fn test_mark_promoted_not_found() {
        let storage = Storage::open_in_memory().unwrap();
        let result = storage
            .knowledge()
            .mark_promoted("K-00000000-000000", "Some Section");
        assert!(result.is_err());
    }

    #[test]
    fn test_resurrect_entry_changes_status_to_validated() {
        let storage = Storage::open_in_memory().unwrap();

        // Create an entry and set it to archived
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "archived entry".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .update_status(&created.id, "archived")
            .unwrap();

        // Resurrect the entry
        storage.knowledge().resurrect_entry(&created.id).unwrap();

        // Verify status changed to "validated"
        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.status, "validated");
        assert!(entry.last_hit_at.is_some(), "last_hit_at should be set");
    }

    #[test]
    fn test_resurrect_entry_increments_hit_count() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "hit count test".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Default hit_count is 1
        assert_eq!(created.hit_count, 1);

        storage
            .knowledge()
            .update_status(&created.id, "archived")
            .unwrap();

        // Resurrect — should increment hit_count from 1 to 2
        storage.knowledge().resurrect_entry(&created.id).unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.hit_count, 2);

        // Resurrect again — should go to 3
        // (first set back to archived)
        storage
            .knowledge()
            .update_status(&entry.id, "archived")
            .unwrap();
        storage.knowledge().resurrect_entry(&entry.id).unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.hit_count, 3);
    }

    #[test]
    fn test_resurrect_entry_not_found() {
        let storage = Storage::open_in_memory().unwrap();
        let result = storage.knowledge().resurrect_entry("K-00000000-000000");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_duplicate_returns_match() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("example.com".into()),
                summary: "uses data-testid selectors".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        let found = storage
            .knowledge()
            .find_duplicate(
                "site_interaction",
                Some("example.com"),
                "uses data-testid selectors",
            )
            .unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[test]
    fn test_find_duplicate_returns_none_for_no_match() {
        let storage = Storage::open_in_memory().unwrap();

        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("example.com".into()),
                summary: "uses data-testid selectors".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Different category
        let result = storage
            .knowledge()
            .find_duplicate(
                "tool_optimization",
                Some("example.com"),
                "uses data-testid selectors",
            )
            .unwrap();
        assert!(result.is_none());

        // Different domain
        let result = storage
            .knowledge()
            .find_duplicate(
                "site_interaction",
                Some("other.com"),
                "uses data-testid selectors",
            )
            .unwrap();
        assert!(result.is_none());

        // Different summary
        let result = storage
            .knowledge()
            .find_duplicate("site_interaction", Some("example.com"), "different summary")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_duplicate_with_none_domain() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a universal entry (domain = NULL)
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                domain: None,
                summary: "universal knowledge".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Find with domain=None should match domain IS NULL
        let found = storage
            .knowledge()
            .find_duplicate("tool_optimization", None, "universal knowledge")
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);

        // Find with domain=Some should NOT match domain IS NULL
        let not_found = storage
            .knowledge()
            .find_duplicate(
                "tool_optimization",
                Some("example.com"),
                "universal knowledge",
            )
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_merge_entry_increments_hit_count() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "merge test".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Default hit_count is 1
        assert_eq!(created.hit_count, 1);

        // Merge with add_hits=3
        storage
            .knowledge()
            .merge_entry(&created.id, 3, 0.3)
            .unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert_eq!(entry.hit_count, 4);
        assert!(entry.last_hit_at.is_some());
    }

    #[test]
    fn test_merge_entry_takes_max_confidence() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "confidence test".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Default confidence is 0.5
        assert_eq!(created.confidence, 0.5);

        // Merge with higher confidence
        storage
            .knowledge()
            .merge_entry(&created.id, 1, 0.8)
            .unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert!((entry.confidence - 0.8).abs() < 0.001);

        // Merge with lower confidence should NOT downgrade
        storage.knowledge().merge_entry(&entry.id, 1, 0.3).unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert!((entry.confidence - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_query_by_subject_returns_matching() {
        let storage = Storage::open_in_memory().unwrap();

        // Create entries with different categories and domains
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("example.com".into()),
                summary: "entry 1".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "tool_optimization".into(),
                domain: Some("example.com".into()),
                summary: "entry 2".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("other.com".into()),
                summary: "entry 3".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Query by category=site_interaction, domain=example.com
        let results = storage
            .knowledge()
            .query_by_subject("site_interaction", Some("example.com"), 10)
            .unwrap();

        // Should find entry 1 (matching category+domain) but NOT entry 2 (wrong category) or entry 3 (wrong domain)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "entry 1");
    }

    #[test]
    fn test_query_by_subject_includes_universal() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a domain-specific entry
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: Some("example.com".into()),
                summary: "domain specific".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Create a universal entry (domain = NULL)
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                domain: None,
                summary: "universal".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Query with domain=Some("example.com") should return both
        let results = storage
            .knowledge()
            .query_by_subject("site_interaction", Some("example.com"), 10)
            .unwrap();

        assert_eq!(results.len(), 2);
        let summaries: Vec<&str> = results.iter().map(|k| k.summary.as_str()).collect();
        assert!(summaries.contains(&"domain specific"));
        assert!(summaries.contains(&"universal"));
    }

    #[test]
    fn test_list_without_embeddings() {
        let storage = Storage::open_in_memory().unwrap();

        // Create entry without embedding
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "no embedding".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        // Create entry with embedding
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "has embedding".into(),
                details: "details".into(),
                embedding: Some(vec![0.1, 0.2, 0.3]),
                ..Default::default()
            })
            .unwrap();

        let results = storage.knowledge().list_without_embeddings(10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "no embedding");
        assert!(results[0].embedding.is_none());
    }

    #[test]
    fn test_update_embedding() {
        let storage = Storage::open_in_memory().unwrap();

        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "embedding update test".into(),
                details: "details".into(),
                ..Default::default()
            })
            .unwrap();

        assert!(created.embedding.is_none());

        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4, 0.5];
        storage
            .knowledge()
            .update_embedding(&created.id, &embedding)
            .unwrap();

        let entry = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert!(entry.embedding.is_some());
        let stored = entry.embedding.unwrap();
        assert_eq!(stored.len(), 5);
        for (a, b) in stored.iter().zip(embedding.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_create_with_embedding() {
        let storage = Storage::open_in_memory().unwrap();

        let embedding = vec![1.0_f32, -2.5, 3.14159, 0.0, -0.00001];
        let created = storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: "site_interaction".into(),
                summary: "created with embedding".into(),
                details: "details".into(),
                embedding: Some(embedding.clone()),
                ..Default::default()
            })
            .unwrap();

        assert!(created.embedding.is_some());
        let stored = created.embedding.unwrap();
        assert_eq!(stored.len(), embedding.len());
        for (a, b) in stored.iter().zip(embedding.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }

        // Also verify via get()
        let fetched = storage.knowledge().get(&created.id).unwrap().unwrap();
        assert!(fetched.embedding.is_some());
        let fetched_emb = fetched.embedding.unwrap();
        assert_eq!(fetched_emb.len(), embedding.len());
        for (a, b) in fetched_emb.iter().zip(embedding.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }
}
