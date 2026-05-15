//! Visual identity extraction protocol types.
//!
//! Implements the wire shapes for the `canvas_extract_visual_identity` tool
//! per umbrella spec §6 (Website-to-Video Mode). The tool runs a browser
//! extraction script over a URL or existing tab and returns a structured
//! brand identity blob that can auto-fill a composition's DESIGN.md.
//!
//! Slice A scope: protocol shapes + dispatch wiring + minimal extraction
//! (title / description / url / hero screenshot). Color / font / logo /
//! key-asset extraction lands in Slice B.
//!
//! See: `docs/superpowers/specs/2026-04-26-video-skill-p5a-design.md`

use serde::{Deserialize, Serialize};

/// Target of an extraction — either a URL to open in a background tab or an
/// existing tab to read. The two variants are mutually exclusive at the API
/// boundary; callers MUST set exactly one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractTarget {
    /// URL string, e.g. `https://stripe.com`. Daemon opens a background tab,
    /// runs extraction, closes the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Existing tab id (WebExtension tab id). Daemon reuses the tab and
    /// does NOT close it after extraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
}

/// `canvas_extract_visual_identity` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractVisualIdentityRequest {
    pub target: ExtractTarget,
    /// Wall-clock budget in seconds for the entire extraction (open tab +
    /// wait load + run script + close tab). Default 20 per spec §6.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_sec: Option<u32>,
    /// Viewport dimensions `[width, height]` for the screenshot. Default
    /// `[1920, 1080]` (16:9) per spec §6.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<[u32; 2]>,
}

/// Role hint produced by color extraction heuristics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorRole {
    Primary,
    Background,
    Text,
    Accent,
    /// Hint not assigned by heuristics (Slice A always emits this; Slice B
    /// upgrades to Primary/Background/Text/Accent).
    Unspecified,
}

/// One quantized color from the hero screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    /// 7-char hex, e.g. `#635bff`.
    pub hex: String,
    /// `[r, g, b]` 0..255.
    pub rgb: [u8; 3],
    /// Frequency in [0.0, 1.0] of pixels matching this color cluster.
    pub frequency: f32,
    pub role_hint: ColorRole,
}

/// One detected font stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontStack {
    /// Raw `font-family` value with full fallback chain.
    pub family: String,
    /// Numeric font weight (400, 700, etc.) when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// Where this font was sampled from: `"hero"` (`<h1>`), `"body"`
    /// (`<body>`), `"mono"` (`<code>`/`<pre>`), or `"other"`.
    pub source: String,
}

/// Logo asset detected on the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoAsset {
    /// Resolved absolute URL of the logo image.
    pub url: String,
    /// `"apple-touch-icon"`, `"og:image"`, `"img-logo"`, `"header-img"`,
    /// or `"link-icon"` per spec §6.3 priority chain.
    pub source: String,
    /// Square-ness score 0..1 (1.0 = perfectly square). Higher is better.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub square_score: Option<f32>,
}

/// One feature/value-proposition item identified on the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureItem {
    pub text: String,
    /// Detection confidence 0..1.
    pub confidence: f32,
}

/// Full extraction result returned by the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualIdentity {
    /// Brand / product name (`og:title` || `twitter:title` || `<title>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tagline / one-line pitch (`og:description` || `meta[name=description]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    /// Final URL after redirects.
    pub url: String,
    /// Hero screenshot — base64-encoded PNG bytes (data URL prefix stripped).
    /// Slice A always emits this; Slice B uses it for color quantization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hero_screenshot_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<LogoAsset>,
    /// Top 5 colors, frequency descending. Slice A returns empty.
    #[serde(default)]
    pub colors: Vec<Color>,
    /// Top 3 fonts (hero / body / mono). Slice A returns empty.
    #[serde(default)]
    pub fonts: Vec<FontStack>,
    /// Best-effort top 3-5 feature items. Slice A returns empty.
    #[serde(default)]
    pub key_assets: Vec<FeatureItem>,
    /// Unix epoch seconds when extraction completed.
    pub extracted_at: i64,
    /// Soft warnings (e.g. `"hydrate_incomplete"`, `"login_wall"`,
    /// `"webp_artifacts"`) — empty when extraction was clean.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_target_url_only_serializes_compact() {
        let t = ExtractTarget {
            url: Some("https://stripe.com".into()),
            tab_id: None,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"url\":\"https://stripe.com\""));
        assert!(!json.contains("tab_id"));
    }

    #[test]
    fn extract_target_tab_id_only_serializes_compact() {
        let t = ExtractTarget {
            url: None,
            tab_id: Some(42),
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"tab_id\":42"));
        assert!(!json.contains("\"url\""));
    }

    #[test]
    fn request_optional_fields_default() {
        let json = r#"{"target":{"url":"https://example.com"}}"#;
        let req: ExtractVisualIdentityRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.target.url.as_deref(), Some("https://example.com"));
        assert!(req.timeout_sec.is_none());
        assert!(req.viewport.is_none());
    }

    #[test]
    fn request_rejects_unknown_top_level_field() {
        let json = r#"{"target":{"url":"https://example.com"},"haha":1}"#;
        let res: Result<ExtractVisualIdentityRequest, _> = serde_json::from_str(json);
        assert!(res.is_err(), "deny_unknown_fields must reject 'haha'");
    }

    #[test]
    fn extract_action_serializes_as_extract_visual_identity_camelcase() {
        // Verify the wire form the extension expects. BrowserToolAction has
        // `#[serde(rename_all = "snake_case")]` for the enum, but we
        // overrode this variant with `#[serde(rename = "extractVisualIdentity")]`
        // so it matches the existing camelCase override pattern (uploadFile,
        // activateTab, fillRichText). The extension's case statement keys on
        // this exact string.
        use crate::common::BrowserToolAction;
        let s = serde_json::to_string(&BrowserToolAction::ExtractVisualIdentity).unwrap();
        assert_eq!(s, "\"extractVisualIdentity\"");
    }

    #[test]
    fn visual_identity_round_trip() {
        let vi = VisualIdentity {
            name: Some("Stripe".into()),
            tagline: Some("Payments infrastructure".into()),
            url: "https://stripe.com".into(),
            hero_screenshot_b64: None,
            logo: None,
            colors: vec![Color {
                hex: "#635bff".into(),
                rgb: [99, 91, 255],
                frequency: 0.42,
                role_hint: ColorRole::Primary,
            }],
            fonts: vec![FontStack {
                family: "Sohne, Inter, system-ui".into(),
                weight: Some(700),
                source: "hero".into(),
            }],
            key_assets: vec![],
            extracted_at: 1777200000,
            warnings: vec![],
        };
        let json = serde_json::to_string(&vi).unwrap();
        let back: VisualIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(back.url, vi.url);
        assert_eq!(back.colors.len(), 1);
        assert_eq!(back.colors[0].rgb, [99, 91, 255]);
    }
}
