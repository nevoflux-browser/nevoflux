//! Encryption core for Canvas Share.
//!
//! Derives a 256-bit key from password + share_id using Argon2id, then
//! encrypts/decrypts a [`ShareBundle`] JSON payload with AES-256-GCM.
//!
//! Key material is zeroized after use to limit exposure in memory.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::Argon2;
use rand::Rng;
use zeroize::Zeroize;

use crate::error::{DaemonError, Result};
use crate::share::types::{EncryptedShareBundle, KdfParams, ShareBundle};

/// AES-256-GCM nonce length in bytes (96 bits).
const NONCE_LEN: usize = 12;

/// AES-256-GCM authentication tag length in bytes (128 bits).
const TAG_LEN: usize = 16;

/// Derive a 256-bit key from password and share_id using Argon2id.
///
/// The salt is constructed as `"{password}|{share_id}"` (matching the
/// `salt_format` in [`KdfParams`]).  The returned key array should be
/// [`zeroize`]d by the caller after use.
pub fn derive_key(password: &str, share_id: &str, params: &KdfParams) -> Result<[u8; 32]> {
    let salt = format!("{password}|{share_id}");

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            params.memory_kib,
            params.iterations,
            params.parallelism,
            Some(32),
        )
        .map_err(|e| DaemonError::InternalError(format!("Argon2 params error: {e}")))?,
    );

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut key)
        .map_err(|e| DaemonError::InternalError(format!("Argon2 KDF error: {e}")))?;

    Ok(key)
}

/// Encrypt a [`ShareBundle`] into an [`EncryptedShareBundle`].
///
/// Steps:
/// 1. Serialize the bundle to JSON.
/// 2. Derive a 256-bit key from `password` + `share_id` via Argon2id.
/// 3. Generate a random 12-byte nonce.
/// 4. AES-256-GCM encrypt the JSON plaintext.
/// 5. Zeroize the key.
///
/// The resulting [`EncryptedShareBundle`] stores ciphertext and auth tag
/// separately for clean serialization.
pub fn encrypt_share_bundle(
    bundle: &ShareBundle,
    password: &str,
    share_id: &str,
) -> Result<EncryptedShareBundle> {
    let kdf_params = KdfParams::default();
    let mut key = derive_key(password, share_id, &kdf_params)?;

    let plaintext = serde_json::to_vec(bundle)
        .map_err(|e| DaemonError::InternalError(format!("JSON serialization error: {e}")))?;

    // Generate random 12-byte nonce.
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt.
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| DaemonError::InternalError(format!("AES-256-GCM key error: {e}")))?;

    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| DaemonError::InternalError(format!("AES-256-GCM encrypt error: {e}")))?;

    // Zeroize key material immediately.
    key.zeroize();

    // aes-gcm appends the 16-byte auth tag to the ciphertext.
    let tag_start = ciphertext_with_tag.len() - TAG_LEN;
    let ciphertext = ciphertext_with_tag[..tag_start].to_vec();
    let auth_tag = ciphertext_with_tag[tag_start..].to_vec();

    Ok(EncryptedShareBundle {
        share_id: share_id.to_string(),
        kdf_params,
        nonce: nonce_bytes.to_vec(),
        ciphertext,
        auth_tag,
    })
}

/// Decrypt an [`EncryptedShareBundle`] back into a [`ShareBundle`].
///
/// Steps:
/// 1. Derive the key from `password` + the bundle's `share_id` via Argon2id.
/// 2. Reconstruct the combined ciphertext+tag expected by aes-gcm.
/// 3. AES-256-GCM decrypt.
/// 4. Deserialize JSON into a [`ShareBundle`].
/// 5. Zeroize the key.
///
/// Returns [`DaemonError::InvalidRequest`] when decryption fails (wrong
/// password or corrupted data).
pub fn decrypt_share_bundle(
    encrypted: &EncryptedShareBundle,
    password: &str,
) -> Result<ShareBundle> {
    let mut key = derive_key(password, &encrypted.share_id, &encrypted.kdf_params)?;

    let nonce = Nonce::from_slice(&encrypted.nonce);

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| DaemonError::InternalError(format!("AES-256-GCM key error: {e}")))?;

    // aes-gcm expects ciphertext || tag concatenated.
    let mut combined = Vec::with_capacity(encrypted.ciphertext.len() + encrypted.auth_tag.len());
    combined.extend_from_slice(&encrypted.ciphertext);
    combined.extend_from_slice(&encrypted.auth_tag);

    let plaintext = cipher.decrypt(nonce, combined.as_ref()).map_err(|_| {
        DaemonError::InvalidRequest("Decryption failed: wrong password or corrupted data".into())
    })?;

    // Zeroize key material.
    key.zeroize();

    let bundle: ShareBundle = serde_json::from_slice(&plaintext)
        .map_err(|e| DaemonError::InternalError(format!("JSON deserialization error: {e}")))?;

    Ok(bundle)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::types::ShareMetadata;

    /// Helper: build a sample ShareBundle for testing.
    fn sample_bundle() -> ShareBundle {
        ShareBundle {
            artifact_id: "art-test-001".into(),
            artifact_name: "Test Canvas".into(),
            artifact_type: "canvas".into(),
            content: serde_json::json!({
                "nodes": [{"id": 1, "text": "Hello"}],
                "edges": []
            }),
            metadata: ShareMetadata {
                created_at: "2025-06-01T12:00:00Z".into(),
                version: "1.0".into(),
                author: Some("test-user".into()),
            },
        }
    }

    /// Use fast KDF params so tests run quickly.
    fn fast_kdf_params() -> KdfParams {
        KdfParams {
            algorithm: "argon2id".into(),
            memory_kib: 1024, // 1 MiB instead of 64 MiB
            iterations: 1,
            parallelism: 1,
            salt_format: "{password}|{share_id}".into(),
        }
    }

    // -- derive_key tests --

    #[test]
    fn test_derive_key_deterministic() {
        let params = fast_kdf_params();
        let key1 = derive_key("password123", "ABCDEFGHJK", &params).unwrap();
        let key2 = derive_key("password123", "ABCDEFGHJK", &params).unwrap();
        assert_eq!(key1, key2, "Same inputs must produce the same key");
    }

    #[test]
    fn test_derive_key_different_passwords() {
        let params = fast_kdf_params();
        let key_a = derive_key("alpha", "ABCDEFGHJK", &params).unwrap();
        let key_b = derive_key("bravo", "ABCDEFGHJK", &params).unwrap();
        assert_ne!(
            key_a, key_b,
            "Different passwords must produce different keys"
        );
    }

    #[test]
    fn test_derive_key_different_share_ids() {
        let params = fast_kdf_params();
        let key_a = derive_key("password", "ABCDEFGHJK", &params).unwrap();
        let key_b = derive_key("password", "ZZZZZZZZZZ", &params).unwrap();
        assert_ne!(
            key_a, key_b,
            "Different share IDs must produce different keys"
        );
    }

    #[test]
    fn test_derive_key_produces_32_bytes() {
        let params = fast_kdf_params();
        let key = derive_key("pw", "ABCDEFGHJK", &params).unwrap();
        assert_eq!(key.len(), 32);
    }

    // -- encrypt / decrypt roundtrip tests --

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let bundle = sample_bundle();
        let password = "T-ESTX-PASS-WORD";
        let share_id = "ABCDEFGHJK";

        let encrypted = encrypt_share_bundle(&bundle, password, share_id).unwrap();
        let decrypted = decrypt_share_bundle(&encrypted, password).unwrap();

        assert_eq!(decrypted.artifact_id, bundle.artifact_id);
        assert_eq!(decrypted.artifact_name, bundle.artifact_name);
        assert_eq!(decrypted.artifact_type, bundle.artifact_type);
        assert_eq!(decrypted.content, bundle.content);
        assert_eq!(decrypted.metadata.created_at, bundle.metadata.created_at);
        assert_eq!(decrypted.metadata.version, bundle.metadata.version);
        assert_eq!(decrypted.metadata.author, bundle.metadata.author);
    }

    #[test]
    fn test_decrypt_wrong_password_fails() {
        let bundle = sample_bundle();
        let share_id = "ABCDEFGHJK";

        let encrypted = encrypt_share_bundle(&bundle, "correct-pass", share_id).unwrap();
        let result = decrypt_share_bundle(&encrypted, "wrong-pass");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("wrong password") || err_msg.contains("Decryption failed"),
            "Error should mention decryption failure, got: {err_msg}"
        );
    }

    #[test]
    fn test_decrypt_corrupted_ciphertext_fails() {
        let bundle = sample_bundle();
        let password = "test-password";
        let share_id = "ABCDEFGHJK";

        let mut encrypted = encrypt_share_bundle(&bundle, password, share_id).unwrap();

        // Flip a byte in the ciphertext.
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }

        let result = decrypt_share_bundle(&encrypted, password);
        assert!(result.is_err(), "Corrupted ciphertext must fail decryption");
    }

    #[test]
    fn test_decrypt_corrupted_auth_tag_fails() {
        let bundle = sample_bundle();
        let password = "test-password";
        let share_id = "ABCDEFGHJK";

        let mut encrypted = encrypt_share_bundle(&bundle, password, share_id).unwrap();

        // Flip a byte in the auth tag.
        if !encrypted.auth_tag.is_empty() {
            encrypted.auth_tag[0] ^= 0xFF;
        }

        let result = decrypt_share_bundle(&encrypted, password);
        assert!(result.is_err(), "Corrupted auth tag must fail decryption");
    }

    #[test]
    fn test_encrypt_produces_expected_sizes() {
        let bundle = sample_bundle();
        let encrypted = encrypt_share_bundle(&bundle, "pw", "ABCDEFGHJK").unwrap();

        assert_eq!(encrypted.nonce.len(), NONCE_LEN, "Nonce must be 12 bytes");
        assert_eq!(
            encrypted.auth_tag.len(),
            TAG_LEN,
            "Auth tag must be 16 bytes"
        );
        assert!(
            !encrypted.ciphertext.is_empty(),
            "Ciphertext must not be empty"
        );
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext_each_time() {
        let bundle = sample_bundle();
        let password = "same-password";
        let share_id = "ABCDEFGHJK";

        let enc1 = encrypt_share_bundle(&bundle, password, share_id).unwrap();
        let enc2 = encrypt_share_bundle(&bundle, password, share_id).unwrap();

        // Random nonces should produce different ciphertexts.
        assert_ne!(enc1.nonce, enc2.nonce, "Nonces should differ");
        assert_ne!(
            enc1.ciphertext, enc2.ciphertext,
            "Ciphertexts should differ due to different nonces"
        );

        // Both should decrypt to the same bundle.
        let dec1 = decrypt_share_bundle(&enc1, password).unwrap();
        let dec2 = decrypt_share_bundle(&enc2, password).unwrap();
        assert_eq!(dec1.artifact_id, dec2.artifact_id);
        assert_eq!(dec1.content, dec2.content);
    }

    #[test]
    fn test_key_zeroization() {
        // Verify that derive_key + zeroize pattern works without panicking.
        let params = fast_kdf_params();
        let mut key = derive_key("password", "ABCDEFGHJK", &params).unwrap();
        assert_ne!(key, [0u8; 32], "Key should not be all zeros before zeroize");
        key.zeroize();
        assert_eq!(key, [0u8; 32], "Key should be all zeros after zeroize");
    }

    #[test]
    fn test_encrypted_bundle_share_id_matches() {
        let bundle = sample_bundle();
        let share_id = "QRSTV01234";
        let encrypted = encrypt_share_bundle(&bundle, "pw", share_id).unwrap();
        assert_eq!(
            encrypted.share_id, share_id,
            "Encrypted bundle must carry the share_id"
        );
    }

    #[test]
    fn test_encrypted_bundle_kdf_params_are_default() {
        let bundle = sample_bundle();
        let encrypted = encrypt_share_bundle(&bundle, "pw", "ABCDEFGHJK").unwrap();
        let defaults = KdfParams::default();

        assert_eq!(encrypted.kdf_params.algorithm, defaults.algorithm);
        assert_eq!(encrypted.kdf_params.memory_kib, defaults.memory_kib);
        assert_eq!(encrypted.kdf_params.iterations, defaults.iterations);
        assert_eq!(encrypted.kdf_params.parallelism, defaults.parallelism);
    }
}
