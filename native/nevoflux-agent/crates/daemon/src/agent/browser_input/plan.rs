// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `ExecutionPlan` and `Action` types for the browser input strategy engine.
//!
//! An `ExecutionPlan` is the output of the pure strategy function
//! `decide()`. It describes exactly which Actor method sequence the
//! executor will run. Flat — `Sequence` cannot contain another
//! `ExecutionPlan`, only leaf `Action`s — to keep the executor simple
//! and plans easy to snapshot-test.

use serde::Serialize;

/// Semantic mode of a browser_input call.
///
/// Matches the `mode` string parameter of the LLM tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    /// Replace existing content (like `browser_fill_by_id`).
    Fill,
    /// Append to existing content (like `browser_type_by_id`).
    Type,
}

/// Leaf actions that the executor dispatches to Actor methods.
///
/// Each variant maps to exactly one `browser.nevoflux.*` Parent API
/// call. Sequences are built from these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Fill { selector: String, text: String },
    RichTextFill { selector: String, text: String },
    Paste { selector: String, text: String },
    SendKey { key: String },
    WaitFor { selector: String, timeout_ms: u64 },
    Click { selector: String },
}

/// Output of the strategy engine: the full plan to execute.
///
/// `Sequence` wraps a `Vec<Action>` — intentionally flat, not
/// recursive. If future strategies need nested plans, they should
/// build them up at `decide()` time and emit a single flat sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPlan {
    /// Standard input/textarea: use Actor `fill()` with native value setter.
    NativeFill { selector: String, text: String },

    /// contentEditable + Fill mode: Actor `fillRichText()` replaces content.
    RichTextFill { selector: String, text: String },

    /// contentEditable + Type mode: Actor `paste()` appends content.
    Paste { selector: String, text: String },

    /// Multi-step sequence (mention flows, compound inputs).
    Sequence(Vec<Action>),

    /// Strategy refused to produce a plan. `recoverable` indicates
    /// whether the caller should retry after page mutation.
    Abort { reason: String, recoverable: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_fill_equality() {
        let a = ExecutionPlan::NativeFill {
            selector: "#x".into(),
            text: "hi".into(),
        };
        let b = ExecutionPlan::NativeFill {
            selector: "#x".into(),
            text: "hi".into(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn different_variants_are_not_equal() {
        let a = ExecutionPlan::NativeFill {
            selector: "#x".into(),
            text: "hi".into(),
        };
        let b = ExecutionPlan::RichTextFill {
            selector: "#x".into(),
            text: "hi".into(),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn sequence_of_actions_can_be_built() {
        let plan = ExecutionPlan::Sequence(vec![
            Action::Paste {
                selector: "#compose".into(),
                text: "Hello ".into(),
            },
            Action::WaitFor {
                selector: ".listbox".into(),
                timeout_ms: 2000,
            },
            Action::SendKey {
                key: "Enter".into(),
            },
        ]);

        if let ExecutionPlan::Sequence(actions) = &plan {
            assert_eq!(actions.len(), 3);
        } else {
            panic!("expected Sequence");
        }
    }

    #[test]
    fn abort_recoverable_flag_is_preserved() {
        let a = ExecutionPlan::Abort {
            reason: "not visible".into(),
            recoverable: true,
        };
        if let ExecutionPlan::Abort { recoverable, .. } = a {
            assert!(recoverable);
        } else {
            panic!("expected Abort");
        }
    }

    #[test]
    fn input_mode_enum_values_are_distinct() {
        assert_ne!(InputMode::Fill, InputMode::Type);
    }
}
