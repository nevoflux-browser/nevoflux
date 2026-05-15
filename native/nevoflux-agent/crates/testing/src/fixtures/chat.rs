//! Chat message fixtures for testing protocol types.

use nevoflux_protocol::{
    ChatMessage, ContentBlock, ContentType, ErrorLevel, ErrorMessage, PermissionRequest,
    PermissionScope, Requester, RequesterType, ResourceAction, ResourceType, StreamChunk,
    StreamEnd, StreamFormat, StreamMetadata,
};

/// Create a sample chat message.
pub fn sample_chat_message(session_id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        session_id: session_id.to_string(),
        message_id: format!("msg-{}", uuid_short()),
        text: text.to_string(),
        attachments: vec![],
        tab_id: None,
        tab_ids: vec![],
    }
}

/// Create a sample stream chunk.
pub fn sample_stream_chunk(session_id: &str, delta: &str) -> StreamChunk {
    StreamChunk {
        session_id: session_id.to_string(),
        stream_id: format!("stream-{}", uuid_short()),
        delta: delta.to_string(),
        format: StreamFormat::Markdown,
        event: None,
        thinking_event: None,
    }
}

/// Create a sample stream end message.
pub fn sample_stream_end(session_id: &str, stream_id: &str) -> StreamEnd {
    StreamEnd {
        session_id: session_id.to_string(),
        stream_id: stream_id.to_string(),
        metadata: Some(StreamMetadata {
            total_tokens: Some(150),
            duration_ms: Some(1200),
            model: Some("claude-3-sonnet".to_string()),
        }),
    }
}

/// Create a sample permission request.
pub fn sample_permission_request(
    session_id: &str,
    resource_type: ResourceType,
    action: ResourceAction,
    resource: &str,
) -> PermissionRequest {
    PermissionRequest {
        request_id: format!("perm-{}", uuid_short()),
        session_id: session_id.to_string(),
        resource_type,
        action,
        resource: resource.to_string(),
        requester: Requester {
            requester_type: RequesterType::Agent,
            id: "nevoflux-agent".to_string(),
            name: "NevoFlux Agent".to_string(),
        },
        reason: "Testing permission request".to_string(),
        scope: PermissionScope::Session,
        timeout_ms: 60000,
    }
}

/// Create a sample error message.
pub fn sample_error(session_id: &str, code: &str, message: &str) -> ErrorMessage {
    ErrorMessage {
        session_id: session_id.to_string(),
        error_id: format!("err-{}", uuid_short()),
        level: ErrorLevel::Error,
        code: code.to_string(),
        message: message.to_string(),
        details: None,
        recoverable: true,
        retry_action: None,
        related_request_id: None,
    }
}

/// Create a sample content block with code.
pub fn sample_code_block(session_id: &str, language: &str, code: &str) -> ContentBlock {
    ContentBlock {
        session_id: session_id.to_string(),
        block_id: format!("block-{}", uuid_short()),
        content_type: ContentType::Code,
        content: serde_json::json!({
            "language": language,
            "code": code
        }),
        metadata: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_chat_message() {
        let msg = sample_chat_message("sess-001", "Hello!");

        assert_eq!(msg.session_id, "sess-001");
        assert!(msg.message_id.starts_with("msg-"));
        assert_eq!(msg.text, "Hello!");
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_sample_stream_chunk() {
        let chunk = sample_stream_chunk("sess-001", "Hello, ");

        assert_eq!(chunk.session_id, "sess-001");
        assert_eq!(chunk.delta, "Hello, ");
        assert_eq!(chunk.format, StreamFormat::Markdown);
    }

    #[test]
    fn test_sample_permission_request() {
        let req = sample_permission_request(
            "sess-001",
            ResourceType::File,
            ResourceAction::Read,
            "/home/user/file.txt",
        );

        assert_eq!(req.session_id, "sess-001");
        assert_eq!(req.resource_type, ResourceType::File);
        assert_eq!(req.action, ResourceAction::Read);
        assert_eq!(req.resource, "/home/user/file.txt");
    }

    #[test]
    fn test_sample_error() {
        let err = sample_error("sess-001", "LLM_TIMEOUT", "Request timed out");

        assert_eq!(err.code, "LLM_TIMEOUT");
        assert_eq!(err.message, "Request timed out");
        assert!(err.recoverable);
    }

    #[test]
    fn test_sample_code_block() {
        let block = sample_code_block("sess-001", "rust", "fn main() {}");

        assert_eq!(block.content_type, ContentType::Code);
        assert_eq!(block.content["language"], "rust");
        assert_eq!(block.content["code"], "fn main() {}");
    }
}
