/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! M3 §6.1: the sidebar keeps each session's output in its own conversation,
//! so it must read the `session_id` the daemon already puts on these frames.
//! An older daemon's frame (no id) still parses, with an empty id.

use shared_protocol::chat::ChatMessage;

#[test]
fn a_stream_chunk_carries_its_session() {
    let msg: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "stream_chunk",
        "payload": {"content": "hi", "done": false, "session_id": "s-9", "session_title": "T"}
    }))
    .expect("stream_chunk parses");
    match msg {
        ChatMessage::StreamChunk(p) => {
            assert_eq!(p.session_id, "s-9");
            assert_eq!(p.session_title.as_deref(), Some("T"));
        }
        other => panic!("not a stream chunk: {other:?}"),
    }
}

#[test]
fn a_stream_chunk_without_a_session_still_parses() {
    let msg: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "stream_chunk",
        "payload": {"content": "hi", "done": true}
    }))
    .expect("stream_chunk parses");
    match msg {
        ChatMessage::StreamChunk(p) => assert_eq!(p.session_id, ""),
        other => panic!("not a stream chunk: {other:?}"),
    }
}

#[test]
fn a_plan_proposal_carries_its_session() {
    let msg: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "plan_proposal",
        "payload": {"summary": "S", "steps": [{"description": "a"}], "session_id": "s-3"}
    }))
    .expect("plan_proposal parses");
    match msg {
        ChatMessage::PlanProposal(p) => {
            assert_eq!(p.session_id, "s-3");
            assert_eq!(p.steps.len(), 1);
        }
        other => panic!("not a plan: {other:?}"),
    }
    let old: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "plan_proposal",
        "payload": {"summary": "S", "steps": []}
    }))
    .expect("an older plan_proposal parses");
    match old {
        ChatMessage::PlanProposal(p) => assert_eq!(p.session_id, ""),
        other => panic!("not a plan: {other:?}"),
    }
}

/// Contract v1.1 R27: seven turn-time `error` frames carry `session_id`;
/// `EMPTY_MESSAGE` (sent before the session is known) does not, and a
/// missing id means "the conversation on screen" (`in_view("", _)` is true).
#[test]
fn an_error_frame_with_or_without_a_session_parses() {
    let with: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "error",
        "payload": {"session_id": "s-4", "code": "AGENT_ERROR", "message": "m", "recoverable": true}
    }))
    .expect("error parses");
    match with {
        ChatMessage::Error(p) => assert_eq!(p.session_id, "s-4"),
        other => panic!("not an error: {other:?}"),
    }
    let without: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "error",
        "payload": {"code": "EMPTY_MESSAGE", "message": "m", "recoverable": true}
    }))
    .expect("an error without a session parses");
    match without {
        ChatMessage::Error(p) => assert_eq!(p.session_id, ""),
        other => panic!("not an error: {other:?}"),
    }
}

/// Contract v1.1 R28: `plan_resolved` (like `browser_tool_resolved`) may carry
/// `reason: "timeout" | "stopped"`; an unknown key must not drop the frame.
#[test]
fn a_plan_resolved_with_a_reason_still_parses() {
    let msg: ChatMessage = serde_json::from_value(serde_json::json!({
        "type": "plan_resolved",
        "payload": {"session_id": "s-5", "response": "cancelled", "reason": "stopped"}
    }))
    .expect("plan_resolved with reason parses");
    match msg {
        ChatMessage::PlanResolved(p) => assert_eq!(p.session_id, "s-5"),
        other => panic!("not plan_resolved: {other:?}"),
    }
}
