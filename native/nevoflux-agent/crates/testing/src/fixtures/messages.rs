//! Message fixtures for testing.

use nevoflux_storage::{ContentType, Message, MessageRole};

/// Create a sample user message.
pub fn sample_user_message(session_id: &str, content: &str) -> Message {
    Message {
        id: format!("msg-user-{}", uuid_short()),
        session_id: session_id.to_string(),
        role: MessageRole::User,
        content: content.to_string(),
        content_type: ContentType::Text,
        created_at: current_timestamp(),
        metadata: None,
    }
}

/// Create a sample assistant message.
pub fn sample_assistant_message(session_id: &str, content: &str) -> Message {
    Message {
        id: format!("msg-asst-{}", uuid_short()),
        session_id: session_id.to_string(),
        role: MessageRole::Assistant,
        content: content.to_string(),
        content_type: ContentType::Text,
        created_at: current_timestamp(),
        metadata: None,
    }
}

/// Create a sample tool use message.
pub fn sample_tool_use_message(session_id: &str, tool_name: &str, input: &str) -> Message {
    Message {
        id: format!("msg-tool-{}", uuid_short()),
        session_id: session_id.to_string(),
        role: MessageRole::Assistant,
        content: serde_json::json!({
            "tool": tool_name,
            "input": input
        })
        .to_string(),
        content_type: ContentType::ToolUse,
        created_at: current_timestamp(),
        metadata: None,
    }
}

/// Create a sample tool result message.
pub fn sample_tool_result_message(session_id: &str, result: &str) -> Message {
    Message {
        id: format!("msg-result-{}", uuid_short()),
        session_id: session_id.to_string(),
        role: MessageRole::User,
        content: result.to_string(),
        content_type: ContentType::ToolResult,
        created_at: current_timestamp(),
        metadata: None,
    }
}

/// Create a conversation (alternating user/assistant messages).
pub fn sample_conversation(session_id: &str, exchanges: usize) -> Vec<Message> {
    let mut messages = Vec::with_capacity(exchanges * 2);
    for i in 0..exchanges {
        messages.push(sample_user_message(
            session_id,
            &format!("User message {}", i),
        ));
        messages.push(sample_assistant_message(
            session_id,
            &format!("Assistant response {}", i),
        ));
    }
    messages
}

/// Generate a short UUID-like string.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:08x}", (ts as u32).wrapping_mul(2654435761))
}

/// Get current Unix timestamp.
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_user_message() {
        let msg = sample_user_message("sess-001", "Hello!");

        assert!(msg.id.starts_with("msg-user-"));
        assert_eq!(msg.session_id, "sess-001");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello!");
        assert_eq!(msg.content_type, ContentType::Text);
    }

    #[test]
    fn test_sample_assistant_message() {
        let msg = sample_assistant_message("sess-001", "Hi there!");

        assert!(msg.id.starts_with("msg-asst-"));
        assert_eq!(msg.role, MessageRole::Assistant);
    }

    #[test]
    fn test_sample_tool_use_message() {
        let msg = sample_tool_use_message("sess-001", "bash", "ls -la");

        assert!(msg.id.starts_with("msg-tool-"));
        assert_eq!(msg.content_type, ContentType::ToolUse);
        assert!(msg.content.contains("bash"));
    }

    #[test]
    fn test_sample_conversation() {
        let conv = sample_conversation("sess-001", 3);

        assert_eq!(conv.len(), 6);
        assert_eq!(conv[0].role, MessageRole::User);
        assert_eq!(conv[1].role, MessageRole::Assistant);
        assert_eq!(conv[2].role, MessageRole::User);
    }
}
