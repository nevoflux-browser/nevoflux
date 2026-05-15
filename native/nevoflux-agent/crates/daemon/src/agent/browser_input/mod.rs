// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Browser input strategy engine.
//!
//! Exposes two LLM-facing tools:
//!
//! - `browser_input` — high-level tool that probes the target, runs
//!   the pure strategy function, executes the chosen plan via Actor
//!   methods, and verifies the result.
//! - `browser_probe` — escape-hatch tool that returns a raw
//!   `Fingerprint` so LLMs can reason without running the full
//!   pipeline.
//!
//! Platform adapter recipes (spec section 7) are NOT loaded in PR #2 —
//! the registry is always empty and all decisions fall through to the
//! generic strategy branch.

pub mod bridge;
pub mod error;
pub mod executor;
pub mod fingerprint;
pub mod plan;
pub mod platform_adapter;
pub mod strategy;
pub mod upload;
pub mod verifier;

pub use bridge::{BrowserBridge, RealBrowserBridge};
pub use error::BrowserInputError;
pub use fingerprint::{EditorFramework, Fingerprint};
pub use plan::{Action, ExecutionPlan, InputMode};
pub use platform_adapter::{
    AdapterRegistry, ComposeConfig, MentionConfig, Recipe, SubmitConfig, UploadConfig,
};
pub use strategy::{decide, StrategyInput};
pub use verifier::{verify, VerifyReport};

use nevoflux_protocol::BrowserToolAction;
use serde::Serialize;
use serde_json::json;

/// Result of a `browser_input` call.
///
/// Serialized as the LLM-facing tool result.
#[derive(Debug, Clone, Serialize)]
pub struct BrowserInputResult {
    pub success: bool,
    pub strategy_used: String,
    pub framework_detected: Option<String>,
    pub verify: Option<VerifyReport>,
    pub fingerprint: Option<Fingerprint>,
}

/// Run one full `browser_input` invocation from start to finish.
///
/// The orchestration is:
/// 1. Probe the target element to get a Fingerprint.
/// 2. Run decide() to pick an ExecutionPlan. (Platform adapter is
///    always None in PR #2.)
/// 3. Execute the plan via the bridge.
/// 4. Optionally verify the result via get_content.
/// 5. Return a BrowserInputResult with everything the LLM needs.
pub async fn run_browser_input(
    bridge: &dyn BrowserBridge,
    adapter_registry: &AdapterRegistry,
    selector: &str,
    text: &str,
    mode: InputMode,
    tab_id: Option<i64>,
    verify_enabled: bool,
) -> Result<BrowserInputResult, BrowserInputError> {
    // Step 0: resolve hostname via ListTabs so we can look up a
    // matching platform adapter recipe. Failures fall back to an
    // empty hostname, which means no adapter is selected.
    let hostname = resolve_hostname(bridge, tab_id).await;
    let adapter = adapter_registry.lookup(&hostname);

    // Step 1: probe
    let probe_response = bridge
        .call_action(
            BrowserToolAction::Probe,
            json!({ "selector": selector }),
            tab_id,
        )
        .await?;

    // probe response shape: {"result": {...Fingerprint...}} — unwrap one level
    let fp_value = probe_response
        .get("result")
        .cloned()
        .unwrap_or(probe_response);
    let fingerprint: Fingerprint =
        serde_json::from_value(fp_value.clone()).map_err(|e| BrowserInputError::ProbeFailed {
            code: -1,
            message: format!("failed to parse Fingerprint: {}", e),
        })?;

    // Step 2: decide
    let strategy_input = StrategyInput {
        selector,
        text,
        mode,
        fingerprint: &fingerprint,
        hostname: &hostname,
        adapter,
    };
    let plan = decide(&strategy_input);

    // Capture metadata for the return shape.
    let strategy_used = plan_variant_name(&plan);
    let framework_detected = fingerprint.editor_framework.map(framework_name);

    // Step 3: execute
    executor::execute_plan(plan, tab_id, bridge).await?;

    // Step 4: verify (optional)
    let verify_report = if verify_enabled {
        Some(verify(selector, text, mode, tab_id, bridge).await?)
    } else {
        None
    };

    // Step 5: report
    Ok(BrowserInputResult {
        success: verify_report.as_ref().map(|v| v.matched).unwrap_or(true),
        strategy_used,
        framework_detected,
        verify: verify_report,
        fingerprint: Some(fingerprint),
    })
}

/// Run a `browser_probe` call: probe + return Fingerprint.
pub async fn run_browser_probe(
    bridge: &dyn BrowserBridge,
    selector: &str,
    tab_id: Option<i64>,
) -> Result<Fingerprint, BrowserInputError> {
    let probe_response = bridge
        .call_action(
            BrowserToolAction::Probe,
            json!({ "selector": selector }),
            tab_id,
        )
        .await?;
    let fp_value = probe_response
        .get("result")
        .cloned()
        .unwrap_or(probe_response);
    let fingerprint: Fingerprint =
        serde_json::from_value(fp_value).map_err(|e| BrowserInputError::ProbeFailed {
            code: -1,
            message: format!("failed to parse Fingerprint: {}", e),
        })?;
    Ok(fingerprint)
}

/// Return a stable string name for each ExecutionPlan variant.
/// Used in the LLM-facing response so the model knows which path ran.
fn plan_variant_name(plan: &ExecutionPlan) -> String {
    match plan {
        ExecutionPlan::NativeFill { .. } => "native_fill".into(),
        ExecutionPlan::RichTextFill { .. } => "rich_text_fill".into(),
        ExecutionPlan::Paste { .. } => "paste".into(),
        ExecutionPlan::Sequence(_) => "sequence".into(),
        ExecutionPlan::Abort { .. } => "abort".into(),
    }
}

/// Return a stable string name for each EditorFramework variant.
fn framework_name(framework: EditorFramework) -> String {
    match framework {
        EditorFramework::DraftJs => "draft.js".into(),
        EditorFramework::Lexical => "lexical".into(),
        EditorFramework::ProseMirror => "prosemirror".into(),
        EditorFramework::Slate => "slate".into(),
        EditorFramework::CodeMirror => "codemirror".into(),
        EditorFramework::Monaco => "monaco".into(),
        EditorFramework::Quill => "quill".into(),
        EditorFramework::TinyMce => "tinymce".into(),
        EditorFramework::Unknown => "unknown".into(),
    }
}

/// Look up the hostname for the given tab (or the active tab) via
/// a `ListTabs` bridge call. Returns an empty string on any error —
/// the strategy engine then treats the page as having no adapter.
async fn resolve_hostname(bridge: &dyn BrowserBridge, tab_id: Option<i64>) -> String {
    let response = match bridge
        .call_action(BrowserToolAction::ListTabs, json!({}), None)
        .await
    {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    let tabs = response
        .get("tabs")
        .or_else(|| response.get("result").and_then(|r| r.get("tabs")))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let tab = match tab_id {
        Some(id) => tabs
            .into_iter()
            .find(|t| t.get("id").and_then(|v| v.as_i64()) == Some(id)),
        None => tabs
            .into_iter()
            .find(|t| t.get("active").and_then(|v| v.as_bool()).unwrap_or(false)),
    };

    let url = tab
        .as_ref()
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    parse_hostname(url)
}

/// Tiny hostname extractor. Accepts inputs like:
///   "https://x.com/home" → "x.com"
///   "https://mobile.x.com:443/path" → "mobile.x.com"
///   "about:blank" → ""
fn parse_hostname(url: &str) -> String {
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => return String::new(),
    };
    let end = after_scheme
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_scheme.len());
    let host_port = &after_scheme[..end];
    // Strip :port if present; browser URLs won't include IPv6 brackets.
    match host_port.rsplit_once(':') {
        Some((host, _port)) if !host.is_empty() => host.to_string(),
        _ => host_port.to_string(),
    }
}

#[cfg(test)]
mod hostname_parser_tests {
    use super::parse_hostname;

    #[test]
    fn plain_host() {
        assert_eq!(parse_hostname("https://x.com/home"), "x.com");
    }

    #[test]
    fn subdomain_with_port() {
        assert_eq!(
            parse_hostname("https://mobile.x.com:443/path"),
            "mobile.x.com"
        );
    }

    #[test]
    fn empty_on_non_url() {
        assert_eq!(parse_hostname("about:blank"), "");
    }

    #[test]
    fn http_scheme() {
        assert_eq!(parse_hostname("http://example.com"), "example.com");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::browser_input::bridge::testing::FakeBridge;
    use serde_json::Value;
    use std::sync::Mutex;

    fn empty_registry() -> AdapterRegistry {
        AdapterRegistry::new()
    }

    /// Multi-response fake that pops responses from a FIFO queue,
    /// allowing distinct answers for probe vs execute vs verify.
    struct SeqBridge {
        responses: Mutex<Vec<Result<Value, BrowserInputError>>>,
        calls: Mutex<Vec<(BrowserToolAction, Value)>>,
    }

    impl SeqBridge {
        fn new(responses: Vec<Result<Value, BrowserInputError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl BrowserBridge for SeqBridge {
        async fn call_action(
            &self,
            action: BrowserToolAction,
            params: Value,
            _tab_id: Option<i64>,
        ) -> Result<Value, BrowserInputError> {
            self.calls.lock().unwrap().push((action, params));
            let mut vec = self.responses.lock().unwrap();
            if vec.is_empty() {
                return Err(BrowserInputError::Bridge("no more canned responses".into()));
            }
            vec.remove(0)
        }
    }

    fn standard_input_probe_value() -> Value {
        json!({
            "result": {
                "tag": "input",
                "input_type": "text",
                "has_value_property": true,
                "is_content_editable": false,
                "disabled": false,
                "readonly": false,
                "is_visible": true,
                "is_focusable": true,
                "editor_framework": null,
                "react_fiber_present": false,
                "inside_iframe": false,
                "shadow_root_depth": 0,
                "innermost_editable_selector": null,
                "computed_role": null
            }
        })
    }

    fn draft_js_probe_value() -> Value {
        json!({
            "result": {
                "tag": "div",
                "input_type": null,
                "has_value_property": false,
                "is_content_editable": true,
                "disabled": false,
                "readonly": false,
                "is_visible": true,
                "is_focusable": true,
                "editor_framework": "draft.js",
                "react_fiber_present": true,
                "inside_iframe": false,
                "shadow_root_depth": 0,
                "innermost_editable_selector": "div.public-DraftEditor-content",
                "computed_role": "textbox"
            }
        })
    }

    /// Canned response for the ListTabs call that `run_browser_input`
    /// now makes before probing.
    fn empty_tabs_response() -> Value {
        json!({ "tabs": [] })
    }

    #[tokio::test]
    async fn run_browser_input_standard_input_fill_path() {
        let bridge = SeqBridge::new(vec![
            Ok(empty_tabs_response()),        // hostname resolve
            Ok(standard_input_probe_value()), // probe
            Ok(json!({"success": true})),     // execute (Fill)
            Ok(json!({"text": "Hello"})),     // verify (GetContent)
        ]);

        let registry = empty_registry();
        let result = run_browser_input(
            &bridge,
            &registry,
            "#tgt",
            "Hello",
            InputMode::Fill,
            Some(1),
            /* verify_enabled */ true,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.strategy_used, "native_fill");
        assert_eq!(result.framework_detected, None);
        assert!(result.verify.is_some());
        assert!(result.verify.as_ref().unwrap().matched);

        // 4 bridge calls expected: ListTabs, Probe, Fill, GetContent
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(matches!(calls[0].0, BrowserToolAction::ListTabs));
        assert!(matches!(calls[1].0, BrowserToolAction::Probe));
        assert!(matches!(calls[2].0, BrowserToolAction::Fill));
        assert!(matches!(calls[3].0, BrowserToolAction::GetContent));
    }

    #[tokio::test]
    async fn run_browser_input_draft_js_fill_path() {
        let bridge = SeqBridge::new(vec![
            Ok(empty_tabs_response()),          // hostname resolve
            Ok(draft_js_probe_value()),         // probe
            Ok(json!({"success": true})),       // execute (FillRichText)
            Ok(json!({"text": "Hello Draft"})), // verify
        ]);

        let registry = empty_registry();
        let result = run_browser_input(
            &bridge,
            &registry,
            "[data-testid='tweetTextarea_0']",
            "Hello Draft",
            InputMode::Fill,
            None,
            true,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.strategy_used, "rich_text_fill");
        assert_eq!(result.framework_detected.as_deref(), Some("draft.js"));

        let calls = bridge.calls.lock().unwrap();
        assert!(matches!(calls[2].0, BrowserToolAction::FillRichText));
        // The innermost selector should have been used, not the caller one
        assert_eq!(
            calls[2].1["selector"],
            serde_json::Value::String("div.public-DraftEditor-content".into())
        );
    }

    #[tokio::test]
    async fn run_browser_input_verify_disabled_skips_get_content() {
        let bridge = SeqBridge::new(vec![
            Ok(empty_tabs_response()),
            Ok(standard_input_probe_value()),
            Ok(json!({"success": true})),
        ]);

        let registry = empty_registry();
        let result = run_browser_input(
            &bridge,
            &registry,
            "#tgt",
            "Hello",
            InputMode::Fill,
            Some(1),
            false,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert!(result.verify.is_none());

        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 3); // ListTabs + probe + Fill only
    }

    #[tokio::test]
    async fn run_browser_input_probe_failure_surfaces_error() {
        let bridge = SeqBridge::new(vec![
            Ok(empty_tabs_response()),
            Err(BrowserInputError::ElementNotFound {
                selector: "#missing".into(),
            }),
        ]);

        let registry = empty_registry();
        let err = run_browser_input(
            &bridge,
            &registry,
            "#missing",
            "Hello",
            InputMode::Fill,
            None,
            true,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BrowserInputError::ElementNotFound { .. }));
    }

    #[tokio::test]
    async fn run_browser_probe_returns_fingerprint() {
        let bridge = FakeBridge::with_response(Ok(draft_js_probe_value()));
        let fp = run_browser_probe(&bridge, "#x", None).await.unwrap();
        assert!(fp.is_content_editable);
        assert_eq!(fp.editor_framework, Some(EditorFramework::DraftJs));
    }

    // ===== Task 12: end-to-end integration through the bridge =====

    #[tokio::test]
    async fn run_browser_input_x_com_mention_flow() {
        // Build a registry from the compiled-in recipe only.
        let registry = AdapterRegistry::load_standard(None, None);
        assert!(
            registry.lookup("x.com").is_some(),
            "x_com recipe must be loaded"
        );

        // Canned responses, in call order:
        //   1. ListTabs        → active tab on x.com
        //   2. Probe           → Draft.js fingerprint
        //   3. Paste "Hello "  (prefix)
        //   4. Paste "@nevoflux"
        //   5. WaitFor listbox
        //   6. KeyPress Enter
        //   7. Paste " welcome" (suffix)
        //   8. GetContent      → verify
        let bridge = SeqBridge::new(vec![
            Ok(json!({
                "tabs": [{ "id": 1, "url": "https://x.com/home", "active": true }]
            })),
            Ok(draft_js_probe_value()),
            Ok(json!({"success": true})),
            Ok(json!({"success": true})),
            Ok(json!({"success": true})),
            Ok(json!({"success": true})),
            Ok(json!({"success": true})),
            Ok(json!({"text": "Hello @nevoflux welcome"})),
        ]);

        let result = run_browser_input(
            &bridge,
            &registry,
            "[data-testid='tweetTextarea_0']",
            "Hello @nevoflux welcome",
            InputMode::Fill,
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(result.strategy_used, "sequence");
        assert_eq!(result.framework_detected.as_deref(), Some("draft.js"));

        let calls = bridge.calls.lock().unwrap();
        let action_names: Vec<_> = calls.iter().map(|(a, _)| format!("{:?}", a)).collect();
        // Expected: ListTabs, Probe, Paste, Paste, WaitFor, KeyPress, Paste, GetContent
        assert_eq!(
            calls.len(),
            8,
            "unexpected call sequence: {:?}",
            action_names
        );
        assert!(matches!(calls[0].0, BrowserToolAction::ListTabs));
        assert!(matches!(calls[1].0, BrowserToolAction::Probe));
        assert!(matches!(calls[2].0, BrowserToolAction::Paste));
        assert!(matches!(calls[3].0, BrowserToolAction::Paste));
        assert!(matches!(calls[4].0, BrowserToolAction::WaitFor));
        assert!(matches!(calls[5].0, BrowserToolAction::KeyPress));
        assert!(matches!(calls[6].0, BrowserToolAction::Paste));
        assert!(matches!(calls[7].0, BrowserToolAction::GetContent));

        // Each Paste targets the innermost editable selector from the fp.
        for i in [2usize, 3, 6] {
            assert_eq!(
                calls[i].1["selector"],
                serde_json::Value::String("div.public-DraftEditor-content".into())
            );
        }
    }

    #[tokio::test]
    async fn run_browser_input_no_mention_uses_rich_text_fill_on_x_com() {
        let registry = AdapterRegistry::load_standard(None, None);
        let bridge = SeqBridge::new(vec![
            Ok(json!({
                "tabs": [{ "id": 1, "url": "https://x.com/home", "active": true }]
            })),
            Ok(draft_js_probe_value()),
            Ok(json!({"success": true})),
            Ok(json!({"text": "Just a regular tweet"})),
        ]);

        let result = run_browser_input(
            &bridge,
            &registry,
            "[data-testid='tweetTextarea_0']",
            "Just a regular tweet",
            InputMode::Fill,
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(result.strategy_used, "rich_text_fill");
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 4); // ListTabs, Probe, FillRichText, GetContent
        assert!(matches!(calls[2].0, BrowserToolAction::FillRichText));
    }
}
