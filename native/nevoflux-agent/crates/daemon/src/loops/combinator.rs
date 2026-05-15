//! AND/OR combinators for trigger composition (spec §5.2).
//!
//! A combinator runtime is a small state machine that aggregates per-child
//! "fire" pulses into a parent fire. AND requires every child to have fired
//! at least once since the last AND fire (then resets); OR fires on any
//! child.

use std::collections::HashSet;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum CombinatorState {
    And { needed: usize, fired: HashSet<usize> },
    Or,
}

pub struct CombinatorRuntime {
    state: CombinatorState,
    out: mpsc::Sender<()>,
}

impl CombinatorRuntime {
    pub fn new_and(child_count: usize, out: mpsc::Sender<()>) -> Self {
        Self {
            state: CombinatorState::And {
                needed: child_count,
                fired: HashSet::new(),
            },
            out,
        }
    }

    pub fn new_or(out: mpsc::Sender<()>) -> Self {
        Self {
            state: CombinatorState::Or,
            out,
        }
    }

    /// Record a fire from `child_index`. Emits a parent fire when the
    /// combinator's predicate is satisfied; AND resets afterwards.
    pub async fn on_child_fire(&mut self, child_index: usize) {
        match &mut self.state {
            CombinatorState::Or => {
                let _ = self.out.send(()).await;
            }
            CombinatorState::And { needed, fired } => {
                fired.insert(child_index);
                if fired.len() >= *needed {
                    fired.clear();
                    let _ = self.out.send(()).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn or_fires_on_any_child() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut c = CombinatorRuntime::new_or(tx);
        c.on_child_fire(0).await;
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn and_waits_for_all_then_resets() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut c = CombinatorRuntime::new_and(2, tx);

        c.on_child_fire(0).await;
        assert!(rx.try_recv().is_err());

        // Duplicate child fire — set membership doesn't progress.
        c.on_child_fire(0).await;
        assert!(rx.try_recv().is_err());

        c.on_child_fire(1).await;
        assert!(rx.recv().await.is_some());

        // Cycle resets — new fire from child 0 alone shouldn't trip.
        c.on_child_fire(0).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn and_three_children() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut c = CombinatorRuntime::new_and(3, tx);
        c.on_child_fire(0).await;
        c.on_child_fire(1).await;
        assert!(rx.try_recv().is_err());
        c.on_child_fire(2).await;
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn or_subsequent_fires_keep_emitting() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut c = CombinatorRuntime::new_or(tx);
        c.on_child_fire(0).await;
        rx.recv().await.unwrap();
        c.on_child_fire(1).await;
        rx.recv().await.unwrap();
    }
}
