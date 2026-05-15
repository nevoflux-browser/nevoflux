//! Types for canvas share bundles and encrypted payloads.
//!
//! A [`ShareBundle`] contains the artifact content and metadata that gets
//! encrypted. An [`EncryptedShareBundle`] wraps the ciphertext with the
//! share ID and KDF parameters needed for decryption.

use serde::{Deserialize, Serialize};

/// Plaintext bundle containing the shared artifact and its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareBundle {
    /// Unique identifier of the artifact being shared.
    pub artifact_id: String,
    /// Human-readable name of the artifact.
    pub artifact_name: String,
    /// Type discriminator (e.g., "canvas", "document", "code").
    pub artifact_type: String,
    /// The artifact content as structured JSON.
    pub content: serde_json::Value,
    /// Metadata about the share.
    pub metadata: ShareMetadata,
}

/// Metadata attached to a shared artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareMetadata {
    /// ISO 8601 timestamp of when the share was created.
    pub created_at: String,
    /// Version string for the share format.
    pub version: String,
    /// Optional author identifier.
    pub author: Option<String>,
}

/// Key derivation function parameters used to derive the encryption key
/// from the password and share ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    /// Algorithm name (always "argon2id").
    pub algorithm: String,
    /// Memory cost in KiB.
    pub memory_kib: u32,
    /// Number of iterations (time cost).
    pub iterations: u32,
    /// Degree of parallelism.
    pub parallelism: u32,
    /// Salt derivation format string.
    pub salt_format: String,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            algorithm: "argon2id".into(),
            memory_kib: 65536,
            iterations: 3,
            parallelism: 4,
            salt_format: "{password}|{share_id}".into(),
        }
    }
}

/// Encrypted share bundle stored on the server.
///
/// Contains everything needed to decrypt the original [`ShareBundle`],
/// given the correct password and share ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedShareBundle {
    /// The share ID (used as part of the KDF salt).
    pub share_id: String,
    /// Parameters for the key derivation function.
    pub kdf_params: KdfParams,
    /// 12-byte AES-GCM nonce.
    pub nonce: Vec<u8>,
    /// Encrypted JSON of the [`ShareBundle`].
    pub ciphertext: Vec<u8>,
    /// 16-byte AES-GCM authentication tag.
    pub auth_tag: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_share_bundle() -> ShareBundle {
        ShareBundle {
            artifact_id: "art-001".into(),
            artifact_name: "My Canvas".into(),
            artifact_type: "canvas".into(),
            content: serde_json::json!({"key": "value"}),
            metadata: ShareMetadata {
                created_at: "2025-01-15T10:30:00Z".into(),
                version: "1.0".into(),
                author: Some("alice".into()),
            },
        }
    }

    fn sample_encrypted_bundle() -> EncryptedShareBundle {
        EncryptedShareBundle {
            share_id: "ABCDEFGHJK".into(),
            kdf_params: KdfParams::default(),
            nonce: vec![0u8; 12],
            ciphertext: vec![0xDE, 0xAD, 0xBE, 0xEF],
            auth_tag: vec![0u8; 16],
        }
    }

    #[test]
    fn test_share_bundle_serde_roundtrip() {
        let bundle = sample_share_bundle();
        let json = serde_json::to_string(&bundle).expect("serialize");
        let decoded: ShareBundle = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.artifact_id, bundle.artifact_id);
        assert_eq!(decoded.artifact_name, bundle.artifact_name);
        assert_eq!(decoded.artifact_type, bundle.artifact_type);
        assert_eq!(decoded.content, bundle.content);
        assert_eq!(decoded.metadata.created_at, bundle.metadata.created_at);
        assert_eq!(decoded.metadata.version, bundle.metadata.version);
        assert_eq!(decoded.metadata.author, bundle.metadata.author);
    }

    #[test]
    fn test_share_metadata_optional_author() {
        let meta = ShareMetadata {
            created_at: "2025-01-15T10:30:00Z".into(),
            version: "1.0".into(),
            author: None,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        let decoded: ShareMetadata = serde_json::from_str(&json).expect("deserialize");
        assert!(decoded.author.is_none());
    }

    #[test]
    fn test_kdf_params_default() {
        let params = KdfParams::default();
        assert_eq!(params.algorithm, "argon2id");
        assert_eq!(params.memory_kib, 65536);
        assert_eq!(params.iterations, 3);
        assert_eq!(params.parallelism, 4);
        assert_eq!(params.salt_format, "{password}|{share_id}");
    }

    #[test]
    fn test_kdf_params_serde_roundtrip() {
        let params = KdfParams::default();
        let json = serde_json::to_string(&params).expect("serialize");
        let decoded: KdfParams = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.algorithm, params.algorithm);
        assert_eq!(decoded.memory_kib, params.memory_kib);
        assert_eq!(decoded.iterations, params.iterations);
        assert_eq!(decoded.parallelism, params.parallelism);
        assert_eq!(decoded.salt_format, params.salt_format);
    }

    #[test]
    fn test_encrypted_share_bundle_serde_roundtrip() {
        let bundle = sample_encrypted_bundle();
        let json = serde_json::to_string(&bundle).expect("serialize");
        let decoded: EncryptedShareBundle = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.share_id, bundle.share_id);
        assert_eq!(decoded.nonce, bundle.nonce);
        assert_eq!(decoded.ciphertext, bundle.ciphertext);
        assert_eq!(decoded.auth_tag, bundle.auth_tag);
        assert_eq!(decoded.kdf_params.algorithm, bundle.kdf_params.algorithm);
    }

    #[test]
    fn test_encrypted_share_bundle_json_structure() {
        let bundle = sample_encrypted_bundle();
        let value: serde_json::Value = serde_json::to_value(&bundle).expect("to_value");

        assert!(value.get("share_id").is_some());
        assert!(value.get("kdf_params").is_some());
        assert!(value.get("nonce").is_some());
        assert!(value.get("ciphertext").is_some());
        assert!(value.get("auth_tag").is_some());
    }
}
