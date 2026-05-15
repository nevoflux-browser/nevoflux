//! AES-256-GCM encryption for sensitive learning data at rest.
//!
//! Provides a trait-based key management abstraction with two implementations:
//! - [`FileKeyProvider`]: Stores the encryption key in a file with restricted
//!   permissions (`0600` on Unix). Suitable for production use.
//! - [`InMemoryKeyProvider`]: Holds a fixed key in memory. Used for testing.
//!
//! Encryption functions use AES-256-GCM with a random 12-byte nonce prepended
//! to the ciphertext. String variants add base64 encoding on top.

use std::path::{Path, PathBuf};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand::Rng;

use crate::error::{DaemonError, Result};

/// Nonce length for AES-256-GCM (96 bits).
const NONCE_LEN: usize = 12;

/// AES-256 key length in bytes.
const KEY_LEN: usize = 32;

// ---------------------------------------------------------------------------
// KeyProvider trait
// ---------------------------------------------------------------------------

/// Abstraction over encryption key storage.
///
/// Implementations are responsible for persisting and retrieving a 256-bit
/// AES key. The trait is kept synchronous because key operations are infrequent
/// and typically fast (file I/O or memory lookup).
pub trait KeyProvider: Send + Sync {
    /// Retrieve the existing key or create a new random one if none exists.
    fn get_or_create_key(&self) -> Result<[u8; KEY_LEN]>;

    /// Delete the stored key (e.g. when clearing all learning data).
    fn delete_key(&self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// FileKeyProvider
// ---------------------------------------------------------------------------

/// Stores the AES-256 encryption key in a local file with restricted
/// permissions (`0600` on Unix).
///
/// Default path: `~/.config/nevoflux/learning.key`
pub struct FileKeyProvider {
    path: PathBuf,
}

impl FileKeyProvider {
    /// Create a provider that stores the key at the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Create a provider using the default key path
    /// (`~/.config/nevoflux/learning.key`).
    pub fn default_path() -> Result<Self> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            DaemonError::InternalError("Cannot determine config directory".to_string())
        })?;
        let path = config_dir.join("nevoflux").join("learning.key");
        Ok(Self { path })
    }

    /// Return the path where the key file is stored.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Generate a random 256-bit key.
    fn generate_key() -> [u8; KEY_LEN] {
        let mut key = [0u8; KEY_LEN];
        rand::thread_rng().fill(&mut key);
        key
    }

    /// Set restrictive file permissions on Unix (owner read/write only).
    #[cfg(unix)]
    fn set_permissions(path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }

    /// No-op on non-Unix platforms.
    #[cfg(not(unix))]
    fn set_permissions(_path: &Path) -> Result<()> {
        Ok(())
    }
}

impl KeyProvider for FileKeyProvider {
    fn get_or_create_key(&self) -> Result<[u8; KEY_LEN]> {
        if self.path.exists() {
            let data = std::fs::read(&self.path)?;
            if data.len() != KEY_LEN {
                return Err(DaemonError::InternalError(format!(
                    "Key file has invalid length: expected {KEY_LEN}, got {}",
                    data.len()
                )));
            }
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&data);
            Ok(key)
        } else {
            // Ensure parent directory exists.
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let key = Self::generate_key();
            std::fs::write(&self.path, key)?;
            Self::set_permissions(&self.path)?;
            Ok(key)
        }
    }

    fn delete_key(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// InMemoryKeyProvider (for testing)
// ---------------------------------------------------------------------------

/// A key provider that holds a fixed key in memory.
///
/// Useful for deterministic tests that should not touch the filesystem or
/// OS keychain.
pub struct InMemoryKeyProvider {
    key: [u8; KEY_LEN],
}

impl InMemoryKeyProvider {
    /// Create a provider with the given key.
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }

    /// Create a provider with a randomly generated key.
    pub fn random() -> Self {
        let mut key = [0u8; KEY_LEN];
        rand::thread_rng().fill(&mut key);
        Self { key }
    }
}

impl KeyProvider for InMemoryKeyProvider {
    fn get_or_create_key(&self) -> Result<[u8; KEY_LEN]> {
        Ok(self.key)
    }

    fn delete_key(&self) -> Result<()> {
        // Nothing to do for in-memory keys.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Encryption / decryption
// ---------------------------------------------------------------------------

/// Encrypt `plaintext` with AES-256-GCM using the given key.
///
/// The output format is `nonce (12 bytes) || ciphertext`.
pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| DaemonError::InternalError(format!("Encryption failed: {e}")))?;

    let mut output = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data previously encrypted by [`encrypt`].
///
/// Expects `data` to be `nonce (12 bytes) || ciphertext`.
pub fn decrypt(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN {
        return Err(DaemonError::InternalError(
            "Ciphertext too short: missing nonce".to_string(),
        ));
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key.into());

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| DaemonError::InternalError(format!("Decryption failed: {e}")))
}

/// Encrypt a UTF-8 string and return the result as a base64-encoded string.
pub fn encrypt_string(key: &[u8; KEY_LEN], plaintext: &str) -> Result<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let encrypted = encrypt(key, plaintext.as_bytes())?;
    Ok(STANDARD.encode(encrypted))
}

/// Decrypt a base64-encoded string previously produced by [`encrypt_string`].
pub fn decrypt_string(key: &[u8; KEY_LEN], encoded: &str) -> Result<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let data = STANDARD
        .decode(encoded)
        .map_err(|e| DaemonError::InternalError(format!("Base64 decode failed: {e}")))?;

    let plaintext_bytes = decrypt(key, &data)?;

    String::from_utf8(plaintext_bytes)
        .map_err(|e| DaemonError::InternalError(format!("UTF-8 decode failed: {e}")))
}

// ---------------------------------------------------------------------------
// EncryptionService
// ---------------------------------------------------------------------------

/// High-level encryption service that manages a key and provides convenient
/// encrypt/decrypt methods for the learning system.
///
/// Wraps the low-level `encrypt_string` / `decrypt_string` functions and adds
/// privacy-level-aware logic: only "sensitive" data is encrypted, while
/// "public", "internal", and "private" data passes through unchanged.
pub struct EncryptionService {
    key: [u8; KEY_LEN],
}

impl EncryptionService {
    /// Create a new `EncryptionService` by retrieving (or creating) the key
    /// from the given provider.
    pub fn new(provider: &dyn KeyProvider) -> Result<Self> {
        let key = provider.get_or_create_key()?;
        Ok(Self { key })
    }

    /// Encrypt a string only if the privacy level is "sensitive".
    ///
    /// Returns the original string unchanged for non-sensitive data
    /// (e.g., "public", "internal").
    pub fn encrypt_if_sensitive(&self, value: &str, privacy_level: &str) -> Result<String> {
        if privacy_level == "sensitive" {
            encrypt_string(&self.key, value)
        } else {
            Ok(value.to_string())
        }
    }

    /// Decrypt a string if it appears to be encrypted (base64-encoded with
    /// valid AES-256-GCM structure). Returns the original string unchanged
    /// if decryption fails (i.e., the value was stored as plaintext).
    pub fn decrypt_if_encrypted(&self, value: &str) -> Result<String> {
        match decrypt_string(&self.key, value) {
            Ok(decrypted) => Ok(decrypted),
            Err(_) => Ok(value.to_string()),
        }
    }

    /// Always encrypt a string (for USER.md).
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        encrypt_string(&self.key, plaintext)
    }

    /// Always decrypt a string (for USER.md).
    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        decrypt_string(&self.key, encoded)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: generate a random 256-bit key for tests.
    fn random_key() -> [u8; KEY_LEN] {
        let mut key = [0u8; KEY_LEN];
        rand::thread_rng().fill(&mut key);
        key
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = random_key();
        let plaintext = b"Hello, NevoFlux learning system!";

        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_string_decrypt_string_roundtrip() {
        let key = random_key();
        let plaintext = "Sensitive user behavior pattern: prefers dark mode";

        let encrypted = encrypt_string(&key, plaintext).unwrap();
        let decrypted = decrypt_string(&key, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1 = random_key();
        let key2 = random_key();
        // Ensure they are different (astronomically unlikely to match, but be explicit).
        assert_ne!(key1, key2);

        let encrypted = encrypt(&key1, b"secret data").unwrap();
        let result = decrypt(&key2, &encrypted);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Decryption failed"), "got: {err_msg}");
    }

    #[test]
    fn decrypt_corrupted_ciphertext_fails() {
        let key = random_key();
        let mut encrypted = encrypt(&key, b"original data").unwrap();

        // Corrupt a byte in the ciphertext portion (after the nonce).
        if encrypted.len() > NONCE_LEN {
            encrypted[NONCE_LEN] ^= 0xFF;
        }

        let result = decrypt(&key, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_too_short_fails() {
        let key = random_key();

        // Empty input
        let result = decrypt(&key, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));

        // Input shorter than nonce length
        let result = decrypt(&key, &[1, 2, 3]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn encrypt_empty_plaintext() {
        let key = random_key();
        let plaintext = b"";

        let encrypted = encrypt(&key, plaintext).unwrap();
        // Output should contain at least the nonce.
        assert!(encrypted.len() >= NONCE_LEN);

        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // String variant
        let encrypted_str = encrypt_string(&key, "").unwrap();
        let decrypted_str = decrypt_string(&key, &encrypted_str).unwrap();
        assert_eq!(decrypted_str, "");
    }

    #[test]
    fn different_encryptions_produce_different_ciphertext() {
        let key = random_key();
        let plaintext = b"same data";

        let encrypted1 = encrypt(&key, plaintext).unwrap();
        let encrypted2 = encrypt(&key, plaintext).unwrap();

        // Random nonces should make ciphertext differ.
        assert_ne!(encrypted1, encrypted2);

        // Both should decrypt to the same plaintext.
        assert_eq!(decrypt(&key, &encrypted1).unwrap(), plaintext);
        assert_eq!(decrypt(&key, &encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn in_memory_key_provider_returns_consistent_key() {
        let provider = InMemoryKeyProvider::random();

        let key1 = provider.get_or_create_key().unwrap();
        let key2 = provider.get_or_create_key().unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn file_key_provider_creates_and_retrieves_key() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let key_path = tmp_dir.path().join("test-learning.key");
        let provider = FileKeyProvider::new(key_path.clone());

        // Key file should not exist yet.
        assert!(!key_path.exists());

        // First call creates the key.
        let key1 = provider.get_or_create_key().unwrap();
        assert!(key_path.exists());

        // Second call returns the same key.
        let key2 = provider.get_or_create_key().unwrap();
        assert_eq!(key1, key2);

        // Verify file permissions on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&key_path).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Key file should have mode 0600");
        }

        // Delete the key.
        provider.delete_key().unwrap();
        assert!(!key_path.exists());

        // After deletion, a new key is generated.
        let key3 = provider.get_or_create_key().unwrap();
        // It is extremely unlikely (2^-256) that the new key equals the old one.
        assert_ne!(key1, key3);
    }

    // --- EncryptionService tests ---

    #[test]
    fn encryption_service_encrypt_if_sensitive_encrypts() {
        let provider = InMemoryKeyProvider::random();
        let svc = EncryptionService::new(&provider).unwrap();

        let plaintext = "user prefers dark mode";
        let encrypted = svc.encrypt_if_sensitive(plaintext, "sensitive").unwrap();

        // Encrypted output should differ from the plaintext
        assert_ne!(encrypted, plaintext);

        // Should be valid base64 that decrypts back
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encryption_service_encrypt_if_sensitive_passes_through_public() {
        let provider = InMemoryKeyProvider::random();
        let svc = EncryptionService::new(&provider).unwrap();

        let plaintext = "site uses react";

        // Public data should pass through unchanged
        let result = svc.encrypt_if_sensitive(plaintext, "public").unwrap();
        assert_eq!(result, plaintext);

        // Internal data should also pass through unchanged
        let result = svc.encrypt_if_sensitive(plaintext, "internal").unwrap();
        assert_eq!(result, plaintext);

        // Private data should also pass through unchanged (only "sensitive" triggers encryption)
        let result = svc.encrypt_if_sensitive(plaintext, "private").unwrap();
        assert_eq!(result, plaintext);
    }

    #[test]
    fn encryption_service_decrypt_if_encrypted_roundtrip() {
        let provider = InMemoryKeyProvider::random();
        let svc = EncryptionService::new(&provider).unwrap();

        let plaintext = "sensitive behavior pattern";
        let encrypted = svc.encrypt(plaintext).unwrap();

        // decrypt_if_encrypted should successfully decrypt
        let decrypted = svc.decrypt_if_encrypted(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encryption_service_decrypt_if_encrypted_plaintext_passthrough() {
        let provider = InMemoryKeyProvider::random();
        let svc = EncryptionService::new(&provider).unwrap();

        // Plain text that is NOT encrypted should pass through unchanged
        let plaintext = "just a normal string, not encrypted";
        let result = svc.decrypt_if_encrypted(plaintext).unwrap();
        assert_eq!(result, plaintext);

        // Even something that looks vaguely like base64 but isn't valid ciphertext
        let fake_base64 = "SGVsbG8gV29ybGQ="; // "Hello World" in base64, but not AES-GCM
        let result = svc.decrypt_if_encrypted(fake_base64).unwrap();
        assert_eq!(result, fake_base64);
    }
}
