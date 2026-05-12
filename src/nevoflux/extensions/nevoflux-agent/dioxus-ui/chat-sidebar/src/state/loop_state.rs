/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Sidebar state for the `/loop` skill.

use std::collections::VecDeque;

/// Per-loop sidebar state, populated from `system:loop:*` EventBus deliveries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoopState {
    pub loop_id: String,
    pub session_id: String,
    pub trigger_expr: String,
    pub prompt_text: Option<String>,
    pub wrapped_skill: Option<String>,
    pub state: String, // pending|running|idle|failed|cancelled
    pub iteration_count: i64,
    pub skipped_triggers: i64,
    pub scratchpad_preview: String,
    pub scratchpad_bytes: i64,
    /// Most recent first; capped at 20.
    pub iterations: VecDeque<IterationRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IterationRow {
    pub sequence_number: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: String, // running|ok|error
    pub fire_reason: String,
    pub tool_calls_summary: serde_json::Value,
}

impl LoopState {
    pub fn push_or_update_iteration(&mut self, row: IterationRow) {
        if let Some(existing) = self
            .iterations
            .iter_mut()
            .find(|r| r.sequence_number == row.sequence_number)
        {
            *existing = row;
            return;
        }
        self.iterations.push_front(row);
        if self.iterations.len() > 20 {
            self.iterations.pop_back();
        }
    }
}
