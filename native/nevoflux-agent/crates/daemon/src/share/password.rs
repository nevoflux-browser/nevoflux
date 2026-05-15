//! Password generation and parsing for canvas share links.
//!
//! Generates 64-bit (8 byte) random passwords encoded as 13-character
//! Crockford base32 strings, formatted with hyphens as `X-XXXX-XXXX-XXXX`.
//!
//! Parsing is tolerant: strips hyphens/spaces, case-insensitive, and
//! maps commonly confused characters (O->0, I/L->1).

use rand::Rng;

use super::share_id::{crockford_decode, crockford_encode};

/// Generate a random 64-bit password, returned in hyphenated format.
///
/// The password is 8 random bytes encoded as 13 Crockford base32 characters,
/// then formatted as `X-XXXX-XXXX-XXXX` (1-4-4-4 grouping).
pub fn generate_password() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 8];
    rng.fill(&mut bytes);
    // 8 bytes = 64 bits, ceil(64/5) = 13 characters
    let raw = crockford_encode(&bytes);
    format_password(&raw)
}

/// Format a raw 13-character Crockford base32 string into hyphenated form.
///
/// Input: `"ABCDEFGHJKMNP"` -> Output: `"A-BCDE-FGHJ-KMNP"`
///
/// If the input is not exactly 13 characters, it is returned as-is.
pub fn format_password(raw: &str) -> String {
    if raw.len() != 13 {
        return raw.to_string();
    }
    format!(
        "{}-{}-{}-{}",
        &raw[0..1],
        &raw[1..5],
        &raw[5..9],
        &raw[9..13]
    )
}

/// Parse a password string back to its raw bytes.
///
/// Tolerant parsing:
/// - Strips hyphens (`-`) and spaces
/// - Case-insensitive
/// - Maps `O`/`o` -> `0`, `I`/`i`/`L`/`l` -> `1`
///
/// Returns the decoded bytes or an error if the cleaned input is not valid
/// 13-character Crockford base32.
pub fn parse_password(input: &str) -> Result<Vec<u8>, PasswordError> {
    // Strip hyphens and spaces
    let cleaned: String = input.chars().filter(|&c| c != '-' && c != ' ').collect();

    if cleaned.len() != 13 {
        return Err(PasswordError::InvalidLength {
            expected: 13,
            got: cleaned.len(),
        });
    }

    // Apply tolerant character mappings and normalize to uppercase
    let normalized: String = cleaned
        .chars()
        .map(|c| match c {
            'o' | 'O' => '0',
            'i' | 'I' | 'l' | 'L' => '1',
            other => other.to_ascii_uppercase(),
        })
        .collect();

    crockford_decode(&normalized).ok_or(PasswordError::InvalidCharacter)
}

/// Errors that can occur during password parsing.
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("invalid password length: expected {expected} characters (after stripping hyphens), got {got}")]
    InvalidLength { expected: usize, got: usize },

    #[error("password contains invalid Crockford base32 characters")]
    InvalidCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_password_format() {
        let pw = generate_password();
        // Should be in format X-XXXX-XXXX-XXXX (16 chars with hyphens)
        assert_eq!(pw.len(), 16, "Formatted password should be 16 chars: {pw}");
        let parts: Vec<&str> = pw.split('-').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts: {pw}");
        assert_eq!(parts[0].len(), 1, "First part should be 1 char");
        assert_eq!(parts[1].len(), 4, "Second part should be 4 chars");
        assert_eq!(parts[2].len(), 4, "Third part should be 4 chars");
        assert_eq!(parts[3].len(), 4, "Fourth part should be 4 chars");
    }

    #[test]
    fn test_generate_password_uniqueness() {
        let passwords: Vec<String> = (0..100).map(|_| generate_password()).collect();
        let unique: std::collections::HashSet<&String> = passwords.iter().collect();
        assert_eq!(unique.len(), passwords.len());
    }

    #[test]
    fn test_password_roundtrip() {
        for _ in 0..100 {
            let pw = generate_password();
            let parsed = parse_password(&pw).expect("should parse generated password");
            assert_eq!(parsed.len(), 8, "Decoded password should be 8 bytes");

            // Re-encode and re-format should match
            let re_encoded = crockford_encode(&parsed);
            let re_formatted = format_password(&re_encoded);
            assert_eq!(re_formatted, pw, "Roundtrip should produce same password");
        }
    }

    #[test]
    fn test_parse_password_strips_hyphens() {
        let pw = generate_password();
        // Also test without hyphens
        let raw: String = pw.chars().filter(|&c| c != '-').collect();
        let from_formatted = parse_password(&pw).unwrap();
        let from_raw = parse_password(&raw).unwrap();
        assert_eq!(from_formatted, from_raw);
    }

    #[test]
    fn test_parse_password_strips_spaces() {
        let pw = generate_password();
        let with_spaces = pw.replace('-', " ");
        let from_pw = parse_password(&pw).unwrap();
        let from_spaces = parse_password(&with_spaces).unwrap();
        assert_eq!(from_pw, from_spaces);
    }

    #[test]
    fn test_parse_password_case_insensitive() {
        let pw = generate_password();
        let lower = pw.to_lowercase();
        let from_pw = parse_password(&pw).unwrap();
        let from_lower = parse_password(&lower).unwrap();
        assert_eq!(from_pw, from_lower);
    }

    #[test]
    fn test_parse_password_tolerant_mappings() {
        // O -> 0, I -> 1, L -> 1
        // Create a password that starts with "0-1..." and verify O/I/L map correctly
        let result_o = parse_password("O-0000-0000-0000");
        let result_0 = parse_password("0-0000-0000-0000");
        assert_eq!(result_o.unwrap(), result_0.unwrap());

        let result_i = parse_password("I-0000-0000-0000").unwrap();
        let result_l = parse_password("L-0000-0000-0000").unwrap();
        let result_1 = parse_password("1-0000-0000-0000").unwrap();
        assert_eq!(result_i, result_l);
        assert_eq!(result_l, result_1);
    }

    #[test]
    fn test_parse_password_invalid_length() {
        let err = parse_password("A-BCDE-FGHJ").unwrap_err();
        assert!(matches!(err, PasswordError::InvalidLength { .. }));
    }

    #[test]
    fn test_parse_password_invalid_chars() {
        // '!' is not a valid Crockford character
        let err = parse_password("!-BCDE-FGHJ-KMNP").unwrap_err();
        assert!(matches!(err, PasswordError::InvalidCharacter));
    }

    #[test]
    fn test_format_password_wrong_length() {
        // Input not 13 chars should be returned as-is
        assert_eq!(format_password("ABC"), "ABC");
        assert_eq!(format_password(""), "");
    }

    #[test]
    fn test_format_password_correct() {
        assert_eq!(format_password("ABCDEFGHJKMNP"), "A-BCDE-FGHJ-KMNP");
        assert_eq!(format_password("0000000000000"), "0-0000-0000-0000");
    }
}
