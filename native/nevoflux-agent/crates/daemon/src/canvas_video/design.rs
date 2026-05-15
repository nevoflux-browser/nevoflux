//! DESIGN.md parsing + CSS variable token injection.
//!
//! Three-layer composition contract:
//! - template = visual structure (layout, animations, scene rhythm)
//! - DESIGN.md = brand identity tokens (colors, typography, spacing, motion)
//! - LLM-edited index.html = content (copy, headlines, CTAs)
//!
//! This module bridges DESIGN.md (Google design.md format + NevoFlux video
//! extension) into a `<style data-nf-design-tokens>:root { ... }</style>`
//! block injected at the top of `<head>`. Templates use
//! `var(--color-primary, #fallback)` patterns; CSS cascade resolves the
//! placeholder to whatever the injected block specifies.
//!
//! `inject_design_tokens` is idempotent — running it twice produces the same
//! result as running it once. The marked block is identifiable by its
//! `data-nf-design-tokens` attribute, allowing non-destructive re-application
//! after DESIGN.md edits without disturbing LLM-authored content.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{DaemonError, Result};

/// Parsed YAML frontmatter of a DESIGN.md file.
///
/// Mirrors the shape used by `crates/daemon/tests/design_template_parse.rs`,
/// extended with an `Eq`-friendly representation suitable for production use.
/// All collection fields are `#[serde(default)]` so partial DESIGN.md files
/// (e.g., colors only) parse without error and unmentioned tokens simply
/// fall through to the template's own `var(--x, fallback)` defaults.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct DesignFrontmatter {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub colors: HashMap<String, String>,
    #[serde(default)]
    pub typography: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub spacing: HashMap<String, String>,
    #[serde(default)]
    pub rounded: HashMap<String, String>,
    #[serde(default)]
    pub components: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub motion: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub voice: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub aspect: Option<HashMap<String, serde_yaml::Value>>,
}

/// Extract the YAML frontmatter slice (between leading `---` and closing `---`).
/// Returns `None` for non-frontmatter documents; tolerates BOM and CRLF.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let s = content.strip_prefix('\u{feff}').unwrap_or(content);
    let after_open = s
        .strip_prefix("---\n")
        .or_else(|| s.strip_prefix("---\r\n"))?;
    for sep in ["\n---\n", "\r\n---\r\n", "\n---\r\n", "\r\n---\n"] {
        if let Some(end) = after_open.find(sep) {
            return Some(&after_open[..end]);
        }
    }
    if let Some(stripped) = after_open.strip_suffix("\n---") {
        return Some(stripped);
    }
    if let Some(stripped) = after_open.strip_suffix("\r\n---") {
        return Some(stripped);
    }
    None
}

/// Parse a DESIGN.md document's frontmatter into a strongly-typed struct.
///
/// Documents without frontmatter or with empty frontmatter return
/// `DesignFrontmatter::default()`. Malformed YAML returns
/// `DaemonError::InvalidRequest`.
pub fn parse_design_md(content: &str) -> Result<DesignFrontmatter> {
    let yaml = extract_frontmatter(content).unwrap_or("");
    if yaml.trim().is_empty() {
        return Ok(DesignFrontmatter::default());
    }
    serde_yaml::from_str(yaml)
        .map_err(|e| DaemonError::InvalidRequest(format!("DESIGN.md frontmatter parse error: {e}")))
}

fn yaml_value_as_css(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Render the CSS variables block from a parsed DESIGN.md frontmatter.
///
/// Returns an empty string if no recognized tokens are present (caller can
/// then skip injection entirely). Otherwise produces a complete
/// `<style data-nf-design-tokens>:root { ... }</style>` block ready to
/// insert into `<head>`.
pub fn render_design_tokens_block(fm: &DesignFrontmatter) -> String {
    let mut lines: Vec<String> = Vec::new();

    // colors → --color-*
    for (key, css_var) in [
        ("primary", "--color-primary"),
        ("secondary", "--color-secondary"),
        ("accent", "--color-accent"),
        ("background", "--color-background"),
        ("foreground", "--color-foreground"),
    ] {
        if let Some(val) = fm.colors.get(key) {
            lines.push(format!("  {css_var}: {val};"));
        }
    }

    // typography.hero/body.{family,weight} → --typography-{group}-{prop}
    for (group, css_family, css_weight) in [
        (
            "hero",
            "--typography-hero-family",
            "--typography-hero-weight",
        ),
        (
            "body",
            "--typography-body-family",
            "--typography-body-weight",
        ),
    ] {
        if let Some(typo_val) = fm.typography.get(group) {
            if let Some(map) = typo_val.as_mapping() {
                if let Some(family) = map
                    .get(serde_yaml::Value::String("family".into()))
                    .and_then(|v| v.as_str())
                {
                    lines.push(format!("  {css_family}: {family};"));
                }
                if let Some(weight) = map
                    .get(serde_yaml::Value::String("weight".into()))
                    .and_then(yaml_value_as_css)
                {
                    lines.push(format!("  {css_weight}: {weight};"));
                }
            }
        }
    }

    // spacing.{xs..xl} → --spacing-*
    for key in ["xs", "sm", "md", "lg", "xl"] {
        if let Some(val) = fm.spacing.get(key) {
            lines.push(format!("  --spacing-{key}: {val};"));
        }
    }

    // rounded.{sm,md,lg} → --rounded-*
    for key in ["sm", "md", "lg"] {
        if let Some(val) = fm.rounded.get(key) {
            lines.push(format!("  --rounded-{key}: {val};"));
        }
    }

    // motion.* → --motion-*
    if let Some(motion) = &fm.motion {
        for key in [
            "ease_default",
            "ease_entrance",
            "ease_exit",
            "scene_duration_default",
            "stagger_default",
            "beat_interval",
        ] {
            if let Some(val) = motion.get(key).and_then(yaml_value_as_css) {
                lines.push(format!("  --motion-{key}: {val};"));
            }
        }
    }

    // aspect.safe_zones.{top,bottom,sides} → --aspect-safe_*
    if let Some(aspect) = &fm.aspect {
        if let Some(sz) = aspect.get("safe_zones").and_then(|v| v.as_mapping()) {
            for (key, css_var) in [
                ("top", "--aspect-safe_top"),
                ("bottom", "--aspect-safe_bottom"),
                ("sides", "--aspect-safe_sides"),
            ] {
                if let Some(val) = sz
                    .get(serde_yaml::Value::String(key.into()))
                    .and_then(|v| v.as_str())
                {
                    lines.push(format!("  {css_var}: {val};"));
                }
            }
        }
    }

    if lines.is_empty() {
        return String::new();
    }

    let body = lines.join("\n");
    format!("<style data-nf-design-tokens>\n:root {{\n{body}\n}}\n</style>")
}

/// Inject DESIGN.md-derived CSS variables into composition HTML's `<head>`.
///
/// Strategy:
/// 1. Parse `design_md` frontmatter.
/// 2. Render the `<style data-nf-design-tokens>` block.
/// 3. If `html` already contains such a block (any attributes), replace it
///    wholesale — preserves all other content byte-for-byte.
/// 4. Otherwise, insert the block right after the `<head>` opening tag.
/// 5. If `<head>` is missing but `<html>` exists, wrap a minimal head.
/// 6. As a last resort (bare fragment), prepend `<head>...</head>`.
///
/// Idempotent: `inject(inject(html, md), md) == inject(html, md)` for any
/// valid DESIGN.md.
pub fn inject_design_tokens(html: &str, design_md: &str) -> Result<String> {
    let fm = parse_design_md(design_md)?;
    let block = render_design_tokens_block(&fm);
    if block.is_empty() {
        return Ok(html.to_string());
    }
    // Replace existing block if present.
    let block_re = regex::Regex::new(r#"<style\s+data-nf-design-tokens[^>]*>[\s\S]*?</style>"#)
        .expect("static regex compiles");
    if block_re.is_match(html) {
        return Ok(block_re.replace(html, block.as_str()).into_owned());
    }
    // Insert after <head> opening tag.
    //
    // The `\b` word-boundary is critical: without it, `<head[^>]*>` greedily
    // matches `<HEADLINE_LINE_1>` inside template HTML comments (because
    // `<HEAD` + `LINE_1` + `>` satisfies the pattern). That caused tokens
    // to be injected mid-comment instead of after the real `<head>`,
    // shattering the template's leading `<!-- ... -->` block and leaving
    // the actual `<head>` un-injected — observed when `tiktok-hook.html`
    // creates produced an MP4 with only scene-3's bg color (the only one
    // not affected by the broken cascade).
    let head_re = regex::Regex::new(r#"(?i)<head\b[^>]*>"#).expect("static regex compiles");
    if let Some(m) = head_re.find(html) {
        let insert_at = m.end();
        let mut out = String::with_capacity(html.len() + block.len() + 2);
        out.push_str(&html[..insert_at]);
        out.push('\n');
        out.push_str(&block);
        out.push_str(&html[insert_at..]);
        return Ok(out);
    }
    // No <head>: wrap minimally if <html> exists. Same `\b` rationale.
    let html_re = regex::Regex::new(r#"(?i)<html\b[^>]*>"#).expect("static regex compiles");
    if let Some(m) = html_re.find(html) {
        let insert_at = m.end();
        let mut out = String::with_capacity(html.len() + block.len() + 16);
        out.push_str(&html[..insert_at]);
        out.push_str("<head>\n");
        out.push_str(&block);
        out.push_str("\n</head>");
        out.push_str(&html[insert_at..]);
        return Ok(out);
    }
    // Bare fragment.
    Ok(format!("<head>\n{block}\n</head>\n{html}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FM: &str = r##"---
name: "test-brand"
colors:
  primary: "#ff6600"
  secondary: "#cc4400"
  background: "#000"
  foreground: "#fff"
typography:
  hero:
    family: "Inter, sans-serif"
    weight: 800
spacing:
  lg: "24px"
motion:
  ease_entrance: "power4.out"
---

## Overview
test
"##;

    #[test]
    fn parse_extracts_frontmatter_fields() {
        let fm = parse_design_md(SAMPLE_FM).expect("parse ok");
        assert_eq!(fm.name, "test-brand");
        assert_eq!(fm.colors.get("primary").unwrap(), "#ff6600");
        assert_eq!(fm.colors.get("foreground").unwrap(), "#fff");
        assert_eq!(fm.spacing.get("lg").unwrap(), "24px");
        let motion = fm.motion.unwrap();
        assert_eq!(
            motion.get("ease_entrance").unwrap().as_str().unwrap(),
            "power4.out"
        );
    }

    #[test]
    fn parse_empty_returns_default() {
        let fm = parse_design_md("").expect("empty ok");
        assert!(fm.name.is_empty());
        assert!(fm.colors.is_empty());
    }

    #[test]
    fn parse_no_frontmatter_returns_default() {
        let fm = parse_design_md("# Just markdown\n\nnothing yaml here\n").expect("ok");
        assert!(fm.colors.is_empty());
    }

    #[test]
    fn parse_malformed_yaml_errors() {
        // Invalid YAML inside frontmatter
        let bad = "---\nfoo: : bar\n---\n";
        let result = parse_design_md(bad);
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    #[test]
    fn render_block_includes_color_and_typography_tokens() {
        let fm = parse_design_md(SAMPLE_FM).expect("parse");
        let block = render_design_tokens_block(&fm);
        assert!(block.contains("data-nf-design-tokens"));
        assert!(block.contains("--color-primary: #ff6600;"));
        assert!(block.contains("--color-foreground: #fff;"));
        assert!(block.contains("--typography-hero-family: Inter, sans-serif;"));
        assert!(block.contains("--typography-hero-weight: 800;"));
        assert!(block.contains("--spacing-lg: 24px;"));
        assert!(block.contains("--motion-ease_entrance: power4.out;"));
    }

    #[test]
    fn render_block_skips_missing_fields() {
        let minimal = "---\ncolors:\n  primary: \"#abc\"\n---\n";
        let fm = parse_design_md(minimal).expect("parse");
        let block = render_design_tokens_block(&fm);
        assert!(block.contains("--color-primary: #abc;"));
        assert!(!block.contains("--color-secondary"));
        assert!(!block.contains("--typography-hero"));
    }

    #[test]
    fn render_block_empty_when_no_tokens() {
        let fm = parse_design_md("---\nname: \"empty\"\n---\n").expect("parse");
        assert!(render_design_tokens_block(&fm).is_empty());
    }

    #[test]
    fn inject_inserts_block_after_head_when_absent() {
        let html = "<!DOCTYPE html><html><head><title>X</title></head><body>Y</body></html>";
        let out = inject_design_tokens(html, SAMPLE_FM).expect("inject");
        assert!(out.contains("data-nf-design-tokens"));
        // block should be inside head, before title
        let head_idx = out.find("<head>").unwrap();
        let block_idx = out.find("data-nf-design-tokens").unwrap();
        let title_idx = out.find("<title>").unwrap();
        assert!(head_idx < block_idx);
        assert!(block_idx < title_idx);
    }

    #[test]
    fn inject_replaces_existing_block_idempotently() {
        let html = "<!DOCTYPE html><html><head><title>X</title></head><body>Y</body></html>";
        let once = inject_design_tokens(html, SAMPLE_FM).expect("inject 1");
        let twice = inject_design_tokens(&once, SAMPLE_FM).expect("inject 2");
        assert_eq!(once, twice, "idempotency violated");
    }

    #[test]
    fn inject_only_changes_marked_block() {
        let html = "<!DOCTYPE html><html><head><title>Original Title</title></head><body><p>Body content here</p></body></html>";
        let injected = inject_design_tokens(html, SAMPLE_FM).expect("inject");
        // Title and body content must survive verbatim.
        assert!(injected.contains("<title>Original Title</title>"));
        assert!(injected.contains("<p>Body content here</p>"));
        // Now edit DESIGN.md to a different primary, re-inject.
        let altered_md = SAMPLE_FM.replace("#ff6600", "#00ff00");
        let reinjected = inject_design_tokens(&injected, &altered_md).expect("reinject");
        assert!(reinjected.contains("--color-primary: #00ff00;"));
        assert!(!reinjected.contains("--color-primary: #ff6600;"));
        // Title and body content STILL byte-identical.
        assert!(reinjected.contains("<title>Original Title</title>"));
        assert!(reinjected.contains("<p>Body content here</p>"));
    }

    #[test]
    fn inject_skips_when_no_recognized_tokens() {
        let html = "<html><head></head><body></body></html>";
        let md = "---\nname: \"empty\"\n---\n";
        let out = inject_design_tokens(html, md).expect("inject");
        assert_eq!(out, html, "should pass through unchanged");
    }

    #[test]
    fn inject_handles_head_with_attributes() {
        let html =
            "<html><head class=\"x\" data-y=\"z\"><title>T</title></head><body>B</body></html>";
        let out = inject_design_tokens(html, SAMPLE_FM).expect("inject");
        assert!(out.contains("data-nf-design-tokens"));
        // The head opening tag (with attrs) should be preserved.
        assert!(out.contains("<head class=\"x\" data-y=\"z\">"));
    }

    #[test]
    fn inject_does_not_match_inside_comment_placeholder() {
        // Regression: tiktok-hook.html template starts with an HTML comment
        // containing `<<HEADLINE_LINE_1>>` which embeds the substring
        // `<HEADLINE_1>` — without the `\b` word boundary, the head-finding
        // regex `<head[^>]*>` matched THAT substring (case-insensitive),
        // injecting tokens into the comment and leaving the real `<head>`
        // untouched. Result: tokens never affected the actual cascade and
        // render came out wrong.
        let html = "\
<!DOCTYPE html>
<!--
  AGENT USAGE:
  1. 替换 <<HEADLINE_LINE_1>> / <<HEADLINE_LINE_2>>
-->
<html>
<head><title>real head</title></head>
<body><div id='scene-1'>X</div></body>
</html>";
        let out = inject_design_tokens(html, SAMPLE_FM).expect("inject");
        // Block must land AFTER the real <head>, not inside the comment.
        let block_idx = out.find("data-nf-design-tokens").expect("block present");
        let real_head_idx = out.find("<head>").expect("real head still present");
        let comment_close_idx = out.find("-->").expect("comment closer present");
        assert!(
            block_idx > real_head_idx,
            "tokens must appear after real <head>, not before: block={block_idx}, head={real_head_idx}"
        );
        assert!(
            block_idx > comment_close_idx,
            "tokens must appear after the closing --> of the leading comment: block={block_idx}, comment_end={comment_close_idx}"
        );
        // Comment payload survives byte-identical (no truncation mid-token).
        assert!(
            out.contains("<<HEADLINE_LINE_1>> / <<HEADLINE_LINE_2>>"),
            "comment payload was shredded: {out}"
        );
    }

    #[test]
    fn inject_wraps_head_when_missing() {
        // Pathological: <html> with no <head>
        let html = "<html><body>just body</body></html>";
        let out = inject_design_tokens(html, SAMPLE_FM).expect("inject");
        assert!(out.contains("<head>"));
        assert!(out.contains("data-nf-design-tokens"));
        assert!(out.contains("<body>just body</body>"));
    }
}
