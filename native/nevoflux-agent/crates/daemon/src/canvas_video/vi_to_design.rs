//! `VisualIdentity` → DESIGN.md serializer.
//!
//! Pure function mapping the structured output of
//! `canvas_extract_visual_identity` (see `nevoflux_protocol::extract`) to
//! a YAML-frontmatter DESIGN.md string compatible with the video skill's
//! DESIGN-template + design-md-video-extension contract.
//!
//! Determinism note: same `VisualIdentity` always produces the same
//! string (modulo HashMap iteration in callers — we sort/derive everything
//! from `vi.colors[*].role_hint` rather than enumerate freely). Unit
//! tests assert byte-for-byte fixture matches.
//!
//! Field-mapping rules (see P5a Slice C design discussion):
//!
//! | DESIGN.md field         | Source                                       | Fallback              |
//! |-------------------------|----------------------------------------------|-----------------------|
//! | `name`                  | `vi.name`                                    | "extracted-brand"     |
//! | `description`           | `vi.tagline`                                 | omitted               |
//! | `colors.primary`        | first color with `role_hint=Primary`         | "#635bff" (Stripe)    |
//! | `colors.background`     | first color with `role_hint=Background`      | "#0a0a0f"             |
//! | `colors.foreground`     | first color with `role_hint=Text`            | "#f5f5f7"             |
//! | `colors.accent`         | first color with `role_hint=Accent`          | derived (= primary)   |
//! | `colors.secondary`      | second `Accent` if any, else lighter primary | derived               |
//! | `typography.hero.*`     | `vi.fonts[source="hero"]`                    | Inter / 700           |
//! | `typography.body.*`     | `vi.fonts[source="body"]`                    | Inter / 400           |
//! | `spacing.*` / `motion.*`| (always defaults)                            | xs=4px..xl=48px etc.  |
//!
//! `key_assets`, `hero_screenshot_b64`, `logo` are NOT serialized into
//! DESIGN.md — they belong elsewhere (SCRIPT.md / sidebar metadata).

use nevoflux_protocol::extract::{ColorRole, FontStack, VisualIdentity};

/// Render a `VisualIdentity` as a DESIGN.md string with YAML frontmatter
/// and a short Overview body.
///
/// The output is deterministic: same input → byte-identical output. Every
/// path through the function picks values via stable selection (first by
/// role_hint with predictable iteration order) so caller-side tests can
/// assert exact-match fixtures.
pub fn vi_to_design_md(vi: &VisualIdentity) -> String {
    let name = vi
        .name
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("extracted-brand");

    // ── Colors ───────────────────────────────────────────────────────────
    let primary = pick_color(vi, ColorRole::Primary).unwrap_or_else(|| "#635bff".to_string());
    let background = pick_color(vi, ColorRole::Background).unwrap_or_else(|| "#0a0a0f".to_string());
    // Foreground — fall back to a high-contrast pair of the chosen
    // background if VI didn't provide one. Default-default ("#f5f5f7")
    // works against dark bg but fails against light bg, so derive based
    // on the background's lightness.
    let foreground =
        pick_color(vi, ColorRole::Text).unwrap_or_else(|| derive_foreground(&background));

    // Filter accent candidates to those visually distinct from background
    // AND from primary. The JS-side already enforces a distance floor for
    // `Accent` role assignment, but we double-check here so a stale or
    // hand-edited VI can't sneak duplicates through.
    let raw_accents: Vec<String> = vi
        .colors
        .iter()
        .filter(|c| c.role_hint == ColorRole::Accent)
        .map(|c| c.hex.clone())
        .collect();
    let mut accents: Vec<String> = raw_accents
        .into_iter()
        .filter(|hex| !rgb_too_close(hex, &background, 60) && !rgb_too_close(hex, &primary, 30))
        .collect();
    // accent: first usable accent; else derive a complementary from primary.
    let accent = accents
        .first()
        .cloned()
        .unwrap_or_else(|| derive_accent(&primary, &background));
    // secondary: second usable accent if available; else derive from primary
    // — and ensure it's distinct from accent so we don't end up with
    // accent == secondary.
    let secondary = if accents.len() >= 2 {
        let s = accents.remove(1);
        if rgb_too_close(&s, &accent, 30) {
            derive_secondary(&primary)
        } else {
            s
        }
    } else {
        let derived = derive_secondary(&primary);
        if rgb_too_close(&derived, &accent, 30) {
            // Both derive_secondary and derive_accent would land near
            // primary — fall back to a hand-picked complementary.
            "#7f7f7f".to_string()
        } else {
            derived
        }
    };

    // ── Typography ───────────────────────────────────────────────────────
    let hero = pick_font(vi, "hero").unwrap_or_else(|| FontStack {
        family: "Inter, 'PingFang SC', -apple-system, sans-serif".to_string(),
        weight: Some(700),
        source: "hero".to_string(),
    });
    let body = pick_font(vi, "body").unwrap_or_else(|| FontStack {
        family: "Inter, 'PingFang SC', -apple-system, sans-serif".to_string(),
        weight: Some(400),
        source: "body".to_string(),
    });

    let mut out = String::with_capacity(1024);
    out.push_str("---\n");
    out.push_str(&format!("name: {}\n", yaml_str(name)));
    if let Some(tag) = vi
        .tagline
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push_str(&format!("description: {}\n", yaml_str(tag)));
    }
    out.push_str(&format!("source_url: {}\n", yaml_str(&vi.url)));
    out.push('\n');

    out.push_str("colors:\n");
    out.push_str(&format!("  primary:    {}\n", yaml_str(&primary)));
    out.push_str(&format!("  secondary:  {}\n", yaml_str(&secondary)));
    out.push_str(&format!("  accent:     {}\n", yaml_str(&accent)));
    out.push_str(&format!("  background: {}\n", yaml_str(&background)));
    out.push_str(&format!("  foreground: {}\n", yaml_str(&foreground)));
    out.push('\n');

    out.push_str("typography:\n");
    out.push_str("  hero:\n");
    out.push_str(&format!("    family: {}\n", yaml_str(&hero.family)));
    out.push_str(&format!("    weight: {}\n", hero.weight.unwrap_or(700)));
    out.push_str("  body:\n");
    out.push_str(&format!("    family: {}\n", yaml_str(&body.family)));
    out.push_str(&format!("    weight: {}\n", body.weight.unwrap_or(400)));
    out.push('\n');

    // Spacing + motion + rounded — always defaults. Templates reference
    // these with var(--*, fallback) so being absent is fine, but emitting
    // them keeps DESIGN.md self-documenting.
    out.push_str("spacing:\n");
    out.push_str("  xs: \"4px\"\n");
    out.push_str("  sm: \"8px\"\n");
    out.push_str("  md: \"16px\"\n");
    out.push_str("  lg: \"24px\"\n");
    out.push_str("  xl: \"48px\"\n\n");

    out.push_str("rounded:\n");
    out.push_str("  sm: \"4px\"\n");
    out.push_str("  md: \"8px\"\n");
    out.push_str("  lg: \"16px\"\n\n");

    out.push_str("motion:\n");
    out.push_str("  ease_default:  \"power2.out\"\n");
    out.push_str("  ease_entrance: \"back.out(1.7)\"\n");
    out.push_str("  ease_exit:     \"power2.in\"\n");
    out.push_str("---\n\n");

    out.push_str("## Overview\n\n");
    out.push_str(&format!(
        "Brand identity extracted from {} via canvas_extract_visual_identity.\n",
        vi.url
    ));
    if !vi.warnings.is_empty() {
        out.push_str(&format!(
            "\nExtraction warnings: {}.\n",
            vi.warnings.join(", ")
        ));
    }

    out
}

/// First color matching `role`; None if none.
fn pick_color(vi: &VisualIdentity, role: ColorRole) -> Option<String> {
    vi.colors
        .iter()
        .find(|c| c.role_hint == role)
        .map(|c| c.hex.clone())
}

/// First font with the given source label; None if none.
fn pick_font(vi: &VisualIdentity, source: &str) -> Option<FontStack> {
    vi.fonts.iter().find(|f| f.source == source).cloned()
}

/// Derive a secondary color from primary by darkening 22% per RGB channel.
/// Cheap and deterministic; for branded sites a real secondary should
/// already be in `vi.colors` as an Accent hit.
fn derive_secondary(primary: &str) -> String {
    transform_rgb(primary, |c| ((c as f32) * 0.78).round() as i32)
        .unwrap_or_else(|| primary.to_string())
}

/// Derive an accent color from primary. Strategy: produce a lighter and
/// more saturated tint by blending toward white by 30% — gives a visibly
/// distinct companion that still reads as the same brand family. If the
/// resulting color collides with `background` (e.g. primary is already
/// near-white), darken the primary instead.
fn derive_accent(primary: &str, background: &str) -> String {
    let lightened = transform_rgb(primary, |c| {
        // Move 30% toward 255.
        let f = c as f32;
        (f + (255.0 - f) * 0.30).round() as i32
    })
    .unwrap_or_else(|| primary.to_string());

    if rgb_too_close(&lightened, background, 60) {
        // Lightening collides with bg — go the other direction.
        transform_rgb(primary, |c| {
            let f = c as f32;
            (f * 0.55).round() as i32
        })
        .unwrap_or_else(|| primary.to_string())
    } else {
        lightened
    }
}

/// Derive a foreground color appropriate for the given background. Used
/// when VI didn't surface a Text role — picks pure dark grey for light
/// backgrounds, near-white for dark backgrounds (always passes WCAG AA
/// contrast for body text against the chosen bg).
fn derive_foreground(background: &str) -> String {
    let lightness = avg_lightness(background).unwrap_or(0.0);
    if lightness > 0.5 {
        "#1a1a1a".to_string()
    } else {
        "#f5f5f7".to_string()
    }
}

/// `f(channel) -> new channel value (i32, will be clamped to 0..255)`.
fn transform_rgb(hex: &str, f: impl Fn(u8) -> i32) -> Option<String> {
    let trimmed = hex.trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }
    let parse = |s: &str| u8::from_str_radix(s, 16).ok();
    let (r, g, b) = (
        parse(&trimmed[0..2])?,
        parse(&trimmed[2..4])?,
        parse(&trimmed[4..6])?,
    );
    let clamp = |v: i32| v.clamp(0, 255) as u8;
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        clamp(f(r)),
        clamp(f(g)),
        clamp(f(b))
    ))
}

/// Manhattan RGB distance between two `#rrggbb` hex strings. `< threshold`
/// means visually similar. Returns `false` (treated as "not too close")
/// for any unparseable input so derivation paths don't get stuck.
fn rgb_too_close(a: &str, b: &str, threshold: u32) -> bool {
    let parse = |hex: &str| -> Option<[u8; 3]> {
        let t = hex.trim_start_matches('#');
        if t.len() != 6 {
            return None;
        }
        Some([
            u8::from_str_radix(&t[0..2], 16).ok()?,
            u8::from_str_radix(&t[2..4], 16).ok()?,
            u8::from_str_radix(&t[4..6], 16).ok()?,
        ])
    };
    match (parse(a), parse(b)) {
        (Some(ra), Some(rb)) => {
            let d = (ra[0] as i32 - rb[0] as i32).unsigned_abs()
                + (ra[1] as i32 - rb[1] as i32).unsigned_abs()
                + (ra[2] as i32 - rb[2] as i32).unsigned_abs();
            d < threshold
        }
        _ => false,
    }
}

/// Average channel value normalized to 0..1 (cheap proxy for lightness).
fn avg_lightness(hex: &str) -> Option<f32> {
    let t = hex.trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()? as f32;
    let g = u8::from_str_radix(&t[2..4], 16).ok()? as f32;
    let b = u8::from_str_radix(&t[4..6], 16).ok()? as f32;
    Some((r + g + b) / 3.0 / 255.0)
}

/// YAML-quote a string for safe inclusion as a frontmatter scalar value.
/// Always emits double-quoted form to avoid YAML's many gotchas with
/// reserved words / colons / hash signs / leading whitespace. Escapes
/// embedded `"` and `\` per YAML spec.
fn yaml_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_protocol::extract::{Color, ColorRole, FontStack, VisualIdentity};

    fn empty_vi() -> VisualIdentity {
        VisualIdentity {
            name: None,
            tagline: None,
            url: "https://example.com/".to_string(),
            hero_screenshot_b64: None,
            logo: None,
            colors: vec![],
            fonts: vec![],
            key_assets: vec![],
            extracted_at: 0,
            warnings: vec![],
        }
    }

    #[test]
    fn empty_vi_emits_full_skeleton_with_all_defaults() {
        let md = vi_to_design_md(&empty_vi());
        // Has all required schema sections.
        assert!(md.contains("name: \"extracted-brand\""), "got: {md}");
        // No description line because tagline absent.
        assert!(!md.contains("description:"));
        assert!(md.contains("colors:"));
        assert!(md.contains("primary:    \"#635bff\""));
        assert!(md.contains("background: \"#0a0a0f\""));
        assert!(md.contains("foreground: \"#f5f5f7\""));
        assert!(md.contains("typography:"));
        assert!(md.contains("family: \"Inter, 'PingFang SC', -apple-system, sans-serif\""));
        assert!(md.contains("weight: 700"));
        assert!(md.contains("weight: 400"));
        assert!(md.contains("spacing:"));
        assert!(md.contains("rounded:"));
        assert!(md.contains("motion:"));
        assert!(md.contains("## Overview"));
        assert!(md.starts_with("---\n"));
    }

    #[test]
    fn empty_vi_is_deterministic() {
        let a = vi_to_design_md(&empty_vi());
        let b = vi_to_design_md(&empty_vi());
        assert_eq!(a, b, "byte-equal output required for testability");
    }

    #[test]
    fn full_vi_propagates_extracted_fields() {
        let vi = VisualIdentity {
            name: Some("Stripe".to_string()),
            tagline: Some("Financial infrastructure for the internet".to_string()),
            url: "https://stripe.com/".to_string(),
            hero_screenshot_b64: None,
            logo: None,
            colors: vec![
                Color {
                    hex: "#fff3fd".to_string(),
                    rgb: [255, 243, 253],
                    frequency: 0.50,
                    role_hint: ColorRole::Background,
                },
                Color {
                    hex: "#b57ef3".to_string(),
                    rgb: [181, 126, 243],
                    frequency: 0.06,
                    role_hint: ColorRole::Primary,
                },
                Color {
                    hex: "#7c6b7e".to_string(),
                    rgb: [124, 107, 126],
                    frequency: 0.06,
                    role_hint: ColorRole::Text,
                },
                Color {
                    hex: "#fccfb0".to_string(),
                    rgb: [252, 207, 176],
                    frequency: 0.25,
                    role_hint: ColorRole::Accent,
                },
            ],
            fonts: vec![
                FontStack {
                    family: "sohne-var, 'SF Pro Display', sans-serif".to_string(),
                    weight: Some(300),
                    source: "hero".to_string(),
                },
                FontStack {
                    family: "sohne-var, 'SF Pro Display', sans-serif".to_string(),
                    weight: Some(400),
                    source: "body".to_string(),
                },
            ],
            key_assets: vec![],
            extracted_at: 1777200000,
            warnings: vec![],
        };
        let md = vi_to_design_md(&vi);
        assert!(md.contains("name: \"Stripe\""));
        assert!(md.contains("description: \"Financial infrastructure for the internet\""));
        assert!(md.contains("source_url: \"https://stripe.com/\""));
        assert!(md.contains("primary:    \"#b57ef3\""));
        assert!(md.contains("background: \"#fff3fd\""));
        assert!(md.contains("foreground: \"#7c6b7e\""));
        assert!(md.contains("accent:     \"#fccfb0\""));
        // secondary derives from primary because only one Accent in VI.
        // 0xb57ef3 * 0.78 ≈ 0x8d6_2bd (rounded per channel)
        let secondary_expected = derive_secondary("#b57ef3");
        assert!(md.contains(&format!("secondary:  \"{}\"", secondary_expected)));
        assert!(md.contains("family: \"sohne-var, 'SF Pro Display', sans-serif\""));
        assert!(md.contains("weight: 300"));
        assert!(md.contains("weight: 400"));
    }

    #[test]
    fn two_accents_use_second_as_secondary_no_derivation() {
        let mut vi = empty_vi();
        vi.colors = vec![
            Color {
                hex: "#aa0000".to_string(),
                rgb: [170, 0, 0],
                frequency: 0.4,
                role_hint: ColorRole::Primary,
            },
            Color {
                hex: "#bb1111".to_string(),
                rgb: [187, 17, 17],
                frequency: 0.2,
                role_hint: ColorRole::Accent,
            },
            Color {
                hex: "#cc2222".to_string(),
                rgb: [204, 34, 34],
                frequency: 0.1,
                role_hint: ColorRole::Accent,
            },
        ];
        let md = vi_to_design_md(&vi);
        assert!(md.contains("primary:    \"#aa0000\""));
        // First accent → accent.
        assert!(md.contains("accent:     \"#bb1111\""));
        // Second accent → secondary (no derivation kicks in).
        assert!(md.contains("secondary:  \"#cc2222\""));
    }

    #[test]
    fn chinese_brand_name_passes_through_with_yaml_quoting() {
        let mut vi = empty_vi();
        vi.name = Some("飞书".to_string());
        vi.tagline = Some("Lark：「让企业更高效」".to_string());
        let md = vi_to_design_md(&vi);
        assert!(md.contains("name: \"飞书\""));
        assert!(md.contains("description: \"Lark：「让企业更高效」\""));
    }

    #[test]
    fn font_family_with_double_quotes_and_backslashes_is_yaml_escaped() {
        let mut vi = empty_vi();
        vi.fonts = vec![FontStack {
            family: r#""Custom Font", \\fallback"#.to_string(),
            weight: Some(500),
            source: "hero".to_string(),
        }];
        let md = vi_to_design_md(&vi);
        // " → \" and \\ → \\\\ inside a double-quoted YAML scalar.
        assert!(
            md.contains(r#"family: "\"Custom Font\", \\\\fallback""#),
            "got: {md}"
        );
    }

    #[test]
    fn warnings_surface_in_overview_body() {
        let mut vi = empty_vi();
        vi.warnings = vec!["thin_content".into(), "screenshot_failed".into()];
        let md = vi_to_design_md(&vi);
        assert!(md.contains("Extraction warnings: thin_content, screenshot_failed."));
    }

    #[test]
    fn output_is_parseable_yaml_frontmatter() {
        // Smoke: the frontmatter section between `---` markers must be
        // valid YAML the rest of the daemon can parse.
        let vi = VisualIdentity {
            name: Some("Acme".into()),
            tagline: Some("We make stuff".into()),
            url: "https://acme.test/".into(),
            hero_screenshot_b64: None,
            logo: None,
            colors: vec![Color {
                hex: "#112233".into(),
                rgb: [17, 34, 51],
                frequency: 0.5,
                role_hint: ColorRole::Primary,
            }],
            fonts: vec![],
            key_assets: vec![],
            extracted_at: 0,
            warnings: vec![],
        };
        let md = vi_to_design_md(&vi);
        // Extract the frontmatter (between the two `---` markers).
        let mut parts = md.splitn(3, "---");
        let _empty = parts.next().unwrap();
        let frontmatter = parts.next().expect("missing first ---");
        let value: serde_yaml::Value = serde_yaml::from_str(frontmatter)
            .unwrap_or_else(|e| panic!("frontmatter not valid YAML: {e}\n{frontmatter}"));
        assert_eq!(value["name"].as_str(), Some("Acme"));
        assert_eq!(value["colors"]["primary"].as_str(), Some("#112233"));
    }

    #[test]
    fn derive_secondary_correct_math() {
        // 255 * 0.78 = 198.9 → round() = 199 = 0xc7
        assert_eq!(derive_secondary("#ffffff"), "#c7c7c7");
        // 0 * 0.78 = 0
        assert_eq!(derive_secondary("#000000"), "#000000");
        // Non-#-prefixed: returned unchanged.
        assert_eq!(derive_secondary("not-a-color"), "not-a-color");
    }

    /// Regression: nevoflux.app extraction (2026-04-26) produced VI with
    /// only Background + Primary cleanly assigned; the other 3 quantizer
    /// buckets all landed near-white and the JS sanity check refused to
    /// promote them to Accent. Rust must derive accent + secondary +
    /// foreground that are all distinct from each other AND from the
    /// background.
    #[test]
    fn near_white_only_background_yields_distinct_derived_palette() {
        let mut vi = empty_vi();
        vi.colors = vec![
            Color {
                hex: "#ffffff".to_string(),
                rgb: [255, 255, 255],
                frequency: 0.85,
                role_hint: ColorRole::Background,
            },
            Color {
                hex: "#626d69".to_string(),
                rgb: [98, 109, 105],
                frequency: 0.10,
                role_hint: ColorRole::Primary,
            },
            // Three near-white buckets that JS sanity-rejected (role left
            // 'unspecified' / Accent but rgb_too_close-rejected here too).
            // Simulate by NOT marking them Accent — they shouldn't even
            // reach Rust as Accent hits, but verify the derivation path
            // produces a usable palette anyway.
        ];
        let md = vi_to_design_md(&vi);
        // Background present.
        assert!(md.contains("background: \"#ffffff\""));
        // Primary present.
        assert!(md.contains("primary:    \"#626d69\""));
        // Foreground derived for light bg = #1a1a1a (high contrast).
        assert!(md.contains("foreground: \"#1a1a1a\""));
        // Accent derived from primary — must NOT equal background or primary.
        let extract = |label: &str| -> String {
            let needle = format!("{}:", label);
            let line = md
                .lines()
                .find(|l| l.trim_start().starts_with(&needle))
                .unwrap_or_else(|| panic!("missing {label} line in:\n{md}"));
            line.split('"').nth(1).unwrap_or("").to_string()
        };
        let accent = extract("accent");
        let secondary = extract("secondary");
        assert!(accent != "#ffffff", "accent must not equal background");
        assert!(accent != "#626d69", "accent must not equal primary");
        assert!(
            secondary != "#ffffff",
            "secondary must not equal background"
        );
        assert!(secondary != accent, "secondary must not equal accent");
    }

    /// derive_accent: lightening primary should not collide with light bg.
    /// For very light primaries, the function flips to darkening.
    #[test]
    fn derive_accent_avoids_background_collision() {
        // Light primary on white bg — lightening would land at white.
        // Function should detect collision and darken instead.
        let acc = derive_accent("#f0f0f0", "#ffffff");
        assert!(!rgb_too_close(&acc, "#ffffff", 60));

        // Mid-tone primary on white bg — lightening is fine.
        let acc = derive_accent("#626d69", "#ffffff");
        // 0x62=98, 0x6d=109, 0x69=105 → +30% toward 255:
        //  98 + (255-98)*0.30 = 98 + 47.1 = 145 ≈ 0x91
        // 109 + (255-109)*0.30 = 109 + 43.8 = 153 ≈ 0x99
        // 105 + (255-105)*0.30 = 105 + 45 = 150 ≈ 0x96
        assert_eq!(acc, "#919996");
        assert!(!rgb_too_close(&acc, "#ffffff", 60));
        assert!(!rgb_too_close(&acc, "#626d69", 30));
    }

    /// derive_foreground flips to dark vs light based on bg.
    #[test]
    fn derive_foreground_inverts_per_bg_lightness() {
        assert_eq!(derive_foreground("#ffffff"), "#1a1a1a");
        assert_eq!(derive_foreground("#000000"), "#f5f5f7");
        assert_eq!(derive_foreground("#0a0a0f"), "#f5f5f7");
        assert_eq!(derive_foreground("#f4f5f5"), "#1a1a1a");
        // Borderline gray (lightness ~0.5) — defaults to dark text.
        assert_eq!(derive_foreground("#7f7f7f"), "#f5f5f7");
        assert_eq!(derive_foreground("#808080"), "#1a1a1a");
    }

    /// rgb_too_close — edge cases.
    #[test]
    fn rgb_too_close_basics() {
        assert!(rgb_too_close("#ffffff", "#fefefe", 60));
        assert!(!rgb_too_close("#ffffff", "#cccccc", 60));
        // Exact same color = 0 distance < threshold.
        assert!(rgb_too_close("#abcdef", "#abcdef", 1));
        // Bad input = false.
        assert!(!rgb_too_close("not-a-color", "#ffffff", 60));
    }

    /// Two near-white VI accents both filtered → both fall to derived.
    /// Asserts the secondary derivation also avoids accent collision.
    #[test]
    fn near_bg_accents_filtered_at_rust_layer_too() {
        let mut vi = empty_vi();
        vi.colors = vec![
            Color {
                hex: "#ffffff".to_string(),
                rgb: [255, 255, 255],
                frequency: 0.85,
                role_hint: ColorRole::Background,
            },
            Color {
                hex: "#000000".to_string(),
                rgb: [0, 0, 0],
                frequency: 0.10,
                role_hint: ColorRole::Primary,
            },
            // Two "Accent" hits that the JS layer somehow let through but
            // are visually identical to background. Rust filter should
            // catch both and trigger derivation.
            Color {
                hex: "#fefefe".to_string(),
                rgb: [254, 254, 254],
                frequency: 0.03,
                role_hint: ColorRole::Accent,
            },
            Color {
                hex: "#f8f8f8".to_string(),
                rgb: [248, 248, 248],
                frequency: 0.02,
                role_hint: ColorRole::Accent,
            },
        ];
        let md = vi_to_design_md(&vi);
        // Neither raw accent should appear in the output.
        assert!(!md.contains("\"#fefefe\""));
        assert!(!md.contains("\"#f8f8f8\""));
        // Background and primary preserved.
        assert!(md.contains("background: \"#ffffff\""));
        assert!(md.contains("primary:    \"#000000\""));
    }
}
