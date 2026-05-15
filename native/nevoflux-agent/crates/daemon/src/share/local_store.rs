//! Local encrypted storage for share passwords and owner tokens.
//!
//! Passwords and owner tokens are sensitive: we encrypt them at rest
//! using AES-256-GCM before writing to SQLite.

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;

use crate::error::{DaemonError, Result};

/// Encrypt a plaintext string using AES-256-GCM with the given 32-byte key.
/// Returns base64-encoded (nonce || ciphertext_with_tag).
pub fn encrypt_for_storage(plaintext: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| DaemonError::InternalError(format!("AES key error: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| DaemonError::InternalError(format!("Encrypt error: {}", e)))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt a value previously produced by `encrypt_for_storage`.
pub fn decrypt_from_storage(encoded: &str, key: &[u8; 32]) -> Result<String> {
    let combined = B64
        .decode(encoded)
        .map_err(|e| DaemonError::InvalidRequest(format!("Invalid base64: {}", e)))?;

    if combined.len() < 12 + 16 {
        return Err(DaemonError::InvalidRequest(
            "Encrypted data too short".into(),
        ));
    }

    let nonce = Nonce::from_slice(&combined[..12]);
    let ciphertext = &combined[12..];

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| DaemonError::InternalError(format!("AES key error: {}", e)))?;

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        DaemonError::InvalidRequest("Decryption failed: wrong key or corrupted".into())
    })?;

    String::from_utf8(plaintext)
        .map_err(|e| DaemonError::InvalidRequest(format!("Invalid UTF-8: {}", e)))
}

/// Encrypt bytes (for owner tokens which are raw bytes, not strings).
pub fn encrypt_bytes_for_storage(plaintext: &[u8], key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| DaemonError::InternalError(format!("AES key error: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| DaemonError::InternalError(format!("Encrypt error: {}", e)))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt bytes previously produced by `encrypt_bytes_for_storage`.
pub fn decrypt_bytes_from_storage(encoded: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
    let combined = B64
        .decode(encoded)
        .map_err(|e| DaemonError::InvalidRequest(format!("Invalid base64: {}", e)))?;

    if combined.len() < 12 + 16 {
        return Err(DaemonError::InvalidRequest(
            "Encrypted data too short".into(),
        ));
    }

    let nonce = Nonce::from_slice(&combined[..12]);
    let ciphertext = &combined[12..];

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| DaemonError::InternalError(format!("AES key error: {}", e)))?;

    cipher.decrypt(nonce, ciphertext).map_err(|_| {
        DaemonError::InvalidRequest("Decryption failed: wrong key or corrupted".into())
    })
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

    fn other_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        k
    }

    #[test]
    fn string_roundtrip() {
        let key = test_key();
        let plaintext = "my-super-secret-share-password-123!";
        let encoded = encrypt_for_storage(plaintext, &key).expect("encrypt");
        let decoded = decrypt_from_storage(&encoded, &key).expect("decrypt");
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn empty_string_roundtrip() {
        let key = test_key();
        let encoded = encrypt_for_storage("", &key).expect("encrypt");
        let decoded = decrypt_from_storage(&encoded, &key).expect("decrypt");
        assert_eq!(decoded, "");
    }

    #[test]
    fn unicode_string_roundtrip() {
        let key = test_key();
        let plaintext = "密码-пароль-🔐";
        let encoded = encrypt_for_storage(plaintext, &key).expect("encrypt");
        let decoded = decrypt_from_storage(&encoded, &key).expect("decrypt");
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn bytes_roundtrip() {
        let key = test_key();
        let plaintext: Vec<u8> = (0u8..=255).collect();
        let encoded = encrypt_bytes_for_storage(&plaintext, &key).expect("encrypt");
        let decoded = decrypt_bytes_from_storage(&encoded, &key).expect("decrypt");
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn empty_bytes_roundtrip() {
        let key = test_key();
        let encoded = encrypt_bytes_for_storage(&[], &key).expect("encrypt");
        let decoded = decrypt_bytes_from_storage(&encoded, &key).expect("decrypt");
        assert!(decoded.is_empty());
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key = test_key();
        let wrong = other_key();
        let encoded = encrypt_for_storage("sensitive", &key).expect("encrypt");
        let err = decrypt_from_storage(&encoded, &wrong);
        assert!(err.is_err(), "decryption with wrong key must fail");
    }

    #[test]
    fn decrypt_wrong_key_bytes_fails() {
        let key = test_key();
        let wrong = other_key();
        let encoded = encrypt_bytes_for_storage(&[1, 2, 3, 4, 5], &key).expect("encrypt");
        let err = decrypt_bytes_from_storage(&encoded, &wrong);
        assert!(err.is_err(), "byte decryption with wrong key must fail");
    }

    #[test]
    fn decrypt_corrupted_fails() {
        let key = test_key();
        let encoded = encrypt_for_storage("hello", &key).expect("encrypt");
        // Flip a byte in the base64 payload (decoded area — change a middle char)
        let mut bytes = encoded.into_bytes();
        let mid = bytes.len() / 2;
        bytes[mid] = if bytes[mid] == b'A' { b'B' } else { b'A' };
        let corrupted = String::from_utf8(bytes).unwrap();
        let err = decrypt_from_storage(&corrupted, &key);
        assert!(err.is_err(), "decryption of corrupted data must fail");
    }

    #[test]
    fn decrypt_invalid_base64_fails() {
        let key = test_key();
        let err = decrypt_from_storage("!!!not-base64!!!", &key);
        assert!(err.is_err());
    }

    #[test]
    fn decrypt_too_short_fails() {
        let key = test_key();
        // 10 bytes, shorter than 12 (nonce) + 16 (tag)
        let short = B64.encode([0u8; 10]);
        let err = decrypt_from_storage(&short, &key);
        assert!(err.is_err());
        let err_b = decrypt_bytes_from_storage(&short, &key);
        assert!(err_b.is_err());
    }

    #[test]
    fn random_nonce_produces_different_ciphertext() {
        let key = test_key();
        let plaintext = "same-plaintext";
        let a = encrypt_for_storage(plaintext, &key).expect("encrypt a");
        let b = encrypt_for_storage(plaintext, &key).expect("encrypt b");
        let c = encrypt_for_storage(plaintext, &key).expect("encrypt c");
        assert_ne!(a, b, "encryption must use fresh nonce");
        assert_ne!(b, c, "encryption must use fresh nonce");
        assert_ne!(a, c, "encryption must use fresh nonce");
        // But all decrypt back to the same plaintext
        assert_eq!(decrypt_from_storage(&a, &key).unwrap(), plaintext);
        assert_eq!(decrypt_from_storage(&b, &key).unwrap(), plaintext);
        assert_eq!(decrypt_from_storage(&c, &key).unwrap(), plaintext);
    }

    #[test]
    fn random_nonce_produces_different_ciphertext_bytes() {
        let key = test_key();
        let plaintext = b"same-plaintext-bytes";
        let a = encrypt_bytes_for_storage(plaintext, &key).expect("encrypt a");
        let b = encrypt_bytes_for_storage(plaintext, &key).expect("encrypt b");
        assert_ne!(a, b, "byte encryption must use fresh nonce");
    }
}
