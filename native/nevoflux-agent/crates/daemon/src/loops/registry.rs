//! In-memory map of `LoopId → LoopRuntime` (spec §4 architecture).
//!
//! Concurrent access via `Arc<RwLock<HashMap>>`. The registry is the
//! single source of truth for which loops are alive *now*; the SQLite
//! `loops` table is the persistent record.

use crate::loops::types::{LoopId, LoopRuntime};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default, Clone)]
pub struct LoopRegistry {
    inner: Arc<RwLock<HashMap<LoopId, LoopRuntime>>>,
}

impl LoopRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&self, runtime: LoopRuntime) {
        self.inner.write().expect("registry poisoned").insert(runtime.id.clone(), runtime);
    }

    pub fn remove(&self, id: &LoopId) -> Option<LoopRuntime> {
        self.inner.write().expect("registry poisoned").remove(id)
    }

    pub fn contains(&self, id: &LoopId) -> bool {
        self.inner.read().expect("registry poisoned").contains_key(id)
    }

    pub fn ids(&self) -> Vec<LoopId> {
        self.inner.read().expect("registry poisoned").keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().expect("registry poisoned").len()
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Apply a mutation while holding the write lock briefly.
    /// Returns `None` if the loop is not in the registry.
    pub fn with_mut<F, R>(&self, id: &LoopId, f: F) -> Option<R>
    where
        F: FnOnce(&mut LoopRuntime) -> R,
    {
        self.inner.write().expect("registry poisoned").get_mut(id).map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(id: &str) -> LoopRuntime {
        LoopRuntime::new(LoopId(id.into()), "s".into())
    }

    #[test]
    fn insert_and_remove() {
        let r = LoopRegistry::new();
        let id = LoopId("a".into());
        r.insert(rt("a"));
        assert!(r.contains(&id));
        assert_eq!(r.len(), 1);
        assert!(r.remove(&id).is_some());
        assert!(!r.contains(&id));
        assert!(r.is_empty());
    }

    #[test]
    fn ids_lists_inserted_keys() {
        let r = LoopRegistry::new();
        r.insert(rt("a"));
        r.insert(rt("b"));
        let mut ids: Vec<String> = r.ids().into_iter().map(|i| i.0).collect();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn with_mut_returns_none_for_missing() {
        let r = LoopRegistry::new();
        assert!(r.with_mut(&LoopId("nope".into()), |_| ()).is_none());
    }

    #[test]
    fn with_mut_applies_change() {
        let r = LoopRegistry::new();
        r.insert(rt("a"));
        let prev = r.with_mut(&LoopId("a".into()), |rt| {
            rt.subscription_ids.push("sub-1".into());
            rt.subscription_ids.len()
        });
        assert_eq!(prev, Some(1));
    }

    #[test]
    fn clone_shares_state() {
        let a = LoopRegistry::new();
        let b = a.clone();
        a.insert(rt("x"));
        assert!(b.contains(&LoopId("x".into())));
    }
}
