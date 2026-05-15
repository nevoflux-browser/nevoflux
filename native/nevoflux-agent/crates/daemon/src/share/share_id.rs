//! Share ID generation and validation.
//!
//! Generates 48-bit (6 byte) random identifiers encoded as 10-character
//! Crockford base32 strings. Crockford base32 uses the alphabet
//! `0123456789ABCDEFGHJKMNPQRSTVWXYZ` (excludes I, L, O, U) and is
//! case-insensitive with no padding.

use rand::Rng;

/// Crockford base32 encoding alphabet (32 symbols).
const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Crockford base32 decoding table: maps ASCII byte value to 5-bit value.
/// Invalid characters map to 0xFF.
const CROCKFORD_DECODE: [u8; 128] = {
    let mut table = [0xFFu8; 128];

    // Digits 0-9
    table[b'0' as usize] = 0;
    table[b'1' as usize] = 1;
    table[b'2' as usize] = 2;
    table[b'3' as usize] = 3;
    table[b'4' as usize] = 4;
    table[b'5' as usize] = 5;
    table[b'6' as usize] = 6;
    table[b'7' as usize] = 7;
    table[b'8' as usize] = 8;
    table[b'9' as usize] = 9;

    // Uppercase A-Z (skipping I, L, O, U)
    table[b'A' as usize] = 10;
    table[b'B' as usize] = 11;
    table[b'C' as usize] = 12;
    table[b'D' as usize] = 13;
    table[b'E' as usize] = 14;
    table[b'F' as usize] = 15;
    table[b'G' as usize] = 16;
    table[b'H' as usize] = 17;
    // I is skipped
    table[b'J' as usize] = 18;
    table[b'K' as usize] = 19;
    // L is skipped
    table[b'M' as usize] = 20;
    table[b'N' as usize] = 21;
    // O is skipped
    table[b'P' as usize] = 22;
    table[b'Q' as usize] = 23;
    table[b'R' as usize] = 24;
    table[b'S' as usize] = 25;
    table[b'T' as usize] = 26;
    // U is skipped
    table[b'V' as usize] = 27;
    table[b'W' as usize] = 28;
    table[b'X' as usize] = 29;
    table[b'Y' as usize] = 30;
    table[b'Z' as usize] = 31;

    // Lowercase aliases
    table[b'a' as usize] = 10;
    table[b'b' as usize] = 11;
    table[b'c' as usize] = 12;
    table[b'd' as usize] = 13;
    table[b'e' as usize] = 14;
    table[b'f' as usize] = 15;
    table[b'g' as usize] = 16;
    table[b'h' as usize] = 17;
    table[b'j' as usize] = 18;
    table[b'k' as usize] = 19;
    table[b'm' as usize] = 20;
    table[b'n' as usize] = 21;
    table[b'p' as usize] = 22;
    table[b'q' as usize] = 23;
    table[b'r' as usize] = 24;
    table[b's' as usize] = 25;
    table[b't' as usize] = 26;
    table[b'v' as usize] = 27;
    table[b'w' as usize] = 28;
    table[b'x' as usize] = 29;
    table[b'y' as usize] = 30;
    table[b'z' as usize] = 31;

    table
};

/// Encode a byte slice as Crockford base32 (no padding).
///
/// The output length is `ceil(input_bits / 5)` characters.
pub fn crockford_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }

    let bit_len = data.len() * 8;
    let out_len = bit_len.div_ceil(5);
    let mut result = String::with_capacity(out_len);

    // Process 5 bits at a time from the MSB end
    let mut buffer: u64 = 0;
    let mut bits_in_buffer: u32 = 0;

    for &byte in data {
        buffer = (buffer << 8) | u64::from(byte);
        bits_in_buffer += 8;

        while bits_in_buffer >= 5 {
            bits_in_buffer -= 5;
            let index = ((buffer >> bits_in_buffer) & 0x1F) as usize;
            result.push(CROCKFORD_ALPHABET[index] as char);
        }
    }

    // Handle remaining bits (left-shift to align to 5-bit boundary)
    if bits_in_buffer > 0 {
        let index = ((buffer << (5 - bits_in_buffer)) & 0x1F) as usize;
        result.push(CROCKFORD_ALPHABET[index] as char);
    }

    result
}

/// Decode a Crockford base32 string back to bytes.
///
/// Returns `None` if the input contains invalid characters.
pub fn crockford_decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return Some(Vec::new());
    }

    let mut buffer: u64 = 0;
    let mut bits_in_buffer: u32 = 0;
    let mut result = Vec::new();

    for ch in encoded.bytes() {
        if ch >= 128 {
            return None;
        }
        let val = CROCKFORD_DECODE[ch as usize];
        if val == 0xFF {
            return None;
        }

        buffer = (buffer << 5) | u64::from(val);
        bits_in_buffer += 5;

        if bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            result.push((buffer >> bits_in_buffer) as u8);
        }
    }

    Some(result)
}

/// Generate a random 48-bit share ID as a 10-character Crockford base32 string.
///
/// The ID is generated from 6 cryptographically random bytes, producing a
/// uniform distribution over the 48-bit space (281 trillion possible IDs).
///
/// Output is lowercase — this matches the canonical wire format expected by
/// the deployed CF Worker API. (Decoders are case-insensitive.)
pub fn generate_share_id() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 6];
    rng.fill(&mut bytes);
    // 6 bytes = 48 bits, ceil(48/5) = 10 characters
    crockford_encode(&bytes).to_ascii_lowercase()
}

/// Validate that a string is a well-formed share ID.
///
/// A valid share ID is exactly 10 characters long and contains only valid
/// Crockford base32 characters (0-9, A-H, J-K, M-N, P-T, V-Z,
/// case-insensitive).
pub fn validate_share_id(id: &str) -> bool {
    if id.len() != 10 {
        return false;
    }
    id.bytes().all(|b| {
        if b >= 128 {
            return false;
        }
        CROCKFORD_DECODE[b as usize] != 0xFF
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_share_id_length() {
        let id = generate_share_id();
        assert_eq!(id.len(), 10, "Share ID should be 10 characters, got: {id}");
    }

    #[test]
    fn test_generate_share_id_valid_chars() {
        for _ in 0..100 {
            let id = generate_share_id();
            assert!(validate_share_id(&id), "Generated ID should be valid: {id}");
        }
    }

    #[test]
    fn test_generate_share_id_uniqueness() {
        let ids: Vec<String> = (0..100).map(|_| generate_share_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        // With 48-bit space, 100 random IDs should all be unique
        assert_eq!(
            unique.len(),
            ids.len(),
            "All generated IDs should be unique"
        );
    }

    #[test]
    fn test_validate_share_id_accepts_valid() {
        assert!(validate_share_id("0123456789"));
        assert!(validate_share_id("ABCDEFGHJK"));
        assert!(validate_share_id("MNPQRSTVWX"));
        assert!(validate_share_id("YZ01234567"));
    }

    #[test]
    fn test_validate_share_id_case_insensitive() {
        assert!(validate_share_id("abcdefghjk"));
        assert!(validate_share_id("AbCdEfGhJk"));
    }

    #[test]
    fn test_validate_share_id_rejects_wrong_length() {
        assert!(!validate_share_id(""));
        assert!(!validate_share_id("123456789")); // 9 chars
        assert!(!validate_share_id("12345678901")); // 11 chars
    }

    #[test]
    fn test_validate_share_id_rejects_invalid_chars() {
        // I, L, O, U are excluded from Crockford base32
        assert!(!validate_share_id("ABCDEFGHIJ")); // contains I
        assert!(!validate_share_id("ABCDEFGHLK")); // contains L
        assert!(!validate_share_id("ABCDEFGHOK")); // contains O
        assert!(!validate_share_id("ABCDEFGHUK")); // contains U
        assert!(!validate_share_id("ABCDE!GHJK")); // contains !
    }

    #[test]
    fn test_crockford_encode_decode_roundtrip() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();
            let mut bytes = [0u8; 6];
            rng.fill(&mut bytes);

            let encoded = crockford_encode(&bytes);
            let decoded = crockford_decode(&encoded).expect("decode should succeed");
            assert_eq!(&decoded[..], &bytes[..], "Roundtrip should preserve bytes");
        }
    }

    #[test]
    fn test_crockford_encode_known_value() {
        // All zeros: 6 bytes = 48 zero bits => 10 '0' characters
        let encoded = crockford_encode(&[0u8; 6]);
        assert_eq!(encoded, "0000000000");

        // All ones: 0xFF * 6 = 48 one-bits
        // 9 full 5-bit groups (31='Z') + 3 remaining bits (111) left-shifted by 2 = 11100 = 28 = 'W'
        let encoded = crockford_encode(&[0xFF; 6]);
        assert_eq!(encoded, "ZZZZZZZZZW");
    }

    #[test]
    fn test_crockford_decode_rejects_invalid() {
        assert!(crockford_decode("ABCDE!GHJK").is_none());
        assert!(crockford_decode("ABCDEIGHJK").is_none()); // I is invalid
    }

    #[test]
    fn test_crockford_encode_empty() {
        assert_eq!(crockford_encode(&[]), "");
        assert_eq!(crockford_decode(""), Some(vec![]));
    }
}
