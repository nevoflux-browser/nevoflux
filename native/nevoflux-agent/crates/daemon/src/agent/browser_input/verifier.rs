// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Read-back verification for browser_input.
//!
//! After executing a plan, verify() calls Actor `get_content` to read
//! the target element and compares it against the expected text.
//! Returns a VerifyReport with a match boolean and diagnostic causes.

use nevoflux_protocol::BrowserToolAction;
use serde::Serialize;
use serde_json::{json, Value};

use crate::agent::browser_input::bridge::BrowserBridge;
use crate::agent::browser_input::error::BrowserInputError;
use crate::agent::browser_input::plan::InputMode;

/// Report produced by `verify()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
    pub matched: bool,
    pub expected_text: String,
    pub actual_text: String,
    pub mode: InputMode,
    pub possible_causes: Vec<String>,
}

/// Call `browser_get_content` on the target and compare to expected.
///
/// Fill mode expects exact match after trimming.
/// Type mode expects the actual text to end with the expected text
/// (since Type appends to existing content).
pub async fn verify(
    selector: &str,
    expected_text: &str,
    mode: InputMode,
    tab_id: Option<i64>,
    bridge: &dyn BrowserBridge,
) -> Result<VerifyReport, BrowserInputError> {
    let response = bridge
        .call_action(
            BrowserToolAction::GetContent,
            json!({ "selector": selector }),
            tab_id,
        )
        .await?;

    let actual = extract_text(&response);
    let matched = compare(&actual, expected_text, mode);

    let possible_causes = if matched {
        Vec::new()
    } else {
        diagnose_mismatch(&actual, expected_text, mode)
    };

    Ok(VerifyReport {
        matched,
        expected_text: expected_text.to_string(),
        actual_text: actual,
        mode,
        possible_causes,
    })
}

/// Extract a text string from a variety of get_content response shapes.
fn extract_text(value: &Value) -> String {
    // Most common shape: {"text": "..."} or {"value": "..."}
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        return text.to_string();
    }
    if let Some(value_str) = value.get("value").and_then(|v| v.as_str()) {
        return value_str.to_string();
    }
    // Fallback: the entire value was a string
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    // Final fallback: nothing recognizable
    String::new()
}

fn compare(actual: &str, expected: &str, mode: InputMode) -> bool {
    match mode {
        InputMode::Fill => actual.trim() == expected.trim(),
        InputMode::Type => actual.ends_with(expected),
    }
}

/// Heuristic causes for a mismatch, surfaced to the LLM so it can
/// reason about why the input didn't land. Intentionally rule-based
/// (not ML) and limited to a few high-signal cases.
pub fn diagnose_mismatch(actual: &str, expected: &str, _mode: InputMode) -> Vec<String> {
    let mut causes = Vec::new();

    if actual.is_empty() {
        causes.push(
            "Target element is empty after execution. The Actor method may have \
             succeeded at the chrome layer but the framework discarded the input \
             (for example, React component unmounted during dispatch)."
                .into(),
        );
    }
    if actual.starts_with("undefined") {
        causes.push(
            "Output begins with literal 'undefined'. This is the signature of the \
             pre-PR-1 type() bug (el.value + char on contentEditable). If it appears \
             after PR #1 merged, some other code path is still using byte concat."
                .into(),
        );
    }
    if actual.len() > expected.len() * 2 && !actual.is_empty() {
        causes.push(
            "Actual text is more than twice the expected length. The strategy \
             probably failed to clear existing content before writing (Fill mode \
             treated as Type mode)."
                .into(),
        );
    }
    if expected.contains('@') && !actual.contains('@') {
        causes.push(
            "Expected text contains '@' mentions but the actual text does not. \
             This site may need a platform adapter recipe (PR #3) that processes \
             mentions as a multi-step Sequence rather than a single insert."
                .into(),
        );
    }

    causes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::bridge::testing::FakeBridge;

    #[tokio::test]
    async fn fill_mode_exact_match_is_matched() {
        let bridge = FakeBridge::with_response(Ok(json!({"text": "Hello world"})));
        let report = verify("#x", "Hello world", InputMode::Fill, None, &bridge)
            .await
            .unwrap();
        assert!(report.matched);
        assert!(report.possible_causes.is_empty());
    }

    #[tokio::test]
    async fn fill_mode_trim_is_tolerated() {
        let bridge = FakeBridge::with_response(Ok(json!({"text": "  Hello  "})));
        let report = verify("#x", "Hello", InputMode::Fill, None, &bridge)
            .await
            .unwrap();
        assert!(report.matched);
    }

    #[tokio::test]
    async fn fill_mode_mismatch_produces_causes() {
        let bridge = FakeBridge::with_response(Ok(json!({"text": ""})));
        let report = verify("#x", "Hello", InputMode::Fill, None, &bridge)
            .await
            .unwrap();
        assert!(!report.matched);
        assert!(!report.possible_causes.is_empty());
        assert!(report.possible_causes[0].to_lowercase().contains("empty"));
    }

    #[tokio::test]
    async fn type_mode_suffix_match_is_matched() {
        let bridge = FakeBridge::with_response(Ok(json!({"text": "ABCDEF"})));
        let report = verify("#x", "DEF", InputMode::Type, None, &bridge)
            .await
            .unwrap();
        assert!(report.matched);
    }

    #[tokio::test]
    async fn type_mode_no_suffix_is_mismatch() {
        let bridge = FakeBridge::with_response(Ok(json!({"text": "ABC"})));
        let report = verify("#x", "DEF", InputMode::Type, None, &bridge)
            .await
            .unwrap();
        assert!(!report.matched);
    }

    #[test]
    fn diagnose_detects_undefined_prefix() {
        let causes = diagnose_mismatch("undefinedHello", "Hello", InputMode::Fill);
        assert!(causes.iter().any(|c| c.contains("undefined")));
    }

    #[test]
    fn diagnose_detects_missing_mentions() {
        let causes = diagnose_mismatch(
            "Hello alice how are you",
            "Hello @alice how are you",
            InputMode::Fill,
        );
        assert!(causes
            .iter()
            .any(|c| c.contains("@ mentions") || c.contains("mentions")));
    }

    #[test]
    fn diagnose_detects_content_too_long() {
        let actual = "ABC".repeat(20);
        let expected = "X";
        let causes = diagnose_mismatch(&actual, expected, InputMode::Fill);
        assert!(causes
            .iter()
            .any(|c| c.contains("twice") || c.contains("length")));
    }

    #[test]
    fn extract_text_from_text_field() {
        assert_eq!(extract_text(&json!({"text": "hi"})), "hi");
    }

    #[test]
    fn extract_text_from_value_field() {
        assert_eq!(extract_text(&json!({"value": "hi"})), "hi");
    }

    #[test]
    fn extract_text_from_bare_string() {
        assert_eq!(extract_text(&json!("hi")), "hi");
    }

    #[test]
    fn extract_text_empty_on_unknown_shape() {
        assert_eq!(extract_text(&json!({"foo": "bar"})), "");
    }
}
