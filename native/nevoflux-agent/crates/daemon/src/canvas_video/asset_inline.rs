//! Asset inlining for the render pipeline.
//!
//! Compositions are stored as multi-file artifacts: HTML lives at
//! `files["index.html"]`, brand assets at `files["assets/hero.png"]`,
//! `files["assets/logo.svg"]`, etc. (base64-encoded for binary types).
//!
//! The render tab loads the composition HTML in a sandbox where there is
//! no HTTP origin to resolve `<img src="assets/hero.png">` against. To
//! make assets actually appear, we preprocess the HTML on the way out of
//! `load_composition`: every `assets/X` reference (in `<img|video|audio
//! |source|link|script src=>`, in CSS `url()`, in `<image href>`) is
//! replaced with a `data:` URI built from `files["assets/X"]`.
//!
//! References that have no matching files entry are left as-is so the
//! linter (`nf/asset-not-in-files`) can flag them at lint time. Refs
//! that are absolute URLs (`https://`, `data:`, `blob:`) are skipped.
//!
//! NOTE: This is a render-time view-only transform. The artifact's stored
//! `index.html` keeps the `assets/...` references unchanged — that's the
//! source of truth Canvas Editor edits, and what the agent reads back.

use std::collections::HashMap;

/// Replace every `assets/<path>` reference in `html` with a `data:` URI
/// built from the corresponding entry in `files`. Returns the
/// transformed HTML; missing assets are left untouched (the linter will
/// flag them).
///
/// Patterns recognized (case-insensitive on attribute names):
/// - `<img src="assets/X">` and the `srcset` variant (first URL only)
/// - `<video|audio|source src="assets/X">`
/// - `<link href="assets/X">` (rare for assets, but covers font @font-face)
/// - `<image href="assets/X">` (SVG)
/// - `<script src="assets/X">`
/// - CSS `url(assets/X)` — both inline `<style>` and `style="..."` attrs
pub fn inline_assets(html: &str, files: &HashMap<String, String>) -> String {
    if files.is_empty() || !html.contains("assets/") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next "assets/" prefix that's wrapped in either:
        //   src=" ... " | src=' ... '
        //   href=" ... " | href=' ... '
        //   url( ... ) | url(" ... ") | url(' ... ')
        if let Some(rel) = bytes[i..].iter().position(|&b| b == b'a') {
            let p = i + rel;
            if p + 7 <= bytes.len() && &bytes[p..p + 7] == b"assets/" {
                // Walk back to find the surrounding quote/opening punctuation.
                let (open_pos, open_kind) = match find_opening(&bytes, p) {
                    Some(v) => v,
                    None => {
                        out.push_str(&html[i..p + 7]);
                        i = p + 7;
                        continue;
                    }
                };
                // Walk forward to find the matching close.
                let close_pos = match find_closing(&bytes, p + 7, open_kind) {
                    Some(v) => v,
                    None => {
                        out.push_str(&html[i..p + 7]);
                        i = p + 7;
                        continue;
                    }
                };
                // The asset path is bytes[open_pos+1 .. close_pos], possibly
                // with a leading "./". Normalize.
                let raw_url = &html[open_pos + 1..close_pos];
                let asset_key = raw_url.trim_start_matches("./").to_string();
                if !asset_key.starts_with("assets/") {
                    // Prefix wasn't actually our asset reference (e.g. some
                    // unrelated word containing 'assets/'). Skip.
                    out.push_str(&html[i..p + 7]);
                    i = p + 7;
                    continue;
                }
                // Look up in the files map.
                if let Some(payload) = files.get(&asset_key) {
                    // MIME priority: magic-byte sniff of the actual bytes
                    // (most reliable; immune to agent mis-extensioning a
                    // JPEG as `.png`) → fall back to path extension. Text
                    // payloads (SVG, JSON, CSS) skip the sniff and use the
                    // extension since they don't have binary magic.
                    let mime = if is_likely_base64(payload) {
                        magic_bytes_mime(payload).unwrap_or_else(|| mime_for_path(&asset_key))
                    } else {
                        mime_for_path(&asset_key)
                    };
                    let data_uri = if is_likely_base64(payload) {
                        format!("data:{};base64,{}", mime, payload)
                    } else if is_text_mime(mime) {
                        // Inline text payloads via percent-encoding so they
                        // round-trip cleanly.
                        format!("data:{};utf8,{}", mime, percent_encode(payload))
                    } else {
                        // Unknown raw binary: assume utf-8 string and let
                        // browser cope. This branch is unlikely.
                        format!("data:{};utf8,{}", mime, percent_encode(payload))
                    };
                    // Emit pre-quote chunk + opening quote, replacement, then continue
                    // from close position so we re-emit the closing delimiter.
                    out.push_str(&html[i..open_pos + 1]);
                    out.push_str(&data_uri);
                    i = close_pos;
                    continue;
                }
                // Not in files map: leave reference unchanged.
                out.push_str(&html[i..close_pos]);
                i = close_pos;
                continue;
            } else {
                out.push_str(&html[i..p + 1]);
                i = p + 1;
                continue;
            }
        } else {
            out.push_str(&html[i..]);
            break;
        }
    }
    out
}

/// Replace every `assets/<name>` reference with the absolute URL provided
/// in `asset_urls`. Same scanner as [`inline_assets`] — only the
/// substitution differs (URL string instead of `data:` URI). Refs missing
/// from the map are left as-is so the linter (`nf/asset-not-in-files`)
/// can still flag them.
///
/// This is the path used by `canvas.video.get_composition` (Phase 2) so
/// the response stays small (KB-class) instead of inlining every binary
/// asset as a data URI (MB-class, blew the NM 1 MB cap).
///
/// Keys in `asset_urls` are the asset NAMES — the part after `assets/`.
/// E.g. for a `<img src="assets/hero.png">` reference the lookup key is
/// `"hero.png"`.
pub fn rewrite_assets_to_urls(html: &str, asset_urls: &HashMap<String, String>) -> String {
    if asset_urls.is_empty() || !html.contains("assets/") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(rel) = bytes[i..].iter().position(|&b| b == b'a') {
            let p = i + rel;
            if p + 7 <= bytes.len() && &bytes[p..p + 7] == b"assets/" {
                let (open_pos, open_kind) = match find_opening(bytes, p) {
                    Some(v) => v,
                    None => {
                        out.push_str(&html[i..p + 7]);
                        i = p + 7;
                        continue;
                    }
                };
                let close_pos = match find_closing(bytes, p + 7, open_kind) {
                    Some(v) => v,
                    None => {
                        out.push_str(&html[i..p + 7]);
                        i = p + 7;
                        continue;
                    }
                };
                let raw_url = &html[open_pos + 1..close_pos];
                let asset_key = raw_url.trim_start_matches("./").to_string();
                let asset_name = match asset_key.strip_prefix("assets/") {
                    Some(name) => name.to_string(),
                    None => {
                        out.push_str(&html[i..p + 7]);
                        i = p + 7;
                        continue;
                    }
                };
                if let Some(url) = asset_urls.get(&asset_name) {
                    out.push_str(&html[i..open_pos + 1]);
                    out.push_str(url);
                    i = close_pos;
                    continue;
                }
                out.push_str(&html[i..close_pos]);
                i = close_pos;
                continue;
            } else {
                out.push_str(&html[i..p + 1]);
                i = p + 1;
                continue;
            }
        } else {
            out.push_str(&html[i..]);
            break;
        }
    }
    out
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Open {
    DoubleQuote, // "
    SingleQuote, // '
    UrlParen,    // url( ... )  with no quote
}

/// Walk backward from `p` to find the opening delimiter that starts the
/// URL value. Returns the byte offset of the delimiter and its kind.
fn find_opening(bytes: &[u8], p: usize) -> Option<(usize, Open)> {
    // Scan back at most 16 bytes (typical context: src=").
    let start = p.saturating_sub(16);
    let mut q: Option<(usize, Open)> = None;
    for j in (start..p).rev() {
        match bytes[j] {
            b'"' => {
                q = Some((j, Open::DoubleQuote));
                break;
            }
            b'\'' => {
                q = Some((j, Open::SingleQuote));
                break;
            }
            b'(' => {
                q = Some((j, Open::UrlParen));
                break;
            }
            // Whitespace continues the scan; `>` `<` `;` mean we never
            // entered a value context.
            b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'.' => continue,
            _ => continue,
        }
    }
    q
}

fn find_closing(bytes: &[u8], from: usize, kind: Open) -> Option<usize> {
    let target: u8 = match kind {
        Open::DoubleQuote => b'"',
        Open::SingleQuote => b'\'',
        Open::UrlParen => b')',
    };
    let limit = (from + 4096).min(bytes.len());
    for j in from..limit {
        if bytes[j] == target {
            return Some(j);
        }
        // URLs don't contain newlines unescaped — if we hit one before the
        // closing quote, treat as malformed and bail.
        if bytes[j] == b'\n' || bytes[j] == b'<' {
            return None;
        }
    }
    None
}

pub fn mime_for_path(p: &str) -> &'static str {
    let ext = p.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "json" => "application/json",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Detect the true MIME type of a base64-encoded binary payload by
/// decoding the first 16 bytes and matching common magic numbers.
/// Returns `None` for unknown / text payloads.
///
/// This protects the inliner from agent path-extension mistakes — an
/// agent that saves a JPEG as `foo.png` would otherwise produce
/// `data:image/png;base64,<jpeg-bytes>` and the browser would refuse it.
pub fn magic_bytes_mime(payload_b64: &str) -> Option<&'static str> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let head: String = payload_b64
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(24)
        .collect();
    if head.len() < 8 {
        return None;
    }
    let padded = match head.len() % 4 {
        0 => head,
        n => format!("{head}{}", "=".repeat(4 - n)),
    };
    let bytes = STANDARD.decode(padded.as_bytes()).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    Some(match bytes.as_slice() {
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x47, 0x49, 0x46, 0x38, ..] => "image/gif",
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => "image/webp",
        [_, _, _, _, 0x66, 0x74, 0x79, 0x70, ..] => "video/mp4",
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x41, 0x56, 0x45, ..] => "audio/wav",
        [0x49, 0x44, 0x33, ..] => "audio/mpeg",
        [0xFF, 0xFB, ..] | [0xFF, 0xF3, ..] | [0xFF, 0xF2, ..] => "audio/mpeg",
        [0x4F, 0x67, 0x67, 0x53, ..] => "audio/ogg",
        [b'w', b'O', b'F', b'2', ..] => "font/woff2",
        [b'w', b'O', b'F', b'F', ..] => "font/woff",
        _ => return None,
    })
}

pub fn is_likely_base64(s: &str) -> bool {
    // Heuristic: base64 strings contain only A-Z a-z 0-9 + / = and are
    // at least ~16 chars (a real binary asset). Reject anything that has
    // whitespace or `<`/`{` (clearly text/HTML/JSON).
    if s.len() < 16 {
        return false;
    }
    if s.bytes()
        .any(|b| b == b'<' || b == b'{' || b == b' ' || b == b'\n')
    {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

fn is_text_mime(m: &str) -> bool {
    m.starts_with("text/")
        || m == "image/svg+xml"
        || m == "application/json"
        || m == "application/javascript"
}

/// Minimal percent-encoder for the small set of chars that break inside
/// `data:<mime>;utf8,...` contexts: `#`, `%`, `<`, `>`, `"`, plus literal
/// newlines. Other ASCII passes through.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'#' => out.push_str("%23"),
            b'%' => out.push_str("%25"),
            b'<' => out.push_str("%3C"),
            b'>' => out.push_str("%3E"),
            b'"' => out.push_str("%22"),
            b'\n' => out.push_str("%0A"),
            b'\r' => out.push_str("%0D"),
            _ => out.push(b as char),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // A 1×1 transparent PNG, base64-encoded.
    const PNG_1x1: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn passthrough_when_no_assets() {
        let html = r#"<html><body><h1>hi</h1></body></html>"#;
        let f = files(&[("assets/foo.png", PNG_1x1)]);
        assert_eq!(inline_assets(html, &f), html);
    }

    #[test]
    fn inlines_img_src() {
        let html = r#"<img src="assets/hero.png">"#;
        let f = files(&[("assets/hero.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/png;base64,iVBORw0K"), "got: {out}");
        assert!(!out.contains("assets/hero.png"));
    }

    #[test]
    fn handles_dot_slash_prefix() {
        let html = r#"<img src="./assets/hero.png">"#;
        let f = files(&[("assets/hero.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/png;base64,"));
    }

    #[test]
    fn inlines_video_and_audio() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        // mp4 ftyp box (bytes 4-7 = "ftyp")
        let mp4_bytes = vec![
            0u8, 0, 0, 0x18, 0x66, 0x74, 0x79, 0x70, b'i', b's', b'o', b'm',
        ];
        // mp3 ID3v2 header
        let mp3_bytes = vec![0x49u8, 0x44, 0x33, 0x04, 0, 0, 0, 0, 0, 0];
        let mp4_b64 = STANDARD.encode(&mp4_bytes);
        let mp3_b64 = STANDARD.encode(&mp3_bytes);
        let html = r#"<video src="assets/clip.mp4"></video><audio src="assets/n.mp3"></audio>"#;
        let f = files(&[
            ("assets/clip.mp4", &mp4_b64 as &str),
            ("assets/n.mp3", &mp3_b64 as &str),
        ]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:video/mp4;base64,"), "got: {out}");
        assert!(out.contains("data:audio/mpeg;base64,"), "got: {out}");
    }

    #[test]
    fn inlines_css_url() {
        let html = r#"<style>.x { background: url(assets/bg.png); }</style>"#;
        let f = files(&[("assets/bg.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/png;base64,"), "got: {out}");
    }

    #[test]
    fn inlines_css_url_with_quotes() {
        let html = r#"<style>.x { background: url("assets/bg.png"); }</style>"#;
        let f = files(&[("assets/bg.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/png;base64,"), "got: {out}");
    }

    #[test]
    fn missing_asset_left_unchanged() {
        let html = r#"<img src="assets/missing.png">"#;
        let f = files(&[]);
        let out = inline_assets(html, &f);
        assert!(out.contains("assets/missing.png"));
    }

    #[test]
    fn external_url_passthrough() {
        let html = r#"<img src="https://example.com/x.png"><img src="assets/y.png">"#;
        let f = files(&[("assets/y.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("https://example.com/x.png"));
        assert!(out.contains("data:image/png;base64,"));
    }

    #[test]
    fn svg_inlined_as_text() {
        let html = r#"<img src="assets/icon.svg">"#;
        let svg = "<svg xmlns='http://www.w3.org/2000/svg'><circle r='5'/></svg>";
        let f = files(&[("assets/icon.svg", svg)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/svg+xml;utf8,"), "got: {out}");
        assert!(out.contains("%3Csvg"), "should percent-encode <"); // <
    }

    #[test]
    fn multiple_refs_same_asset() {
        let html = r#"<img src="assets/x.png"><img src="assets/x.png">"#;
        let f = files(&[("assets/x.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        let n = out.matches("data:image/png;base64,").count();
        assert_eq!(n, 2);
    }

    #[test]
    fn single_quotes_work() {
        let html = r#"<img src='assets/y.png'>"#;
        let f = files(&[("assets/y.png", PNG_1x1)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/png;base64,"));
    }

    #[test]
    fn jpeg_saved_as_png_is_inlined_as_jpeg() {
        // FF D8 FF E0 = JPEG/JFIF — caller mis-extensioned as `.png`.
        // Without magic-byte sniffing, the inliner would emit
        // `data:image/png;base64,<jpeg bytes>` and the browser would
        // refuse to render. With sniffing, we emit `data:image/jpeg;...`.
        use base64::{engine::general_purpose::STANDARD, Engine};
        let jpeg_bytes = vec![
            0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01,
        ];
        let b64 = STANDARD.encode(&jpeg_bytes);
        let html = r#"<img src="assets/foo.png">"#;
        let f = files(&[("assets/foo.png", &b64 as &str)]);
        let out = inline_assets(html, &f);
        assert!(out.contains("data:image/jpeg;base64,"), "got: {out}");
        assert!(!out.contains("data:image/png;base64,"));
    }
}
