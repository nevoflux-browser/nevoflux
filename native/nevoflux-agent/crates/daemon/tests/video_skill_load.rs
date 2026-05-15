//! P4 integration — the /video skill loads into SkillRegistry with all
//! expected assets.
//!
//! Run via:
//!   cargo test -p nevoflux-daemon --test video_skill_load -- --ignored --nocapture

use nevoflux_skills::{LoaderConfig, SkillRegistry};
use std::path::PathBuf;

fn skills_root() -> PathBuf {
    // The /video skill ships in the browser repo at
    // docs/reference/skills/. Reach it directly; the test runs from the
    // daemon repo so the relative path to the browser repo is absolute.
    PathBuf::from("/ai/project/nevoflux/docs/reference/skills")
}

#[ignore]
#[test]
fn video_skill_loads_with_expected_assets() {
    let config = LoaderConfig::new().with_user_dir(skills_root());
    let mut registry = SkillRegistry::with_config(config);
    let count = registry.load().expect("skill load");
    assert!(
        count >= 1,
        "expected at least one skill loaded (video); got {count}"
    );
    assert!(
        registry.contains("video"),
        "registry should contain 'video' skill"
    );

    // All 31 expected aux files.
    let expected = [
        // templates (7)
        "templates/website-promo-16x9.html",
        "templates/product-intro-16x9.html",
        "templates/product-intro-9x16.html",
        "templates/tiktok-hook.html",
        "templates/video-overlay.html",
        "templates/logo-3d-reveal.html",
        "templates/product-3d-spin.html",
        // components (15)
        "components/feature-list-checkmark.html",
        "components/screenshot-reveal.html",
        "components/wipe-diagonal.html",
        "components/caption-subtitle.html",
        "components/caption-bouncy.html",
        "components/caption-typewriter.html",
        "components/caption-animated-overlay.html",
        "components/lower-third-corporate.html",
        "components/lower-third-minimal.html",
        "components/data-chart-bar-race.html",
        "components/data-chart-line.html",
        "components/flash-through-white.html",
        "components/crossfade.html",
        "components/annotation-arrow.html",
        "components/watermark-animated.html",
        // snippets (5)
        "snippets/layout-before-animation.md",
        "snippets/scene-transition-ironrules.md",
        "snippets/three-js-rules.md",
        "snippets/determinism-rules.md",
        "snippets/tts-workflow.md",
        // reference (4)
        "reference/DESIGN-template.md",
        "reference/design-md-video-extension.md",
        "reference/vocabulary.md",
        "reference/canvas-render-determinism-spec.md",
    ];
    for path in expected {
        let r = registry.read_auxiliary_file("video", path);
        assert!(r.is_ok(), "video aux file missing: {path}: {:?}", r.err());
        let content = r.unwrap();
        assert!(!content.is_empty(), "video aux file empty: {path}");
    }

    // Spot-check: triggers from the SKILL.md frontmatter include our
    // user-facing phrases.
    let skill = registry.get("video").expect("video skill exists");
    let triggers: Vec<&str> = skill.metadata.triggers.iter().map(|s| s.as_str()).collect();
    for t in ["/video", "video", "视频"] {
        assert!(
            triggers.iter().any(|x| x.contains(t)),
            "missing trigger containing '{t}': got {triggers:?}",
        );
    }
}
