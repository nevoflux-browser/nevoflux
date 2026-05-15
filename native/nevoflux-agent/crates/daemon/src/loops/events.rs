//! EventBus emitters for /loop (spec §11).
//!
//! All loop events publish under the `system:loop:*` topic prefix and
//! always include `{session_id, loop_id}` in their payload so subscribers
//! can filter without relying on topic-level uniqueness.
//!
//! Iteration-start/end and scratchpad events use Ephemeral delivery
//! (transient, in-flight signaling). State changes use Sticky so a sidebar
//! reopening on the same session sees the current state immediately.

use crate::event_bus::types::{BusEvent, PublisherIdentity};
use crate::event_bus::EventBus;
use crate::loops::types::LoopId;
use serde_json::json;
use std::sync::Arc;

/// Helper around an EventBus handle. If `bus` is `None`, every method is a no-op.
/// LoopManager holds an `Option<Arc<EventBus>>` so unit tests that don't wire
/// a bus still work.
pub struct LoopEvents {
    bus: Option<Arc<EventBus>>,
}

impl LoopEvents {
    pub fn new(bus: Option<Arc<EventBus>>) -> Self {
        Self { bus }
    }

    pub fn from_arc(bus: Arc<EventBus>) -> Self {
        Self { bus: Some(bus) }
    }

    pub async fn created(
        &self,
        session_id: &str,
        id: &LoopId,
        trigger_expr: &str,
        prompt_text: Option<&str>,
        wrapped_skill: Option<&str>,
    ) {
        let Some(bus) = &self.bus else {
            return;
        };
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:created",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "trigger_expr": trigger_expr,
                    "prompt_text": prompt_text,
                    "wrapped_skill": wrapped_skill,
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn state_changed(
        &self,
        session_id: &str,
        id: &LoopId,
        new_state: &str,
        prev_state: &str,
        reason: Option<&str>,
    ) {
        let Some(bus) = &self.bus else {
            return;
        };
        let _ = bus
            .publish(BusEvent::sticky(
                "system:loop:state_changed",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "new_state": new_state,
                    "prev_state": prev_state,
                    "reason": reason,
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn iteration_start(
        &self,
        session_id: &str,
        id: &LoopId,
        sequence: i64,
        started_at: i64,
        fire_reason: &str,
    ) {
        let Some(bus) = &self.bus else {
            return;
        };
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:iteration_start",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "sequence_number": sequence,
                    "started_at": started_at,
                    "fire_reason": fire_reason,
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn iteration_end(
        &self,
        session_id: &str,
        id: &LoopId,
        sequence: i64,
        ended_at: i64,
        status: &str,
        tool_calls_summary: serde_json::Value,
        final_text: Option<&str>,
    ) {
        let Some(bus) = &self.bus else {
            return;
        };
        // Cap final_text at 4KB to avoid bloating event payloads when an
        // iteration produces a long response. Sidebar rendering is for
        // quick inspection, not full transcripts — anything longer can
        // be moved to scratchpad explicitly.
        let final_text_capped: Option<String> = final_text.map(|s| {
            if s.len() > 4096 {
                s.chars().take(4096).collect()
            } else {
                s.to_string()
            }
        });
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:iteration_end",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "sequence_number": sequence,
                    "ended_at": ended_at,
                    "status": status,
                    "tool_calls_summary": tool_calls_summary,
                    "final_text": final_text_capped,
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn scratchpad_changed(&self, session_id: &str, id: &LoopId, content: &str) {
        let Some(bus) = &self.bus else {
            return;
        };
        let preview: String = content.chars().take(120).collect();
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:scratchpad_changed",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "preview": preview,
                    "bytes": content.len(),
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn trigger_dropped(&self, session_id: &str, id: &LoopId, skipped_count: i64) {
        let Some(bus) = &self.bus else {
            return;
        };
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:trigger_dropped",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "skipped_count": skipped_count,
                    "reason": "concurrent_iteration",
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }

    pub async fn cancelled(&self, session_id: &str, id: &LoopId, by: &str, force: bool) {
        let Some(bus) = &self.bus else {
            return;
        };
        let _ = bus
            .publish(BusEvent::ephemeral(
                "system:loop:cancelled",
                json!({
                    "session_id": session_id,
                    "loop_id": id.as_ref(),
                    "cancelled_by": by,
                    "force": force,
                }),
                PublisherIdentity::Internal,
            ))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_op_when_bus_is_none() {
        // Just exercises the early-return branches; should not panic.
        let evts = LoopEvents::new(None);
        let id = LoopId("abc".into());
        evts.created("s1", &id, "time:5m", None, None).await;
        evts.state_changed("s1", &id, "running", "pending", None)
            .await;
        evts.iteration_start("s1", &id, 1, 100, "time").await;
        evts.iteration_end("s1", &id, 1, 110, "ok", serde_json::json!([]), None)
            .await;
        evts.scratchpad_changed("s1", &id, "k=v").await;
        evts.trigger_dropped("s1", &id, 1).await;
        evts.cancelled("s1", &id, "user", false).await;
    }
}
