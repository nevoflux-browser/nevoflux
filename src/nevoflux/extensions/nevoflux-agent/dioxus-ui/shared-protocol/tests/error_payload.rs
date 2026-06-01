//! Regression: the daemon's generic error envelopes (e.g.
//! `SKILL_TOOLS_UNAVAILABLE` from server.rs) carry a minimal payload with no
//! `session_id`/`error_id`/`level`. They must still deserialize as
//! `ChatMessage::Error` so the sidebar renders the error and clears its
//! loading spinner — before adding serde defaults to those fields, the
//! untagged `IncomingMessage` deserialization failed, the sidebar silently
//! dropped the message, and the spinner hung forever (`/brain` reproduced it).
//!
//! Lives as an integration test (not in `chat.rs`'s inline `mod tests`, which
//! is currently broken by unrelated struct-field drift) so it compiles and
//! runs independently.

use shared_protocol::chat::ChatMessage;
use shared_protocol::common::ErrorLevel;

#[test]
fn minimal_error_envelope_deserializes_as_error() {
    let raw = serde_json::json!({
        "type": "error",
        "payload": {
            "code": "SKILL_TOOLS_UNAVAILABLE",
            "message": "Skill 'brain' requires unavailable tools",
            "recoverable": true,
            "missing_tools": ["tool_search", "tool_call_dynamic"]
        }
    });
    let msg: ChatMessage =
        serde_json::from_value(raw).expect("minimal error envelope must deserialize");
    match msg {
        ChatMessage::Error(p) => {
            assert_eq!(p.code, "SKILL_TOOLS_UNAVAILABLE");
            assert!(p.recoverable);
            // Missing fields default rather than failing the whole parse.
            assert_eq!(p.session_id, "");
            assert_eq!(p.error_id, "");
            assert_eq!(p.level, ErrorLevel::Error);
        }
        other => panic!("expected ChatMessage::Error, got {other:?}"),
    }
}
