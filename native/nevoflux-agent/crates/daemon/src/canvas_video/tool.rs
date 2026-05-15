//! Agent-tool bridges for canvas.video.*.
//!
//! Wraps `CanvasVideoService` so the agent runner can invoke
//! `canvas_create_composition` and `canvas_render_video` through the
//! standard `ToolExecutor` / `ToolRegistry` surface.
//!
//! Production wiring in `agent/runner.rs` is deferred — those call sites
//! currently construct `ToolRegistry::new()` without access to
//! `CanvasVideoService`. A follow-up will thread the service through.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::tools::ToolExecutor;
use crate::canvas_video::CanvasVideoService;
use crate::error::{DaemonError, Result};
use nevoflux_protocol::canvas_video::{
    CreateCompositionRequest, LintCompositionRequest, RenderStartRequest,
};

/// `canvas_create_composition` agent tool.
pub struct CanvasCreateCompositionTool {
    svc: Arc<CanvasVideoService>,
}

impl CanvasCreateCompositionTool {
    pub fn new(svc: Arc<CanvasVideoService>) -> Self {
        Self { svc }
    }
}

/// Shared parser used at every LLM dispatch surface.
///
/// `CreateCompositionRequest` accepts both `template` and `html`; the
/// either-or contract (at least one must be present) is enforced downstream
/// in `canvas_video::create::resolve_index_html`. `#[serde(deny_unknown_fields)]`
/// on the request struct catches typo'd fields. The wrapper exists as a
/// single funnel point that all three dispatch surfaces (this tool,
/// mcp_tool_executor, agent_host) route through, so future cross-cutting
/// validation has one place to live.
pub fn parse_create_composition_args_strict(arguments: &Value) -> Result<CreateCompositionRequest> {
    serde_json::from_value(arguments.clone())
        .map_err(|e| DaemonError::InvalidRequest(format!("canvas_create_composition: {}", e)))
}

#[async_trait]
impl ToolExecutor for CanvasCreateCompositionTool {
    async fn execute(&self, _name: &str, arguments: &Value) -> Result<String> {
        let req = parse_create_composition_args_strict(arguments)?;
        let resp = self.svc.create_composition(req).await?;
        serde_json::to_string(&resp)
            .map_err(|e| DaemonError::InternalError(format!("serialize response: {}", e)))
    }
}

/// `canvas_render_video` agent tool.
pub struct CanvasRenderVideoTool {
    svc: Arc<CanvasVideoService>,
}

impl CanvasRenderVideoTool {
    pub fn new(svc: Arc<CanvasVideoService>) -> Self {
        Self { svc }
    }
}

#[async_trait]
impl ToolExecutor for CanvasRenderVideoTool {
    async fn execute(&self, _name: &str, arguments: &Value) -> Result<String> {
        let req: RenderStartRequest = serde_json::from_value(arguments.clone())
            .map_err(|e| DaemonError::InvalidRequest(format!("canvas_render_video: {}", e)))?;
        let resp = self.svc.render_start(req).await?;
        serde_json::to_string(&resp)
            .map_err(|e| DaemonError::InternalError(format!("serialize response: {}", e)))
    }
}

/// JSON Schema for `canvas_create_composition`. Exposed so the WASM agent
/// `ToolDefinition` list can reuse the exact same shape — keeps the dual
/// tool registry in sync without two sources of truth.
///
/// Either `template` (one of seven shipped /video skill templates) or `html`
/// (raw composition body) must be supplied. The downstream service prefers
/// `html` when both are given. JSON Schema can't reliably express "exactly
/// one of these two" across all LLM providers (anyOf/oneOf support is
/// uneven), so the constraint is enforced in
/// `canvas_video::create::resolve_index_html` and surfaced via the tool
/// description.
///
/// The optional `design_md` argument supplies the brand-identity layer
/// (Google design.md + NevoFlux video extension YAML frontmatter); the
/// daemon parses it and injects a `<style data-nf-design-tokens>` block
/// into the composition's `<head>`. When absent, the daemon falls back to
/// the template-specific `templates/<name>.design.md` default, then to the
/// generic `reference/DESIGN-template.md`.
pub fn create_composition_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "title":        { "type": "string" },
            "width":        { "type": "integer", "minimum": 1, "maximum": 1920 },
            "height":       { "type": "integer", "minimum": 1, "maximum": 1920 },
            "duration_sec": { "type": "number",  "minimum": 0.5, "maximum": 60 },
            "fps":          { "type": "integer", "enum": [24, 25, 30] },
            "bg":           { "type": ["string", "null"] },
            "template":     {
                "type": "string",
                "enum": [
                    "website-promo-16x9",
                    "product-intro-16x9",
                    "product-intro-9x16",
                    "tiktok-hook",
                    "video-overlay",
                    "logo-3d-reveal",
                    "product-3d-spin"
                ],
                "description": "Skill template name from the /video skill. The daemon \
                                materializes the named template into the composition; you do \
                                NOT need to call skill_read first. Default path — prefer this \
                                whenever a shipped template fits the request."
            },
            "html": {
                "type": "string",
                "description": "Raw composition HTML body. Use ONLY when the user explicitly \
                                wants a custom layout no template covers (or supplies their own \
                                HTML). When provided alongside `template`, `html` wins."
            },
            "design_md": {
                "type": "string",
                "description": "Brand identity (Google design.md + NevoFlux video extension \
                                YAML frontmatter). Drives colors / typography / spacing / \
                                motion via daemon-injected CSS variables. Omit to use the \
                                template's own default brand identity."
            }
        },
        "required": ["title", "width", "height", "duration_sec", "fps"]
    })
}

/// JSON Schema for `canvas_render_video`.
pub fn render_video_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "composition_id": { "type": "string" }
        },
        "required": ["composition_id"]
    })
}

/// `canvas_lint_composition` agent tool.
///
/// Delegates to `CanvasVideoService::lint_composition`, which publishes a
/// lint request on the EventBus and awaits a `LintReport` from the extension.
pub struct CanvasLintCompositionTool {
    svc: Arc<CanvasVideoService>,
}

impl CanvasLintCompositionTool {
    pub fn new(svc: Arc<CanvasVideoService>) -> Self {
        Self { svc }
    }
}

#[async_trait]
impl ToolExecutor for CanvasLintCompositionTool {
    async fn execute(&self, _name: &str, arguments: &Value) -> Result<String> {
        let req: LintCompositionRequest = serde_json::from_value(arguments.clone())
            .map_err(|e| DaemonError::InvalidRequest(format!("canvas_lint_composition: {e}")))?;
        let report = self.svc.lint_composition(&req.composition_id).await?;
        serde_json::to_string(&report)
            .map_err(|e| DaemonError::InternalError(format!("serialize lint report: {e}")))
    }
}

/// JSON Schema for `canvas_lint_composition`.
pub fn lint_composition_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "composition_id": { "type": "string" }
        },
        "required": ["composition_id"]
    })
}

/// Register all three canvas.video.* tools on an existing `ToolRegistry`.
///
/// Call sites inject the shared `CanvasVideoService` so all tools and
/// bridge handlers see the same job registry / composition store.
pub fn register(registry: &mut crate::agent::tools::ToolRegistry, svc: Arc<CanvasVideoService>) {
    registry.register(
        "canvas_create_composition",
        Box::new(CanvasCreateCompositionTool::new(svc.clone())),
    );
    registry.register(
        "canvas_render_video",
        Box::new(CanvasRenderVideoTool::new(svc.clone())),
    );
    registry.register(
        "canvas_lint_composition",
        Box::new(CanvasLintCompositionTool::new(svc)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::ToolRegistry;

    #[tokio::test]
    async fn test_canvas_create_composition_tool_dispatches_to_service() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let tool = CanvasCreateCompositionTool::new(svc);
        // The dispatch boundary now rejects `html`, and the test SkillRegistry
        // is empty (no template files loaded), so a template payload reaches
        // the service layer and surfaces SkillAssetNotFound. That proves the
        // dispatch path: deserialize -> service::create_composition ->
        // resolve_index_html template branch.
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "template": "tiktok-hook"
        });
        let err = tool
            .execute("canvas_create_composition", &args)
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("skill asset not found") || msg.contains("SkillAssetNotFound"),
            "expected service-layer SkillAssetNotFound, got: {msg}"
        );
    }

    #[test]
    fn test_parser_accepts_html_only_payload() {
        // html alone (no template) must parse cleanly — agents that provide a
        // raw composition body should not be rejected at the dispatch boundary.
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "html": "<html><body>custom</body></html>"
        });
        let req = parse_create_composition_args_strict(&args).expect("html-only payload");
        assert_eq!(
            req.html.as_deref(),
            Some("<html><body>custom</body></html>")
        );
        assert!(req.template.is_none());
    }

    #[test]
    fn test_parser_accepts_template_only_payload() {
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "template": "tiktok-hook"
        });
        let req = parse_create_composition_args_strict(&args).expect("template-only payload");
        assert_eq!(req.template.as_deref(), Some("tiktok-hook"));
        assert!(req.html.is_none());
    }

    #[test]
    fn test_parser_accepts_both_template_and_html() {
        // Service-level precedence (html wins) is tested elsewhere; the
        // parser's job is just not to reject the combo.
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "template": "tiktok-hook",
            "html": "<html><body>override</body></html>"
        });
        let req = parse_create_composition_args_strict(&args).expect("both-fields payload");
        assert_eq!(req.template.as_deref(), Some("tiktok-hook"));
        assert_eq!(
            req.html.as_deref(),
            Some("<html><body>override</body></html>")
        );
    }

    #[test]
    fn test_parser_accepts_design_md_with_template() {
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "template": "tiktok-hook",
            "design_md": "---\nname: \"my-brand\"\ncolors:\n  primary: \"#ff6600\"\n---\n",
        });
        let req = parse_create_composition_args_strict(&args).expect("template+design_md");
        assert_eq!(req.template.as_deref(), Some("tiktok-hook"));
        assert!(req.design_md.is_some());
        assert!(req.design_md.unwrap().contains("#ff6600"));
    }

    #[test]
    fn test_parser_accepts_design_md_with_html() {
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "html": "<html><body>x</body></html>",
            "design_md": "---\nname: \"my-brand\"\n---\n",
        });
        let req = parse_create_composition_args_strict(&args).expect("html+design_md");
        assert!(req.html.is_some());
        assert!(req.design_md.is_some());
    }

    #[test]
    fn test_parser_rejects_unknown_field() {
        // deny_unknown_fields on the request struct must still catch typos.
        let args = serde_json::json!({
            "title": "demo",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 30,
            "template": "tiktok-hook",
            "templat": "tiktok-hook"  // typo
        });
        let err = parse_create_composition_args_strict(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown field") || msg.contains("templat"),
            "expected unknown-field error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_canvas_create_composition_tool_surfaces_validation_errors() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let tool = CanvasCreateCompositionTool::new(svc);
        let args = serde_json::json!({
            "title": "bad",
            "width": 640,
            "height": 360,
            "duration_sec": 1.0,
            "fps": 60
        });
        let err = tool
            .execute("canvas_create_composition", &args)
            .await
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("fps") || msg.contains("60"),
            "unexpected: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_register_adds_all_three_tools() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let mut registry = ToolRegistry::empty();
        register(&mut registry, svc);
        assert!(registry.has_tool("canvas_create_composition"));
        assert!(registry.has_tool("canvas_render_video"));
        assert!(registry.has_tool("canvas_lint_composition"));
    }

    #[test]
    fn test_schemas_match_hard_limits() {
        let s = create_composition_schema();
        let props = &s["properties"];
        assert_eq!(props["width"]["maximum"], 1920);
        assert_eq!(props["height"]["maximum"], 1920);
        assert_eq!(props["duration_sec"]["maximum"], 60);
        let fps_enum = props["fps"]["enum"].as_array().unwrap();
        assert_eq!(fps_enum.len(), 3);

        // template is constrained to the seven shipped names but is no longer
        // strictly required — html is the alternate.
        assert!(props.get("template").is_some(), "template field missing");
        let tpl_enum = props["template"]["enum"].as_array().unwrap();
        assert_eq!(tpl_enum.len(), 7);

        // html is exposed again so agents can supply a raw composition body.
        assert!(
            props.get("html").is_some(),
            "html field must be exposed to LLM (either-or with template)"
        );

        // design_md exposes the brand-identity input channel.
        assert!(
            props.get("design_md").is_some(),
            "design_md field must be exposed to LLM"
        );

        // Either-or contract is enforced at the service layer, so neither
        // template, html, nor design_md should appear in `required`.
        let required = s["required"].as_array().unwrap();
        assert!(
            !required.iter().any(|v| v == "template"),
            "template must NOT be required (either-or with html), got: {required:?}"
        );
        assert!(
            !required.iter().any(|v| v == "html"),
            "html must NOT be required (either-or with template), got: {required:?}"
        );
        assert!(
            !required.iter().any(|v| v == "design_md"),
            "design_md must NOT be required (optional brand layer), got: {required:?}"
        );
    }

    #[tokio::test]
    async fn test_lint_composition_schema_shape() {
        let s = lint_composition_schema();
        let props = &s["properties"];
        assert!(props.get("composition_id").is_some());
        assert_eq!(s["required"][0], "composition_id");
    }

    #[tokio::test]
    async fn test_lint_composition_tool_returns_report_for_resolved_correlator() {
        use nevoflux_protocol::canvas_video::{CreateCompositionRequest, LintReport};
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        // Seed a composition so the tool has something to look up.
        let resp = svc
            .create_composition(CreateCompositionRequest {
                title: "t".into(),
                width: 640,
                height: 360,
                duration_sec: 5.0,
                fps: 30,
                bg: None,
                html: Some("<html><body></body></html>".into()),
                template: None,
                design_md: None,
                session_id: None,
            })
            .await
            .unwrap();
        // Spawn a task that mimics the extension: whenever the service has a
        // pending correlator, resolve it with an empty report.
        let svc_c = svc.clone();
        tokio::spawn(async move {
            for _ in 0..50 {
                if let Some(c) = svc_c.peek_pending_lint_correlator().await {
                    svc_c.on_lint_result(&c, LintReport::default()).await;
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        });
        let tool = CanvasLintCompositionTool::new(svc.clone());
        let args = serde_json::json!({ "composition_id": resp.artifact_id });
        let out = tool
            .execute("canvas_lint_composition", &args)
            .await
            .unwrap();
        assert!(out.contains("\"errors\""), "got: {out}");
    }

    #[tokio::test]
    #[ignore]
    async fn test_lint_composition_tool_times_out_when_no_resolver() {
        use nevoflux_protocol::canvas_video::CreateCompositionRequest;
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let resp = svc
            .create_composition(CreateCompositionRequest {
                title: "t".into(),
                width: 640,
                height: 360,
                duration_sec: 5.0,
                fps: 30,
                bg: None,
                html: Some("<html><body></body></html>".into()),
                template: None,
                design_md: None,
                session_id: None,
            })
            .await
            .unwrap();
        let tool = CanvasLintCompositionTool::new(svc.clone());
        let args = serde_json::json!({ "composition_id": resp.artifact_id });
        let err = tool
            .execute("canvas_lint_composition", &args)
            .await
            .unwrap_err();
        assert!(
            format!("{err}").to_lowercase().contains("timeout"),
            "got: {err}"
        );
    }
}
