//! P4 integration — verify every shipped video template ships a matching
//! `<name>.design.md`, all parse as valid YAML, and contain the Google base
//! tokens (colors / typography / spacing) needed for daemon-side token
//! injection.
//!
//! This test was previously gated behind `#[ignore]` because the daemon
//! didn't actually consume DESIGN.md. Now that `canvas_video::create` injects
//! tokens parsed from the per-template default DESIGN.md, this test enforces
//! the contract: every template directory must have a default DESIGN.md
//! with the minimum Google base schema.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
struct DesignFrontmatter {
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
    #[serde(default)]
    colors: HashMap<String, String>,
    #[serde(default)]
    typography: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    spacing: HashMap<String, String>,
    #[serde(default)]
    #[allow(dead_code)]
    rounded: HashMap<String, String>,
    #[serde(default)]
    #[allow(dead_code)]
    components: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    motion: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    #[allow(dead_code)]
    voice: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    #[allow(dead_code)]
    aspect: Option<HashMap<String, serde_yaml::Value>>,
}

fn extract_frontmatter(full: &str) -> &str {
    let stripped = full
        .strip_prefix("---\n")
        .expect("document starts with ---");
    let end = stripped
        .find("\n---\n")
        .expect("frontmatter closes with ---");
    &stripped[..end]
}

const VIDEO_DIR: &str = "/ai/project/nevoflux/docs/reference/skills/video";

const SHIPPED_TEMPLATES: &[&str] = &[
    "tiktok-hook",
    "product-intro-9x16",
    "product-intro-16x9",
    "logo-3d-reveal",
    "product-3d-spin",
    "video-overlay",
    "website-promo-16x9",
];

/// The shared schema-reference still parses cleanly with the full set of
/// Google base + video extension tokens (sanity check that the production
/// `DesignFrontmatter` shape matches what `DESIGN-template.md` advertises).
#[test]
fn design_template_reference_parses_with_google_base_plus_video_extensions() {
    let path = format!("{VIDEO_DIR}/reference/DESIGN-template.md");
    let text = fs::read_to_string(&path).expect("DESIGN-template.md exists");
    let front = extract_frontmatter(&text);
    let fm: DesignFrontmatter = serde_yaml::from_str(front).expect("frontmatter parses as YAML");

    for color in ["primary", "secondary", "background", "foreground"] {
        assert!(
            fm.colors.contains_key(color),
            "missing color token: {color} (got {:?})",
            fm.colors.keys().collect::<Vec<_>>(),
        );
    }
    assert!(
        !fm.typography.is_empty(),
        "typography must have at least one entry",
    );
    assert!(
        !fm.spacing.is_empty(),
        "spacing must have at least one entry",
    );
    let motion = fm.motion.as_ref().expect("motion extension required");
    for key in ["ease_default", "scene_duration_default", "stagger_default"] {
        assert!(motion.contains_key(key), "missing motion.{key}");
    }
}

/// Every shipped template must have a matching `<name>.design.md` next to
/// `<name>.html`, parseable as YAML, with at minimum the Google base
/// `colors.primary` / `colors.background` / `colors.foreground` plus
/// `typography.hero` and `spacing.lg`. Template-default brand identity is
/// what the daemon falls back to when the caller doesn't supply
/// `design_md`.
#[test]
fn each_shipped_template_has_a_matching_design_md() {
    for tpl in SHIPPED_TEMPLATES {
        let html_path = format!("{VIDEO_DIR}/templates/{tpl}.html");
        let design_path = format!("{VIDEO_DIR}/templates/{tpl}.design.md");
        assert!(
            fs::metadata(&html_path).is_ok(),
            "template HTML missing: {html_path}",
        );
        assert!(
            fs::metadata(&design_path).is_ok(),
            "template-specific DESIGN.md missing: {design_path}",
        );

        let text =
            fs::read_to_string(&design_path).unwrap_or_else(|e| panic!("read {design_path}: {e}"));
        let front = extract_frontmatter(&text);
        let fm: DesignFrontmatter = serde_yaml::from_str(front)
            .unwrap_or_else(|e| panic!("{tpl}: frontmatter parse error: {e}"));

        for color in ["primary", "background", "foreground"] {
            assert!(
                fm.colors.contains_key(color),
                "{tpl}: missing required color {color} (got {:?})",
                fm.colors.keys().collect::<Vec<_>>(),
            );
        }
        assert!(
            fm.typography.contains_key("hero"),
            "{tpl}: typography.hero required (got {:?})",
            fm.typography.keys().collect::<Vec<_>>(),
        );
        assert!(
            fm.spacing.contains_key("lg"),
            "{tpl}: spacing.lg required (got {:?})",
            fm.spacing.keys().collect::<Vec<_>>(),
        );
    }
}
