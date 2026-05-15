//! Tool result microcompaction.
//!
//! Clears old large tool results from conversation history before LLM
//! summarization. Preserves the most recent results and small results,
//! replacing only old large ones with a brief placeholder.

use super::ContextMessage;

/// Result of a microcompaction pass.
#[derive(Debug)]
pub struct MicroCompactResult {
    /// Number of tool results cleared.
    pub cleared_count: usize,
    /// Estimated tokens freed (chars removed / 4).
    pub tokens_freed: u32,
}

/// Clears old large tool results from conversation history.
pub struct MicroCompactor {
    /// Keep the N most recent large tool results untouched.
    keep_recent: usize,
    /// Only clear tool results with content longer than this (chars).
    content_threshold: usize,
}

impl MicroCompactor {
    /// Create a new MicroCompactor.
    pub fn new(keep_recent: usize, content_threshold: usize) -> Self {
        Self {
            keep_recent,
            content_threshold,
        }
    }

    /// Compact tool results in-place, returning stats.
    pub fn compact(&self, messages: &mut Vec<ContextMessage>) -> MicroCompactResult {
        // Collect indices of large tool results, from newest to oldest
        let large_tool_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, m)| m.role == "tool" && m.content.len() > self.content_threshold)
            .map(|(i, _)| i)
            .collect();

        // Skip the most recent `keep_recent` large results
        let to_clear = if large_tool_indices.len() > self.keep_recent {
            &large_tool_indices[self.keep_recent..]
        } else {
            return MicroCompactResult {
                cleared_count: 0,
                tokens_freed: 0,
            };
        };

        let mut cleared_count = 0;
        let mut chars_freed: usize = 0;

        for &idx in to_clear {
            let original_len = messages[idx].content.len();
            let preview = make_preview(&messages[idx].content, 100);
            let placeholder = format!(
                "[Tool result cleared ({} chars): {}]",
                original_len, preview
            );
            chars_freed += original_len.saturating_sub(placeholder.len());
            messages[idx].content = placeholder;
            cleared_count += 1;
        }

        MicroCompactResult {
            cleared_count,
            tokens_freed: (chars_freed / 4) as u32,
        }
    }
}

/// Extract a UTF-8 safe preview of the first `max_chars` characters.
fn make_preview(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let boundary = content.floor_char_boundary(max_chars);
    format!("{}...", &content[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ContextMessage {
        ContextMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    fn large_content(size: usize) -> String {
        "x".repeat(size)
    }

    #[test]
    fn test_microcompact_no_tool_messages() {
        let mut messages = vec![
            msg("user", "hello"),
            msg("assistant", "hi there"),
            msg("user", "how are you?"),
        ];
        let original = messages.clone();
        let compactor = MicroCompactor::new(5, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 0);
        assert_eq!(result.tokens_freed, 0);
        assert_eq!(messages.len(), original.len());
        for (a, b) in messages.iter().zip(original.iter()) {
            assert_eq!(a.content, b.content);
        }
    }

    #[test]
    fn test_microcompact_small_results_preserved() {
        let mut messages = vec![
            msg("user", "read this file"),
            msg("tool", "small result under threshold"),
            msg("assistant", "got it"),
            msg("user", "read another"),
            msg("tool", "another small result"),
        ];
        let original_contents: Vec<String> = messages.iter().map(|m| m.content.clone()).collect();
        let compactor = MicroCompactor::new(5, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 0);
        for (m, orig) in messages.iter().zip(original_contents.iter()) {
            assert_eq!(&m.content, orig);
        }
    }

    #[test]
    fn test_microcompact_large_results_cleared() {
        let large = large_content(2000);
        let mut messages = vec![
            msg("user", "read file"),
            msg("tool", &large),
            msg("assistant", "ok"),
            msg("user", "read another"),
            msg("tool", &large),
            msg("assistant", "done"),
        ];
        // keep_recent=1: only the last large tool result is preserved
        let compactor = MicroCompactor::new(1, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 1);
        assert!(result.tokens_freed > 0);
        // First tool result (index 1) should be cleared
        assert!(messages[1].content.starts_with("[Tool result cleared"));
        assert!(messages[1].content.contains("2000 chars"));
        // Second tool result (index 4) should be preserved
        assert_eq!(messages[4].content.len(), 2000);
    }

    #[test]
    fn test_microcompact_keeps_recent() {
        let large = large_content(5000);
        let mut messages = vec![
            msg("tool", &large), // oldest — should be cleared
            msg("tool", &large), // 2nd oldest — should be cleared
            msg("tool", &large), // 3rd — kept (recent 1 of 3)
            msg("tool", &large), // 4th — kept (recent 2 of 3)
            msg("tool", &large), // newest — kept (recent 3 of 3)
        ];
        let compactor = MicroCompactor::new(3, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 2);
        // First two cleared
        assert!(messages[0].content.starts_with("[Tool result cleared"));
        assert!(messages[1].content.starts_with("[Tool result cleared"));
        // Last three preserved
        assert_eq!(messages[2].content.len(), 5000);
        assert_eq!(messages[3].content.len(), 5000);
        assert_eq!(messages[4].content.len(), 5000);
    }

    #[test]
    fn test_microcompact_placeholder_format() {
        let content = format!("PREFIX_{}", "y".repeat(2000));
        let mut messages = vec![msg("tool", &content), msg("tool", "recent small")];
        let compactor = MicroCompactor::new(0, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 1);
        let placeholder = &messages[0].content;
        // Should contain original length
        assert!(placeholder.contains(&format!("{} chars", content.len())));
        // Should contain preview starting with PREFIX_
        assert!(placeholder.contains("PREFIX_"));
        // Should end with ...]
        assert!(placeholder.ends_with("...]"));
    }

    #[test]
    fn test_microcompact_returns_stats() {
        let large = large_content(4000);
        let mut messages = vec![msg("tool", &large), msg("tool", &large)];
        let compactor = MicroCompactor::new(0, 1000);
        let result = compactor.compact(&mut messages);

        assert_eq!(result.cleared_count, 2);
        // Each result was ~4000 chars, placeholder is ~150 chars
        // So freed ~3850 chars each, ~7700 total, /4 = ~1925 tokens
        assert!(result.tokens_freed > 1500);
    }
}
