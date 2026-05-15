//! Owner token generation and hashing.
//!
//! The owner token is a 32-byte random secret that proves ownership of a
//! shared canvas. The server stores `SHA-256(share_id || token)` so the
//! raw token never leaves the client.

use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random 32-byte owner token.
pub fn generate_owner_token() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut token = vec![0u8; 32];
    rng.fill(token.as_mut_slice());
    token
}

/// Compute the ownership proof hash: `hex(SHA-256(share_id || token))`.
///
/// The server stores this hash. The client proves ownership by presenting
/// the raw token, which the server can verify by recomputing the hash.
pub fn hash_owner_token(share_id: &str, token: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(share_id.as_bytes());
    hasher.update(token);
    let digest = hasher.finalize();
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_owner_token_length() {
        let token = generate_owner_token();
        assert_eq!(token.len(), 32, "Owner token should be 32 bytes");
    }

    #[test]
    fn test_generate_owner_token_uniqueness() {
        let tokens: Vec<Vec<u8>> = (0..100).map(|_| generate_owner_token()).collect();
        let unique: std::collections::HashSet<Vec<u8>> = tokens.into_iter().collect();
        assert_eq!(unique.len(), 100, "All tokens should be unique");
    }

    #[test]
    fn test_hash_owner_token_deterministic() {
        let share_id = "ABCDEFGHJK";
        let token = vec![0x42u8; 32];

        let hash1 = hash_owner_token(share_id, &token);
        let hash2 = hash_owner_token(share_id, &token);

        assert_eq!(hash1, hash2, "Same inputs should produce same hash");
    }

    #[test]
    fn test_hash_owner_token_hex_format() {
        let share_id = "ABCDEFGHJK";
        let token = vec![0x00u8; 32];

        let hash = hash_owner_token(share_id, &token);

        // SHA-256 produces 32 bytes = 64 hex characters
        assert_eq!(hash.len(), 64, "Hash should be 64 hex characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex characters"
        );
    }

    #[test]
    fn test_hash_owner_token_different_share_ids() {
        let token = vec![0x42u8; 32];

        let hash1 = hash_owner_token("AAAAAAAAAA", &token);
        let hash2 = hash_owner_token("BBBBBBBBBB", &token);

        assert_ne!(
            hash1, hash2,
            "Different share IDs should produce different hashes"
        );
    }

    #[test]
    fn test_hash_owner_token_different_tokens() {
        let share_id = "ABCDEFGHJK";
        let token1 = vec![0x00u8; 32];
        let token2 = vec![0xFFu8; 32];

        let hash1 = hash_owner_token(share_id, &token1);
        let hash2 = hash_owner_token(share_id, &token2);

        assert_ne!(
            hash1, hash2,
            "Different tokens should produce different hashes"
        );
    }

    #[test]
    fn test_hash_owner_token_known_value() {
        // Verify against a known SHA-256 computation:
        // SHA-256("TEST" || [0x00; 32]) should be deterministic
        let hash = hash_owner_token("TEST", &[0x00; 32]);

        // Compute expected value
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"TEST");
        hasher.update(&[0x00; 32]);
        let expected = hex::encode(hasher.finalize());

        assert_eq!(hash, expected);
    }
}
