// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Executes `ExecutionPlan`s by calling `BrowserBridge` methods.
//!
//! This module is the "glue" between the pure strategy decision
//! layer and the browser side. It walks a plan, translates each
//! leaf `Action` into a `BrowserToolAction` + params JSON, and
//! awaits each call via the bridge.

use nevoflux_protocol::BrowserToolAction;
use serde_json::json;

use crate::agent::browser_input::bridge::BrowserBridge;
use crate::agent::browser_input::error::BrowserInputError;
use crate::agent::browser_input::plan::{Action, ExecutionPlan};

/// Run an entire `ExecutionPlan` via the supplied bridge.
///
/// Returns `Ok(())` if every leaf action succeeded, or the first
/// error wrapped with step context.
pub async fn execute_plan(
    plan: ExecutionPlan,
    tab_id: Option<i64>,
    bridge: &dyn BrowserBridge,
) -> Result<(), BrowserInputError> {
    match plan {
        ExecutionPlan::NativeFill { selector, text } => bridge
            .call_action(
                BrowserToolAction::Fill,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        ExecutionPlan::RichTextFill { selector, text } => bridge
            .call_action(
                BrowserToolAction::FillRichText,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        ExecutionPlan::Paste { selector, text } => bridge
            .call_action(
                BrowserToolAction::Paste,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        ExecutionPlan::Sequence(actions) => {
            let total = actions.len();
            for (i, action) in actions.into_iter().enumerate() {
                if let Err(inner) = execute_action(action, tab_id, bridge).await {
                    return Err(BrowserInputError::ActionFailed {
                        step: i + 1,
                        total,
                        inner: Box::new(inner),
                    });
                }
            }
            Ok(())
        }
        ExecutionPlan::Abort {
            reason,
            recoverable,
        } => Err(BrowserInputError::Aborted {
            reason,
            recoverable,
        }),
    }
}

/// Dispatch a single leaf `Action` via the bridge.
async fn execute_action(
    action: Action,
    tab_id: Option<i64>,
    bridge: &dyn BrowserBridge,
) -> Result<(), BrowserInputError> {
    match action {
        Action::Fill { selector, text } => bridge
            .call_action(
                BrowserToolAction::Fill,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        Action::RichTextFill { selector, text } => bridge
            .call_action(
                BrowserToolAction::FillRichText,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        Action::Paste { selector, text } => bridge
            .call_action(
                BrowserToolAction::Paste,
                json!({ "selector": selector, "text": text }),
                tab_id,
            )
            .await
            .map(|_| ()),
        Action::SendKey { key } => bridge
            .call_action(BrowserToolAction::KeyPress, json!({ "key": key }), tab_id)
            .await
            .map(|_| ()),
        Action::WaitFor {
            selector,
            timeout_ms,
        } => bridge
            .call_action(
                BrowserToolAction::WaitFor,
                json!({ "selector": selector, "timeout_ms": timeout_ms }),
                tab_id,
            )
            .await
            .map(|_| ()),
        Action::Click { selector } => bridge
            .call_action(
                BrowserToolAction::Click,
                json!({ "selector": selector }),
                tab_id,
            )
            .await
            .map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::bridge::testing::FakeBridge;

    #[tokio::test]
    async fn native_fill_calls_fill_action() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!({"success": true})));
        let plan = ExecutionPlan::NativeFill {
            selector: "#a".into(),
            text: "hi".into(),
        };
        execute_plan(plan, Some(1), &bridge).await.unwrap();
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].0, BrowserToolAction::Fill));
        assert_eq!(
            calls[0].1,
            serde_json::json!({"selector": "#a", "text": "hi"})
        );
    }

    #[tokio::test]
    async fn rich_text_fill_calls_fill_rich_text_action() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!({"success": true})));
        let plan = ExecutionPlan::RichTextFill {
            selector: ".compose".into(),
            text: "Hello".into(),
        };
        execute_plan(plan, None, &bridge).await.unwrap();
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].0, BrowserToolAction::FillRichText));
    }

    #[tokio::test]
    async fn paste_calls_paste_action() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!({"success": true})));
        let plan = ExecutionPlan::Paste {
            selector: ".compose".into(),
            text: "!".into(),
        };
        execute_plan(plan, None, &bridge).await.unwrap();
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].0, BrowserToolAction::Paste));
    }

    #[tokio::test]
    async fn sequence_runs_actions_in_order() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!({"success": true})));
        let plan = ExecutionPlan::Sequence(vec![
            Action::Paste {
                selector: "#c".into(),
                text: "A".into(),
            },
            Action::WaitFor {
                selector: ".box".into(),
                timeout_ms: 2000,
            },
            Action::SendKey {
                key: "Enter".into(),
            },
        ]);
        execute_plan(plan, None, &bridge).await.unwrap();
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(matches!(calls[0].0, BrowserToolAction::Paste));
        assert!(matches!(calls[1].0, BrowserToolAction::WaitFor));
        assert!(matches!(calls[2].0, BrowserToolAction::KeyPress));
    }

    #[tokio::test]
    async fn sequence_error_is_wrapped_with_step_context() {
        let bridge = FakeBridge::with_response(Err(BrowserInputError::ChannelClosed));
        let plan = ExecutionPlan::Sequence(vec![
            Action::Paste {
                selector: "#c".into(),
                text: "A".into(),
            },
            Action::Paste {
                selector: "#c".into(),
                text: "B".into(),
            },
        ]);
        let err = execute_plan(plan, None, &bridge).await.unwrap_err();
        match err {
            BrowserInputError::ActionFailed { step, total, inner } => {
                assert_eq!(step, 1);
                assert_eq!(total, 2);
                assert!(matches!(*inner, BrowserInputError::ChannelClosed));
            }
            other => panic!("expected ActionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn abort_plan_returns_aborted_error() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!(null)));
        let plan = ExecutionPlan::Abort {
            reason: "disabled".into(),
            recoverable: false,
        };
        let err = execute_plan(plan, None, &bridge).await.unwrap_err();
        assert!(matches!(
            err,
            BrowserInputError::Aborted {
                recoverable: false,
                ..
            }
        ));
        // No calls should have been made
        assert_eq!(bridge.calls.lock().unwrap().len(), 0);
    }
}
