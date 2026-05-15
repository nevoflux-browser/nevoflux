//! Knowledge export with privacy-aware anonymization.
//!
//! Exports knowledge entries from SQLite while respecting privacy levels:
//! - `public` entries are exported as-is.
//! - `internal` entries have domains anonymized (SHA-256 hash).
//! - `sensitive` and `private` entries are never exported.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;
use nevoflux_storage::{Knowledge, Storage};

/// A single exported knowledge entry with potentially anonymized fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedEntry {
    /// Unique identifier.
    pub id: String,
    /// Category of knowledge.
    pub category: String,
    /// Optional subcategory.
    pub subcategory: Option<String>,
    /// Domain (may be anonymized for internal entries).
    pub domain: Option<String>,
    /// Brief summary of the knowledge.
    pub summary: String,
    /// Detailed description.
    pub details: String,
    /// Confidence score (0.0-1.0).
    pub confidence: f64,
    /// Number of times this knowledge was accessed.
    pub hit_count: i64,
    /// Computed effectiveness score.
    pub effectiveness: f64,
    /// RFC3339 timestamp when created.
    pub created_at: String,
}

/// Full export result containing metadata and entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// Export format version.
    pub version: String,
    /// When the export was created (RFC3339).
    pub exported_at: String,
    /// Total number of entries exported.
    pub count: usize,
    /// Number of entries that were anonymized.
    pub anonymized_count: usize,
    /// Number of entries that were skipped (sensitive/private).
    pub skipped_count: usize,
    /// The exported entries.
    pub entries: Vec<ExportedEntry>,
}

/// Export knowledge entries from storage with privacy-appropriate handling.
///
/// - `public` entries are exported without modification.
/// - `internal` entries have their domain anonymized via SHA-256.
/// - `sensitive` and `private` entries are never exported.
pub fn export_knowledge(storage: &Storage) -> Result<ExportResult> {
    let all_entries = storage.knowledge().query_all(10000)?;

    let mut entries = Vec::new();
    let mut anonymized_count = 0;
    let mut skipped_count = 0;

    for entry in &all_entries {
        match entry.privacy_level.as_str() {
            "public" => {
                entries.push(knowledge_to_export(entry));
            }
            "internal" => {
                entries.push(anonymize_entry(entry));
                anonymized_count += 1;
            }
            // sensitive and private are never exported
            _ => {
                skipped_count += 1;
            }
        }
    }

    let count = entries.len();

    Ok(ExportResult {
        version: "1.0".to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        count,
        anonymized_count,
        skipped_count,
        entries,
    })
}

/// Convert a Knowledge entry to an ExportedEntry without modification.
fn knowledge_to_export(entry: &Knowledge) -> ExportedEntry {
    ExportedEntry {
        id: entry.id.clone(),
        category: entry.category.clone(),
        subcategory: entry.subcategory.clone(),
        domain: entry.domain.clone(),
        summary: entry.summary.clone(),
        details: entry.details.clone(),
        confidence: entry.confidence,
        hit_count: entry.hit_count,
        effectiveness: entry.effectiveness,
        created_at: entry.created_at.clone(),
    }
}

/// Anonymize an internal knowledge entry by hashing the domain.
fn anonymize_entry(entry: &Knowledge) -> ExportedEntry {
    let anonymized_domain = entry.domain.as_ref().map(|d| hash_domain(d));

    ExportedEntry {
        id: entry.id.clone(),
        category: entry.category.clone(),
        subcategory: entry.subcategory.clone(),
        domain: anonymized_domain,
        summary: entry.summary.clone(),
        details: entry.details.clone(),
        confidence: entry.confidence,
        hit_count: entry.hit_count,
        effectiveness: entry.effectiveness,
        created_at: entry.created_at.clone(),
    }
}

/// Hash a domain name with SHA-256, returning the first 16 hex chars.
///
/// Uses the first 8 bytes (16 hex characters) of the SHA-256 digest,
/// which is sufficient to avoid collisions while being unrecoverable.
pub fn hash_domain(domain: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    let result = hasher.finalize();
    // First 16 hex chars (8 bytes)
    hex::encode(&result[..8])
}

/// Serialize an ExportResult to a pretty-printed JSON string.
pub fn export_to_json(result: &ExportResult) -> Result<String> {
    serde_json::to_string_pretty(result).map_err(|e| {
        crate::error::DaemonError::InternalError(format!("Failed to serialize export: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::{CreateKnowledgeParams, Storage};

    fn setup() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    fn create_entry(storage: &Storage, category: &str, domain: Option<&str>, privacy_level: &str) {
        storage
            .knowledge()
            .create(CreateKnowledgeParams {
                category: category.into(),
                summary: format!("{} entry", privacy_level),
                details: "test details".into(),
                domain: domain.map(|d| d.to_string()),
                privacy_level: Some(privacy_level.to_string()),
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn export_includes_public_entries() {
        let storage = setup();
        create_entry(&storage, "site_interaction", Some("github.com"), "public");

        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.entries[0].domain, Some("github.com".to_string()));
        assert_eq!(result.anonymized_count, 0);
    }

    #[test]
    fn export_anonymizes_internal_entries() {
        let storage = setup();
        create_entry(&storage, "site_interaction", Some("taobao.com"), "internal");

        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.anonymized_count, 1);
        // Domain should be hashed, not the original
        assert_ne!(result.entries[0].domain, Some("taobao.com".to_string()));
        assert!(result.entries[0].domain.is_some());
    }

    #[test]
    fn export_skips_sensitive_entries() {
        let storage = setup();
        create_entry(&storage, "user_preference", None, "sensitive");

        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 0);
        assert_eq!(result.skipped_count, 1);
    }

    #[test]
    fn export_skips_private_entries() {
        let storage = setup();
        create_entry(&storage, "user_preference", None, "private");

        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 0);
        assert_eq!(result.skipped_count, 1);
    }

    #[test]
    fn export_mixed_privacy_levels() {
        let storage = setup();
        create_entry(&storage, "site_interaction", Some("github.com"), "public");
        create_entry(&storage, "site_interaction", Some("taobao.com"), "internal");
        create_entry(&storage, "user_preference", None, "sensitive");
        create_entry(&storage, "user_preference", None, "private");

        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 2); // public + internal
        assert_eq!(result.anonymized_count, 1); // internal
        assert_eq!(result.skipped_count, 2); // sensitive + private
    }

    #[test]
    fn hash_domain_is_deterministic() {
        let hash1 = hash_domain("example.com");
        let hash2 = hash_domain("example.com");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_domain_is_different_for_different_domains() {
        let hash1 = hash_domain("example.com");
        let hash2 = hash_domain("different.com");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_domain_returns_16_hex_chars() {
        let hash = hash_domain("example.com");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn export_to_json_produces_valid_json() {
        let storage = setup();
        create_entry(&storage, "site_interaction", Some("github.com"), "public");

        let result = export_knowledge(&storage).unwrap();
        let json = export_to_json(&result).unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["count"], 1);
    }

    #[test]
    fn export_empty_database() {
        let storage = setup();
        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.count, 0);
        assert_eq!(result.anonymized_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn export_result_version_is_1_0() {
        let storage = setup();
        let result = export_knowledge(&storage).unwrap();
        assert_eq!(result.version, "1.0");
    }

    #[test]
    fn export_result_has_exported_at_timestamp() {
        let storage = setup();
        let result = export_knowledge(&storage).unwrap();
        // Should be a valid RFC3339 timestamp
        assert!(result.exported_at.contains('T'));
        assert!(!result.exported_at.is_empty());
    }
}
