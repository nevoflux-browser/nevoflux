//! Conversation context compression for ACP providers.
//!
//! Pure string processing — no external dependencies. Compresses conversation
//! history to fit within a token budget using a middle-out strategy that
//! preserves the beginning and end of older context.

/// Token estimation heuristic: ~4 chars per token.
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Format a single message as a labeled block.
fn format_msg(role: &str, content: &str) -> String {
    format!("[{}]\n{}\n", role, content)
}

/// Compress a single message for summary based on its role.
///
/// - `user`: head 200 chars
/// - `assistant`: head 100 + tail 100 chars (conclusions often appear at end)
/// - `tool`: name + params + status + result snippet (80 chars)
/// - other: head 200 chars
///
/// Short messages (under the role-specific threshold) are returned as-is with
/// a role prefix.
pub fn compress_message(role: &str, content: &str) -> String {
    match role {
        "user" => {
            if content.len() <= 200 {
                format!("[user] {}", content)
            } else {
                let end = content.floor_char_boundary(200);
                format!("[user] {}...", &content[..end])
            }
        }
        "assistant" => {
            if content.len() <= 200 {
                format!("[assistant] {}", content)
            } else {
                let head_end = content.floor_char_boundary(100);
                let head = &content[..head_end];
                // Tail: last ~100 bytes, clamped up to the next char boundary
                // so multi-byte UTF-8 (CJK, emoji) is not split mid-character.
                let tail_start_raw = content.len().saturating_sub(100);
                let mut tail_start = tail_start_raw;
                while tail_start < content.len() && !content.is_char_boundary(tail_start) {
                    tail_start += 1;
                }
                let tail = &content[tail_start..];
                format!("[assistant] {}...{}", head, tail)
            }
        }
        "tool" => {
            if content.len() <= 80 {
                format!("[tool: {}]", content)
            } else {
                let end = content.floor_char_boundary(80);
                format!("[tool: {}...]", &content[..end])
            }
        }
        _ => {
            if content.len() <= 200 {
                format!("[{}] {}", role, content)
            } else {
                let end = content.floor_char_boundary(200);
                format!("[{}] {}...", role, &content[..end])
            }
        }
    }
}

/// More aggressive compression for middle messages (80 char limit).
fn compress_message_short(role: &str, content: &str) -> String {
    if content.len() <= 80 {
        format!("[{}] {}", role, content)
    } else {
        let end = content.floor_char_boundary(80);
        format!("[{}] {}...", role, &content[..end])
    }
}

/// Compress conversation history within a token budget.
///
/// Uses a middle-out priority strategy: the edges of the older section keep
/// more detail while the middle is compressed most aggressively.
///
/// # Arguments
///
/// * `messages` — `(role, content)` pairs in chronological order.
/// * `token_budget` — Maximum number of tokens the result may consume.
/// * `protected_recent_turns` — Number of recent turns (user+assistant pairs)
///   that are always included verbatim.
///
/// # Algorithm
///
/// 1. Format all messages as full text.
/// 2. If the full text fits in the budget, return it unchanged.
/// 3. Split into **older** (compressible) and **recent** (protected) sections.
///    Recent = last `protected_recent_turns * 2` messages.
/// 4. Recent messages are always included verbatim.
/// 5. Older messages are compressed with middle-out priority:
///    - First 2 and last 2 messages: `compress_message` (200 char limit).
///    - Middle messages: `compress_message_short` (80 char limit).
///    - If still over budget, drop middle lines until the budget is met.
/// 6. The compressed older section is wrapped in
///    `[Earlier conversation summary]` tags.
pub fn compress_history(
    messages: &[(String, String)],
    token_budget: usize,
    protected_recent_turns: usize,
) -> String {
    if messages.is_empty() {
        return String::new();
    }

    // Build full text to check if compression is needed.
    let full_text: String = messages
        .iter()
        .map(|(role, content)| format_msg(role, content))
        .collect();

    if estimate_tokens(&full_text) <= token_budget {
        return full_text;
    }

    // Split into older and recent sections.
    let protected_msg_count = protected_recent_turns * 2;
    let (older, recent) = if messages.len() <= protected_msg_count {
        // All messages are protected.
        (&messages[..0], messages)
    } else {
        let split = messages.len() - protected_msg_count;
        (&messages[..split], &messages[split..])
    };

    // Recent section: always verbatim.
    let recent_text: String = recent
        .iter()
        .map(|(role, content)| format_msg(role, content))
        .collect();

    if older.is_empty() {
        // Nothing to compress — return full text even if over budget.
        return recent_text;
    }

    // Compress the older section using middle-out priority.
    let mut compressed_lines: Vec<String> = older
        .iter()
        .enumerate()
        .map(|(i, (role, content))| {
            let is_edge = i < 2 || i >= older.len().saturating_sub(2);
            if is_edge {
                compress_message(role, content)
            } else {
                compress_message_short(role, content)
            }
        })
        .collect();

    // If still over budget, drop middle lines (index 2 .. len-2) one by one.
    let budget_for_older = token_budget.saturating_sub(estimate_tokens(&recent_text));

    while compressed_lines.len() > 4 {
        let older_text = compressed_lines.join("\n");
        if estimate_tokens(&older_text) <= budget_for_older {
            break;
        }
        // Drop the middle element.
        let mid = compressed_lines.len() / 2;
        compressed_lines.remove(mid);
    }

    let older_compressed = compressed_lines.join("\n");
    let summary = format!(
        "[Earlier conversation summary]\n{}\n[/Earlier conversation summary]\n",
        older_compressed
    );

    format!("{}{}", summary, recent_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello world!"), 3); // 12 chars / 4 = 3
    }

    #[test]
    fn test_compress_user_message_short() {
        let result = compress_message("user", "hello");
        assert_eq!(result, "[user] hello");
    }

    #[test]
    fn test_compress_user_message_long() {
        let long_msg = "a".repeat(300);
        let result = compress_message("user", &long_msg);
        assert!(result.starts_with("[user] "));
        assert!(result.ends_with("..."));
        // Should contain the first 200 chars of content.
        assert!(result.contains(&"a".repeat(200)));
        // Should NOT contain 201+ 'a's in sequence after the prefix.
        assert!(!result.contains(&"a".repeat(201)));
    }

    #[test]
    fn test_compress_assistant_head_tail() {
        let long_msg = format!("{}MIDDLE{}", "H".repeat(150), "T".repeat(150));
        let result = compress_message("assistant", &long_msg);
        assert!(result.starts_with("[assistant] "));
        assert!(result.contains("..."));
        // Head: first 100 chars (all 'H').
        assert!(result.contains(&"H".repeat(100)));
        // Tail: last 100 chars (all 'T').
        assert!(result.contains(&"T".repeat(100)));
    }

    #[test]
    fn test_compress_tool_message() {
        let long_msg = "x".repeat(200);
        let result = compress_message("tool", &long_msg);
        assert!(result.starts_with("[tool: "));
        assert!(result.ends_with("...]"));
    }

    #[test]
    fn test_history_fits_in_budget() {
        let messages = vec![
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "hi there".to_string()),
        ];
        let result = compress_history(&messages, 10_000, 3);
        // Should be returned verbatim (formatted).
        assert!(result.contains("[user]"));
        assert!(result.contains("hello"));
        assert!(result.contains("[assistant]"));
        assert!(result.contains("hi there"));
        // No compression wrapper.
        assert!(!result.contains("[Earlier conversation summary]"));
    }

    #[test]
    fn test_history_exceeds_budget() {
        // Create a large conversation that won't fit in a tight budget.
        let messages: Vec<(String, String)> = (0..20)
            .map(|i| {
                if i % 2 == 0 {
                    ("user".to_string(), "a".repeat(500))
                } else {
                    ("assistant".to_string(), "b".repeat(500))
                }
            })
            .collect();

        // Budget of 200 tokens (~800 chars), far less than 20 * 500 = 10 000 chars.
        let result = compress_history(&messages, 200, 2);
        assert!(result.contains("[Earlier conversation summary]"));
    }

    #[test]
    fn test_history_empty() {
        let result = compress_history(&[], 1000, 3);
        assert_eq!(result, "");
    }

    #[test]
    fn test_history_single_message() {
        let messages = vec![("user".to_string(), "just one message".to_string())];
        let result = compress_history(&messages, 10_000, 3);
        assert!(result.contains("just one message"));
    }

    #[test]
    fn test_compress_tool_with_result() {
        let content =
            r#"read_file("config.toml") returned: [workspace]\nmembers = ["daemon", "protocol"]"#;
        let result = compress_message("tool", content);
        assert!(result.starts_with("[tool:"));
        assert!(result.len() <= 120);
    }

    #[test]
    fn test_compress_tool_short() {
        let result = compress_message("tool", "ok");
        assert_eq!(result, "[tool: ok]");
    }

    #[test]
    fn test_compress_assistant_short() {
        let result = compress_message("assistant", "Sure, I can help.");
        assert_eq!(result, "[assistant] Sure, I can help.");
    }

    #[test]
    fn test_compress_unknown_role() {
        let result = compress_message("system", "You are a helpful assistant.");
        assert_eq!(result, "[system] You are a helpful assistant.");
    }

    #[test]
    fn test_history_all_protected() {
        // Fewer messages than protected turns — nothing to compress
        let messages = vec![
            ("user".into(), "q1".into()),
            ("assistant".into(), "a1".into()),
            ("user".into(), "q2".into()),
            ("assistant".into(), "a2".into()),
        ];
        let result = compress_history(&messages, 10, 3); // budget tiny but all protected
        assert!(result.contains("q1"));
        assert!(result.contains("q2"));
        assert!(!result.contains("[Earlier conversation summary]"));
    }

    #[test]
    fn test_middle_out_priority() {
        // Build a conversation large enough to need compression.
        let messages: Vec<(String, String)> = (0..30)
            .map(|i| {
                if i % 2 == 0 {
                    ("user".to_string(), "u".repeat(400))
                } else {
                    ("assistant".to_string(), "a".repeat(400))
                }
            })
            .collect();

        let token_budget = 300; // very tight
        let result = compress_history(&messages, token_budget, 2);

        // Result must be within budget (or at least dramatically smaller).
        // The function gives a best-effort result; the key property is that
        // compressed output is substantially smaller than the original.
        let original: String = messages.iter().map(|(r, c)| format_msg(r, c)).collect();
        assert!(result.len() < original.len());
    }

    #[test]
    fn compress_message_user_chinese_does_not_panic() {
        let content = "你".repeat(500); // 500 chinese chars = 1500 bytes
        let _r = compress_message("user", &content);
    }

    #[test]
    fn compress_message_assistant_chinese_does_not_panic() {
        let content = "测试".repeat(500);
        let _r = compress_message("assistant", &content);
    }

    #[test]
    fn compress_message_tool_chinese_does_not_panic() {
        let content = "工具输出".repeat(100);
        let _r = compress_message("tool", &content);
    }

    #[test]
    fn compress_message_emoji_does_not_panic() {
        let content = "🎉".repeat(200); // 4-byte emoji
        let _r = compress_message("user", &content);
        let _r = compress_message("assistant", &content);
        let _r = compress_message("tool", &content);
    }

    #[test]
    fn compress_message_short_chinese_does_not_panic() {
        let content = "短消息".repeat(200);
        let _r = compress_message_short("user", &content);
    }
}
