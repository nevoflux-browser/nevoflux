//! In-memory types for /loop runtime state (spec §6, §8).

use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LoopId(pub String);

impl LoopId {
    /// Generate an 8-char lowercase loop id from a v4 UUID's simple form.
    /// This is unique enough within a session — collision probability is
    /// negligible at the scale of "tens of loops per session".
    pub fn generate() -> Self {
        let s = uuid::Uuid::new_v4().simple().to_string();
        Self(s.chars().take(8).collect())
    }
}

impl AsRef<str> for LoopId {
    fn as_ref(&self) -> &str { &self.0 }
}

impl std::fmt::Display for LoopId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// In-memory state for a running loop.
///
/// `cancel_token` cancels the loop as a whole (tear down all triggers).
/// `current_iteration` is set while an iteration is executing; it's the
/// fine-grained token a force-cancel uses to abort an in-flight AgentRunner
/// (spec §8.3).
/// `subscription_ids` are scheduler-issued ids used to unsubscribe trigger
/// sources at cancel time.
/// `dom_watchers` are extension-side watcher ids that need an explicit
/// uninstall RPC at cancel time (Phase 19).
/// `first_cancel_at_ms` records the wall-clock time of the first soft
/// cancel for the two-click force semantics (spec §8.3).
#[derive(Debug)]
pub struct LoopRuntime {
    pub id: LoopId,
    pub session_id: String,
    pub cancel_token: CancellationToken,
    pub subscription_ids: Vec<String>,
    pub current_iteration: Option<Arc<CancellationToken>>,
    pub dom_watchers: Vec<String>,
    pub first_cancel_at_ms: Option<u64>,
}

impl LoopRuntime {
    pub fn new(id: LoopId, session_id: String) -> Self {
        Self {
            id,
            session_id,
            cancel_token: CancellationToken::new(),
            subscription_ids: Vec::new(),
            current_iteration: None,
            dom_watchers: Vec::new(),
            first_cancel_at_ms: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_8_chars() {
        let id = LoopId::generate();
        assert_eq!(id.0.len(), 8);
        // ascii alphanumeric (uuid simple form is hex)
        assert!(id.0.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn generated_ids_differ() {
        let a = LoopId::generate();
        let b = LoopId::generate();
        assert_ne!(a, b, "two consecutive UUID-derived ids should not collide");
    }

    #[test]
    fn loop_runtime_constructs_clean() {
        let id = LoopId("abcd1234".into());
        let rt = LoopRuntime::new(id.clone(), "s".into());
        assert_eq!(rt.id, id);
        assert!(rt.subscription_ids.is_empty());
        assert!(rt.current_iteration.is_none());
        assert!(rt.first_cancel_at_ms.is_none());
        assert!(!rt.cancel_token.is_cancelled());
    }
}
