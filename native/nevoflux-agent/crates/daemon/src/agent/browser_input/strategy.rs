// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pure strategy decision function.
//!
//! `decide()` is the entry point for the strategy engine: given an
//! element fingerprint, input text, desired mode, and current
//! platform adapter (always None in PR #2), it returns an
//! `ExecutionPlan` describing how to fulfill the input request.
//!
//! This module is deliberately side-effect-free. All IO happens
//! in the executor. Testing `decide()` is a matter of constructing
//! input structs and asserting on the returned plan — no mocks or
//! async runtime required.

use crate::agent::browser_input::fingerprint::Fingerprint;
use crate::agent::browser_input::plan::{Action, ExecutionPlan, InputMode};
use crate::agent::browser_input::platform_adapter::Recipe;

/// Input to the strategy decision.
///
/// `adapter` is the platform recipe for the current page's hostname,
/// if one is registered. PR #3 introduced this; callers resolve
/// hostname and look up the recipe before calling `decide()`.
pub struct StrategyInput<'a> {
    pub selector: &'a str,
    pub text: &'a str,
    pub mode: InputMode,
    pub fingerprint: &'a Fingerprint,
    pub hostname: &'a str,
    /// Platform recipe for this hostname, if one is registered.
    /// `None` means "no recipe applies; fall through to generic strategy".
    pub adapter: Option<&'a Recipe>,
}

/// Try to produce a platform-specific plan from the active recipe.
///
/// Returns `Some(ExecutionPlan::Sequence(..))` if the recipe has a
/// mention config AND the input text contains a match for the
/// mention pattern. Otherwise returns `None` so `decide()` falls
/// through to the generic strategy branches.
///
/// Hashtag handling is intentionally deferred to a future PR; the
/// recipe field is parsed but ignored here.
fn apply_recipe(recipe: &Recipe, input: &StrategyInput) -> Option<ExecutionPlan> {
    let mention_cfg = recipe.mention.as_ref()?;
    // Cheap contains check first so we avoid compiling the regex on
    // every call when the text obviously has no trigger character.
    if !input.text.contains(&mention_cfg.trigger_char) {
        return None;
    }
    let regex = regex::Regex::new(&mention_cfg.pattern).ok()?;
    if !regex.is_match(input.text) {
        return None;
    }
    Some(build_mention_sequence(input, mention_cfg, &regex))
}

/// Build the action sequence for a mention-containing text.
///
/// The algorithm walks `input.text` via the compiled regex and splits
/// it into alternating literal-text and mention segments. Each
/// mention expands into:
///   Paste(mention)
///   WaitFor(candidate_list_selector, timeout)
///   SendKey("Enter")  or  Click(candidate_item_selector)
///
/// Empty prefix/suffix text segments are skipped to avoid
/// redundant no-op Paste actions.
fn build_mention_sequence(
    input: &StrategyInput,
    mention: &crate::agent::browser_input::platform_adapter::MentionConfig,
    regex: &regex::Regex,
) -> ExecutionPlan {
    use crate::agent::browser_input::platform_adapter::ConfirmMethod;

    let target = input
        .fingerprint
        .innermost_editable_selector
        .as_deref()
        .unwrap_or(input.selector)
        .to_string();

    let text = input.text;
    let mut actions: Vec<Action> = Vec::new();
    let mut last_end = 0usize;

    for m in regex.find_iter(text) {
        let prefix = &text[last_end..m.start()];
        if !prefix.is_empty() {
            actions.push(Action::Paste {
                selector: target.clone(),
                text: prefix.to_string(),
            });
        }

        // Paste the raw mention token (e.g. "@nevoflux").
        let mention_text = m.as_str().to_string();
        actions.push(Action::Paste {
            selector: target.clone(),
            text: mention_text,
        });

        // Wait for the candidate list to appear.
        actions.push(Action::WaitFor {
            selector: mention.candidate_list_selector.clone(),
            timeout_ms: mention.candidate_list_timeout_ms,
        });

        // Confirm per the recipe's confirm_method.
        match mention.confirm_method {
            ConfirmMethod::EnterKey => {
                actions.push(Action::SendKey {
                    key: "Enter".to_string(),
                });
            }
            ConfirmMethod::ClickFirst => {
                let selector = mention
                    .candidate_item_selector
                    .clone()
                    .unwrap_or_else(|| format!("{} :first-child", mention.candidate_list_selector));
                actions.push(Action::Click { selector });
            }
        }

        last_end = m.end();
    }

    let suffix = &text[last_end..];
    if !suffix.is_empty() {
        actions.push(Action::Paste {
            selector: target,
            text: suffix.to_string(),
        });
    }

    ExecutionPlan::Sequence(actions)
}

/// Pure strategy decision.
///
/// Given a fingerprint and input spec, produce an `ExecutionPlan`.
/// This function performs no IO; it is a transform over its inputs.
pub fn decide(input: &StrategyInput) -> ExecutionPlan {
    // Rejected cases: disabled, readonly, invisible.
    if input.fingerprint.disabled || input.fingerprint.readonly {
        return ExecutionPlan::Abort {
            reason: "Element is disabled or readonly".into(),
            recoverable: false,
        };
    }
    if !input.fingerprint.is_visible {
        return ExecutionPlan::Abort {
            reason: "Element is not visible".into(),
            recoverable: true,
        };
    }

    // Platform adapter first: mention flows and other recipe-driven
    // special cases override the generic branches.
    if let Some(recipe) = input.adapter {
        if let Some(plan) = apply_recipe(recipe, input) {
            return plan;
        }
    }

    // contentEditable branch: different plan per input mode.
    if input.fingerprint.is_content_editable {
        // If the Actor identified a deeper editable target (e.g., Draft.js
        // nested wrappers), prefer it over the caller-supplied selector.
        let target_selector = input
            .fingerprint
            .innermost_editable_selector
            .as_deref()
            .unwrap_or(input.selector)
            .to_string();

        return match input.mode {
            InputMode::Fill => ExecutionPlan::RichTextFill {
                selector: target_selector,
                text: input.text.to_string(),
            },
            InputMode::Type => ExecutionPlan::Paste {
                selector: target_selector,
                text: input.text.to_string(),
            },
        };
    }

    // Standard input/textarea: native value setter.
    if input.fingerprint.has_value_property {
        return ExecutionPlan::NativeFill {
            selector: input.selector.to_string(),
            text: input.text.to_string(),
        };
    }

    // Last resort: unknown element with no supported path.
    ExecutionPlan::Abort {
        reason: format!(
            "Element tag={} is not editable (no .value property and not contentEditable)",
            input.fingerprint.tag
        ),
        recoverable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::fingerprint::EditorFramework;

    /// Build a baseline Fingerprint that matches a standard <input type="text">.
    fn standard_input_fp() -> Fingerprint {
        Fingerprint {
            tag: "input".into(),
            input_type: Some("text".into()),
            has_value_property: true,
            is_content_editable: false,
            disabled: false,
            readonly: false,
            is_visible: true,
            is_focusable: true,
            editor_framework: None,
            react_fiber_present: false,
            inside_iframe: false,
            shadow_root_depth: 0,
            innermost_editable_selector: None,
            computed_role: None,
        }
    }

    /// Build a baseline Fingerprint that matches a Draft.js-style compose box.
    fn draft_js_fp() -> Fingerprint {
        Fingerprint {
            tag: "div".into(),
            input_type: None,
            has_value_property: false,
            is_content_editable: true,
            disabled: false,
            readonly: false,
            is_visible: true,
            is_focusable: true,
            editor_framework: Some(EditorFramework::DraftJs),
            react_fiber_present: true,
            inside_iframe: false,
            shadow_root_depth: 0,
            innermost_editable_selector: Some("div.public-DraftEditor-content".into()),
            computed_role: Some("textbox".into()),
        }
    }

    // ===== Standard input + rejection tests (Task 7) =====

    #[test]
    fn standard_input_fill_returns_native_fill() {
        let fp = standard_input_fp();
        let input = StrategyInput {
            selector: "#name",
            text: "Hello",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        let plan = decide(&input);
        assert_eq!(
            plan,
            ExecutionPlan::NativeFill {
                selector: "#name".into(),
                text: "Hello".into(),
            }
        );
    }

    #[test]
    fn standard_textarea_fill_returns_native_fill() {
        let mut fp = standard_input_fp();
        fp.tag = "textarea".into();
        fp.input_type = None;

        let input = StrategyInput {
            selector: "#msg",
            text: "Multi\nline",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        let plan = decide(&input);
        assert_eq!(
            plan,
            ExecutionPlan::NativeFill {
                selector: "#msg".into(),
                text: "Multi\nline".into(),
            }
        );
    }

    #[test]
    fn disabled_element_aborts_non_recoverable() {
        let mut fp = standard_input_fp();
        fp.disabled = true;

        let input = StrategyInput {
            selector: "#x",
            text: "",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        let plan = decide(&input);
        match plan {
            ExecutionPlan::Abort { recoverable, .. } => assert!(!recoverable),
            other => panic!("expected Abort, got {:?}", other),
        }
    }

    #[test]
    fn readonly_element_aborts_non_recoverable() {
        let mut fp = standard_input_fp();
        fp.readonly = true;

        let input = StrategyInput {
            selector: "#x",
            text: "",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        assert!(matches!(
            decide(&input),
            ExecutionPlan::Abort {
                recoverable: false,
                ..
            }
        ));
    }

    #[test]
    fn invisible_element_aborts_recoverable() {
        let mut fp = standard_input_fp();
        fp.is_visible = false;

        let input = StrategyInput {
            selector: "#x",
            text: "",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        assert!(matches!(
            decide(&input),
            ExecutionPlan::Abort {
                recoverable: true,
                ..
            }
        ));
    }

    #[test]
    fn unknown_element_aborts_non_recoverable() {
        let mut fp = standard_input_fp();
        fp.tag = "span".into();
        fp.has_value_property = false;

        let input = StrategyInput {
            selector: "#x",
            text: "",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        match decide(&input) {
            ExecutionPlan::Abort {
                recoverable,
                reason,
            } => {
                assert!(!recoverable);
                assert!(reason.contains("span"));
            }
            other => panic!("expected Abort, got {:?}", other),
        }
    }

    // ===== contentEditable tests (Task 8) =====

    #[test]
    fn draft_js_fill_uses_rich_text_fill_with_innermost_selector() {
        let fp = draft_js_fp();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "Hello world",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: None,
        };
        let plan = decide(&input);
        assert_eq!(
            plan,
            ExecutionPlan::RichTextFill {
                // innermost selector wins, not the caller-supplied one
                selector: "div.public-DraftEditor-content".into(),
                text: "Hello world".into(),
            }
        );
    }

    #[test]
    fn draft_js_type_uses_paste() {
        let fp = draft_js_fp();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "!!!",
            mode: InputMode::Type,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::Paste {
                selector: "div.public-DraftEditor-content".into(),
                text: "!!!".into(),
            }
        );
    }

    #[test]
    fn lexical_fill_uses_rich_text_fill() {
        let mut fp = draft_js_fp();
        fp.editor_framework = Some(EditorFramework::Lexical);
        fp.innermost_editable_selector = Some("#lex".into());

        let input = StrategyInput {
            selector: "#lex",
            text: "Lex text",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "facebook.com",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::RichTextFill {
                selector: "#lex".into(),
                text: "Lex text".into(),
            }
        );
    }

    #[test]
    fn prosemirror_fill_uses_rich_text_fill() {
        let mut fp = draft_js_fp();
        fp.editor_framework = Some(EditorFramework::ProseMirror);
        fp.innermost_editable_selector = Some(".ProseMirror".into());

        let input = StrategyInput {
            selector: ".ProseMirror",
            text: "PM text",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "bsky.app",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::RichTextFill {
                selector: ".ProseMirror".into(),
                text: "PM text".into(),
            }
        );
    }

    #[test]
    fn slate_fill_uses_rich_text_fill() {
        let mut fp = draft_js_fp();
        fp.editor_framework = Some(EditorFramework::Slate);
        fp.innermost_editable_selector = Some("#slate".into());

        let input = StrategyInput {
            selector: "#slate",
            text: "Slate text",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "discord.com",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::RichTextFill {
                selector: "#slate".into(),
                text: "Slate text".into(),
            }
        );
    }

    #[test]
    fn unknown_framework_still_uses_rich_text_fill() {
        // Bare contentEditable div with no framework detected.
        let mut fp = draft_js_fp();
        fp.editor_framework = None;
        fp.react_fiber_present = false;
        fp.innermost_editable_selector = None;

        let input = StrategyInput {
            selector: "#bareCE",
            text: "Generic",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::RichTextFill {
                // Falls back to caller-supplied selector since no
                // innermost editable was provided.
                selector: "#bareCE".into(),
                text: "Generic".into(),
            }
        );
    }

    #[test]
    fn content_editable_without_innermost_uses_caller_selector() {
        let mut fp = draft_js_fp();
        fp.innermost_editable_selector = None;

        let input = StrategyInput {
            selector: "#outer",
            text: "hi",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "example.com",
            adapter: None,
        };
        assert_eq!(
            decide(&input),
            ExecutionPlan::RichTextFill {
                selector: "#outer".into(),
                text: "hi".into(),
            }
        );
    }

    // ===== apply_recipe fall-through tests (Task 7) =====

    #[test]
    fn no_adapter_matches_existing_behavior() {
        let fp = draft_js_fp();
        let input = StrategyInput {
            selector: "#c",
            text: "Hello world",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: None,
        };
        assert!(matches!(decide(&input), ExecutionPlan::RichTextFill { .. }));
    }

    // ===== Mention flow tests (Task 8) =====

    use crate::agent::browser_input::platform_adapter::Recipe;

    /// Build a minimal Recipe whose mention config matches x.com's
    /// default, for use in strategy tests.
    fn x_com_test_recipe() -> Recipe {
        const YAML: &str = r#"
name: x_com
hostname_patterns: ["x.com"]
version: 1
compose:
  selector: '[data-testid="tweetTextarea_0"]'
submit:
  selector: '[data-testid="tweetButtonInline"]'
mention:
  trigger_char: "@"
  pattern: '@([A-Za-z0-9_]{1,15})'
  candidate_list_selector: 'div[role="listbox"]'
  candidate_list_timeout_ms: 2000
  confirm_method: "enter_key"
  pause_between_segments_ms: 150
"#;
        Recipe::from_yaml("<test>", YAML).unwrap()
    }

    #[test]
    fn text_without_mention_falls_through_to_rich_text_fill() {
        let fp = draft_js_fp();
        let recipe = x_com_test_recipe();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "Just a regular tweet",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: Some(&recipe),
        };
        assert!(matches!(decide(&input), ExecutionPlan::RichTextFill { .. }));
    }

    #[test]
    fn single_mention_produces_sequence_with_wait_and_enter() {
        let fp = draft_js_fp();
        let recipe = x_com_test_recipe();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "Hi @nevoflux good morning",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: Some(&recipe),
        };
        let plan = decide(&input);
        let actions = match plan {
            ExecutionPlan::Sequence(a) => a,
            other => panic!("expected Sequence, got {:?}", other),
        };

        // Expected sequence (roughly):
        //   Paste "Hi "           — prefix before the first mention
        //   Paste "@nevoflux"     — the mention itself
        //   WaitFor listbox
        //   SendKey "Enter"
        //   Paste " good morning" — suffix
        assert_eq!(actions.len(), 5);

        match &actions[0] {
            Action::Paste { text, .. } => assert_eq!(text, "Hi "),
            other => panic!("action 0: expected Paste, got {:?}", other),
        }
        match &actions[1] {
            Action::Paste { text, .. } => assert_eq!(text, "@nevoflux"),
            other => panic!("action 1: expected Paste, got {:?}", other),
        }
        match &actions[2] {
            Action::WaitFor {
                selector,
                timeout_ms,
            } => {
                assert_eq!(selector, "div[role=\"listbox\"]");
                assert_eq!(*timeout_ms, 2000);
            }
            other => panic!("action 2: expected WaitFor, got {:?}", other),
        }
        match &actions[3] {
            Action::SendKey { key } => assert_eq!(key, "Enter"),
            other => panic!("action 3: expected SendKey(Enter), got {:?}", other),
        }
        match &actions[4] {
            Action::Paste { text, .. } => assert_eq!(text, " good morning"),
            other => panic!("action 4: expected Paste, got {:?}", other),
        }
    }

    #[test]
    fn multiple_mentions_produces_segmented_sequence() {
        let fp = draft_js_fp();
        let recipe = x_com_test_recipe();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "@alice and @bob say hi",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: Some(&recipe),
        };
        let plan = decide(&input);
        let actions = match plan {
            ExecutionPlan::Sequence(a) => a,
            other => panic!("expected Sequence, got {:?}", other),
        };
        // Pattern for two mentions with interstitial text:
        //   Paste "@alice", WaitFor, SendKey Enter,
        //   Paste " and ",
        //   Paste "@bob",   WaitFor, SendKey Enter,
        //   Paste " say hi"
        assert_eq!(actions.len(), 8);

        // Only assert on the high-signal landmarks; full shape is
        // covered by the single-mention test.
        let pastes: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Paste { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(pastes, vec!["@alice", " and ", "@bob", " say hi"]);
        let wait_count = actions
            .iter()
            .filter(|a| matches!(a, Action::WaitFor { .. }))
            .count();
        assert_eq!(wait_count, 2);
        let enter_count = actions
            .iter()
            .filter(|a| matches!(a, Action::SendKey { key } if key == "Enter"))
            .count();
        assert_eq!(enter_count, 2);
    }

    #[test]
    fn mention_at_start_has_no_empty_prefix_paste() {
        let fp = draft_js_fp();
        let recipe = x_com_test_recipe();
        let input = StrategyInput {
            selector: "[data-testid='tweetTextarea_0']",
            text: "@nevoflux hi",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "x.com",
            adapter: Some(&recipe),
        };
        if let ExecutionPlan::Sequence(actions) = decide(&input) {
            // Should not have an empty Paste as the first action.
            for a in &actions {
                if let Action::Paste { text, .. } = a {
                    assert!(!text.is_empty(), "no empty Paste allowed: {:?}", actions);
                }
            }
            // First non-WaitFor action should be the mention.
            match &actions[0] {
                Action::Paste { text, .. } => assert_eq!(text, "@nevoflux"),
                other => panic!("expected Paste @nevoflux, got {:?}", other),
            }
        } else {
            panic!("expected Sequence");
        }
    }

    #[test]
    fn mention_uses_click_first_when_recipe_says_so() {
        let fp = draft_js_fp();
        let yaml = r##"
name: click_first_site
hostname_patterns: ["click.example"]
version: 1
compose: {selector: '#c'}
submit: {selector: '#s'}
mention:
  trigger_char: "@"
  pattern: '@([A-Za-z0-9_]+)'
  candidate_list_selector: '.list'
  candidate_list_timeout_ms: 1000
  confirm_method: "click_first"
  candidate_item_selector: '.list .option'
"##;
        let recipe = Recipe::from_yaml("<t>", yaml).unwrap();
        let input = StrategyInput {
            selector: "#c",
            text: "Hello @bob",
            mode: InputMode::Fill,
            fingerprint: &fp,
            hostname: "click.example",
            adapter: Some(&recipe),
        };
        if let ExecutionPlan::Sequence(actions) = decide(&input) {
            assert!(
                actions.iter().any(
                    |a| matches!(a, Action::Click { selector } if selector == ".list .option")
                ),
                "expected a Click action on the candidate_item_selector, got {:?}",
                actions
            );
            // Should NOT contain a SendKey Enter when click_first is set.
            assert!(!actions
                .iter()
                .any(|a| matches!(a, Action::SendKey { key } if key == "Enter")));
        } else {
            panic!("expected Sequence");
        }
    }
}
