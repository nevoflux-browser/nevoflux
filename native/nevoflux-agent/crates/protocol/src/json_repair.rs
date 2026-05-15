//! Lenient JSON parsing utilities for LLM-generated tool arguments.
//!
//! LLMs sometimes produce malformed JSON strings in tool call arguments:
//! - Invalid escape sequences: `\d`, `\w`, `\s` from regex patterns
//! - Unescaped quotes inside string values: `print("=" * 80)`
//!
//! This module provides a three-tier fallback chain:
//! 1. Standard `serde_json::from_str`
//! 2. Fix invalid escape sequences, retry
//! 3. Lenient field extraction using structural boundary detection

/// Parse a JSON string of tool arguments with fallback recovery.
///
/// Attempts standard JSON parsing first, then fixes invalid escape sequences,
/// and finally tries lenient field extraction for malformed strings with
/// unescaped quotes.
pub fn parse_tool_arguments_json(s: &str) -> serde_json::Value {
    if s.is_empty() {
        return serde_json::json!({});
    }

    if let Ok(v) = serde_json::from_str(s) {
        return v;
    }

    let fixed = fix_invalid_json_escapes(s);
    if let Ok(v) = serde_json::from_str(&fixed) {
        return v;
    }

    // Last resort: LLMs may produce unescaped quotes inside string values.
    // Try lenient field extraction for the "code" field (python-exec).
    if let Some(v) = lenient_extract_json_field(s, "code") {
        return v;
    }

    // Try to repair truncated JSON (common when model output hits token limit).
    // Close any open strings and braces to make it parseable.
    if let Some(v) = repair_truncated_json(s) {
        return v;
    }

    let fixed_truncated = repair_truncated_json(&fixed);
    if let Some(v) = fixed_truncated {
        return v;
    }

    // If all parsing fails, wrap the raw string so callers can still access it.
    serde_json::json!({ "_raw": s })
}

/// Fix invalid JSON escape sequences produced by LLMs.
///
/// LLMs sometimes emit JSON strings containing raw regex patterns like `\d`, `\w`, `\s`
/// which are not valid JSON escapes. This function escapes lone backslashes that are not
/// followed by a valid JSON escape character.
pub fn fix_invalid_json_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 32);
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next)
                    if matches!(next, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u') =>
                {
                    result.push('\\');
                    result.push(next);
                }
                Some(next) => {
                    // Invalid escape like \d, \w, \s — double the backslash
                    result.push('\\');
                    result.push('\\');
                    result.push(next);
                }
                None => {
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Leniently unescape a JSON string value.
///
/// Handles standard JSON escapes (`\"`, `\\`, `\n`, etc.) and treats unknown escape
/// sequences (like `\d`, `\w`) as literal backslash + char. Bare (unescaped) quotes
/// are passed through as-is — this is used after boundary detection has already
/// identified the string extent.
fn unescape_json_string_lenient(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            result.push(ch);
                        }
                    }
                }
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Repair truncated JSON by closing open strings and braces.
///
/// When a model's output hits the token limit, the JSON is cut off mid-value.
/// This function attempts to close any open string and the top-level object
/// so that fields completed before the truncation point can still be parsed.
fn repair_truncated_json(s: &str) -> Option<serde_json::Value> {
    let trimmed = s.trim();
    if !trimmed.starts_with('{') || trimmed.ends_with('}') {
        // Not a truncated object (either not an object, or already complete)
        return None;
    }

    // Walk the string to find the state at the end: are we inside a string?
    let mut in_string = false;
    let mut escape_next = false;
    for ch in trimmed.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    // Try closing the JSON: if inside a string, close it; then close the object.
    let suffix = if in_string { "\"}" } else { "}" };
    let repaired = format!("{}{}", trimmed, suffix);
    serde_json::from_str(&repaired).ok()
}

/// Extract a string field from a malformed JSON object string.
///
/// Last-resort fallback when standard JSON parsing fails due to unescaped quotes
/// inside string values. Finds the field by key name, then determines the value
/// boundaries using structural cues (last `"}` in the string).
///
/// Only reliable for single-field JSON objects like `{"code": "..."}`.
fn lenient_extract_json_field(json_str: &str, field_name: &str) -> Option<serde_json::Value> {
    let trimmed = json_str.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }

    // Find "field_name" followed by :
    let key_pattern = format!("\"{}\"", field_name);
    let key_pos = trimmed.find(&key_pattern)?;
    let after_key = trimmed[key_pos + key_pattern.len()..].trim_start();
    if !after_key.starts_with(':') {
        return None;
    }
    let after_colon = after_key[1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }

    // Value content starts after the opening quote
    let value_start = trimmed.len() - after_colon.len() + 1;

    // Find end: scan backward from end of string.
    // Skip closing }, optional whitespace, then expect closing "
    let bytes = trimmed.as_bytes();
    let mut pos = trimmed.len();
    while pos > value_start && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    if pos <= value_start || bytes[pos - 1] != b'}' {
        return None;
    }
    pos -= 1;
    while pos > value_start && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    if pos <= value_start || bytes[pos - 1] != b'"' {
        return None;
    }
    let value_end = pos - 1;

    let raw = if value_end > value_start {
        &trimmed[value_start..value_end]
    } else {
        ""
    };

    let value = unescape_json_string_lenient(raw);
    let mut map = serde_json::Map::new();
    map.insert(field_name.to_string(), serde_json::Value::String(value));
    Some(serde_json::Value::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_invalid_json_escapes() {
        // Valid escapes should pass through
        let input = r#"{"code": "hello\nworld"}"#;
        assert_eq!(fix_invalid_json_escapes(input), input);

        // Invalid escapes like \d should be double-escaped
        let input = r#"{"code": "re.match(\\d+)"}"#;
        let fixed = fix_invalid_json_escapes(input);
        assert!(fixed.contains(r"\\d"));
    }

    #[test]
    fn test_unescape_json_string_lenient() {
        // Standard escapes
        assert_eq!(
            unescape_json_string_lenient(r#"hello\nworld"#),
            "hello\nworld"
        );
        assert_eq!(unescape_json_string_lenient(r#"a\"b"#), "a\"b");
        assert_eq!(unescape_json_string_lenient(r#"a\\b"#), "a\\b");

        // Unknown escapes preserved as literal
        assert_eq!(unescape_json_string_lenient(r#"\d+"#), r#"\d+"#);

        // Bare quotes pass through
        assert_eq!(
            unescape_json_string_lenient(r#"print("hi")"#),
            r#"print("hi")"#
        );

        // Unicode escape
        assert_eq!(unescape_json_string_lenient(r#"\u0041"#), "A");

        // Mixed: escaped quote + bare quote
        assert_eq!(
            unescape_json_string_lenient(r#"print(\"=" * 80)"#),
            r#"print("=" * 80)"#
        );
    }

    #[test]
    fn test_lenient_extract_json_field_basic() {
        let json = r#"{"code": "print(1)"}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        assert_eq!(result["code"].as_str().unwrap(), "print(1)");
    }

    #[test]
    fn test_lenient_extract_json_field_unescaped_quotes() {
        // Malformed JSON with unescaped quotes
        let json = r#"{"code": "print(\"=" * 80)"}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        assert_eq!(result["code"].as_str().unwrap(), r#"print("=" * 80)"#);
    }

    #[test]
    fn test_lenient_extract_json_field_empty_code() {
        let json = r#"{"code": ""}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        assert_eq!(result["code"].as_str().unwrap(), "");
    }

    #[test]
    fn test_lenient_extract_json_field_with_newlines() {
        let json = r#"{"code": "line1\nline2\nline3"}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        assert_eq!(result["code"].as_str().unwrap(), "line1\nline2\nline3");
    }

    #[test]
    fn test_lenient_extract_json_field_missing_field() {
        let json = r#"{"text": "hello"}"#;
        assert!(lenient_extract_json_field(json, "code").is_none());
    }

    #[test]
    fn test_lenient_extract_json_field_not_json() {
        assert!(lenient_extract_json_field("not json", "code").is_none());
        assert!(lenient_extract_json_field("", "code").is_none());
    }

    #[test]
    fn test_lenient_extract_json_field_complex_code() {
        let json = r#"{"code": "d = {\"key\": \"value\"}\nprint(d)"}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        assert_eq!(
            result["code"].as_str().unwrap(),
            "d = {\"key\": \"value\"}\nprint(d)"
        );
    }

    #[test]
    fn test_parse_tool_arguments_json_valid() {
        let result = parse_tool_arguments_json(r#"{"code": "print(1)"}"#);
        assert_eq!(result["code"].as_str().unwrap(), "print(1)");
    }

    #[test]
    fn test_parse_tool_arguments_json_invalid_escapes() {
        let result = parse_tool_arguments_json(r#"{"code": "re.match(\\d+, s)"}"#);
        assert!(result["code"].as_str().is_some());
    }

    #[test]
    fn test_parse_tool_arguments_json_unescaped_quotes() {
        let malformed = r#"{"code": "print(\"=" * 80)"}"#;
        let result = parse_tool_arguments_json(malformed);
        assert_eq!(result["code"].as_str().unwrap(), r#"print("=" * 80)"#);
    }

    #[test]
    fn test_lenient_extract_json_field_bare_quotes() {
        let json = r#"{"code": "print(\"Title\")\nprint(\"=" * 80)\nprint(\"Done\")"}"#;
        let result = lenient_extract_json_field(json, "code").unwrap();
        let code = result["code"].as_str().unwrap();
        assert!(code.contains("print(\"Title\")"));
        assert!(code.contains("print(\"=\" * 80)"));
    }

    #[test]
    fn test_repair_truncated_json_in_string() {
        // Simulate truncated create_artifact args (cut off inside "content" string)
        let truncated = r#"{"title": "Test Page", "content_type": "text/html", "content": "<!DOCTYPE html><html><body>trunc"#;
        let result = parse_tool_arguments_json(truncated);
        assert_eq!(result["title"].as_str().unwrap(), "Test Page");
        assert_eq!(result["content_type"].as_str().unwrap(), "text/html");
        assert!(result["content"]
            .as_str()
            .unwrap()
            .contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_repair_truncated_json_between_fields() {
        // Truncated between fields
        let truncated = r#"{"title": "Page", "content_type": "text/html""#;
        let result = parse_tool_arguments_json(truncated);
        assert_eq!(result["title"].as_str().unwrap(), "Page");
        assert_eq!(result["content_type"].as_str().unwrap(), "text/html");
    }

    #[test]
    fn test_repair_truncated_json_not_truncated() {
        // Complete JSON should NOT go through repair
        let complete = r#"{"title": "Page"}"#;
        let result = parse_tool_arguments_json(complete);
        assert_eq!(result["title"].as_str().unwrap(), "Page");
    }
}
