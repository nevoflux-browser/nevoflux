//! Parser for the `time:dynamic` protocol's `loop-meta` JSON block.
//!
//! Spec §5.2 / §7.2: an iteration running under a `time:dynamic` trigger
//! is expected to end its output with a fenced JSON block of the form
//!
//! ```text
//! ```loop-meta
//! { "next_delay_seconds": 240 }
//! ```
//! ```
//!
//! The scheduler reads `next_delay_seconds`, clamps to [60, 3600], and
//! reschedules the next fire after that duration. If the block is missing
//! or unparseable, the default (300s) is used and the iteration is NOT
//! marked as a failure (lenient parser per §15).

use std::time::Duration;

const DEFAULT: Duration = Duration::from_secs(300);
const MIN: u64 = 60;
const MAX: u64 = 3600;

/// Extract the next-delay duration from an iteration's final assistant text.
/// Returns the [60, 3600] clamped value, or the default 300s if absent or
/// malformed.
pub fn extract_next_delay(text: &str) -> Duration {
    let Some(start) = text.find("```loop-meta") else { return DEFAULT };
    let after = &text[start + "```loop-meta".len()..];
    let Some(end) = after.find("```") else { return DEFAULT };
    let body = after[..end].trim();
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return DEFAULT,
    };
    let Some(secs) = v.get("next_delay_seconds").and_then(|x| x.as_u64()) else {
        return DEFAULT;
    };
    Duration::from_secs(secs.clamp(MIN, MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_block() {
        let s = "ok\n```loop-meta\n{\"next_delay_seconds\": 240}\n```\n";
        assert_eq!(extract_next_delay(s), Duration::from_secs(240));
    }

    #[test]
    fn missing_block_returns_default() {
        assert_eq!(extract_next_delay("hello"), DEFAULT);
    }

    #[test]
    fn malformed_json_returns_default() {
        let s = "```loop-meta\nnot json\n```";
        assert_eq!(extract_next_delay(s), DEFAULT);
    }

    #[test]
    fn under_60_clamps_up() {
        let s = "```loop-meta\n{\"next_delay_seconds\": 30}\n```";
        assert_eq!(extract_next_delay(s), Duration::from_secs(60));
    }

    #[test]
    fn over_3600_clamps_down() {
        let s = "```loop-meta\n{\"next_delay_seconds\": 99999}\n```";
        assert_eq!(extract_next_delay(s), Duration::from_secs(3600));
    }

    #[test]
    fn zero_clamps_up_to_60() {
        let s = "```loop-meta\n{\"next_delay_seconds\": 0}\n```";
        assert_eq!(extract_next_delay(s), Duration::from_secs(60));
    }

    #[test]
    fn block_with_surrounding_prose() {
        let s = "Iteration done.\n\n```loop-meta\n{\"next_delay_seconds\": 90}\n```\n\nMore text after.";
        assert_eq!(extract_next_delay(s), Duration::from_secs(90));
    }

    #[test]
    fn block_without_closing_fence_returns_default() {
        let s = "```loop-meta\n{\"next_delay_seconds\": 120}";
        assert_eq!(extract_next_delay(s), DEFAULT);
    }

    #[test]
    fn unrelated_field_returns_default() {
        let s = "```loop-meta\n{\"other_field\": 240}\n```";
        assert_eq!(extract_next_delay(s), DEFAULT);
    }
}
