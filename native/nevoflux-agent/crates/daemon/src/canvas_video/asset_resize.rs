//! Image asset downscaling for `canvas_attach_asset`.
//!
//! When the agent attaches a hero image, every rendered frame ships
//! that bitmap through `drawSnapshot`, and the render-time inliner
//! turns the asset into a `data:` URI inside the composition HTML.
//! Both bytes-on-the-wire AND on-screen pixel count drive per-frame
//! cost. A 6.45 MB PNG (2816×1536) won't reach the render tab at all
//! — the bridge protocol or iframe srcdoc rejects payloads that big.
//!
//! Strategy:
//! - Decode with `image::guess_format` + `load_from_memory_with_format`.
//! - Trigger resize when EITHER:
//!     1. byte size > BYTE_THRESHOLD (500 KB), OR
//!     2. max(w, h) > max(stage_w, stage_h)
//! - When resizing:
//!     - Has alpha → scale to fit stage_max, keep PNG (preserves
//!       transparency for logos / cutouts).
//!     - No alpha → scale to fit stage_max, convert to JPEG q=85
//!       (5-30× smaller than re-encoded PNG for photographic content;
//!       Hello Kitty merch photos, screenshots, hero shots).
//! - Resize uses Lanczos3 (sharper than the default Triangle filter
//!   for content that gets scrutinized).
//! - Any failure (unsupported format, decode error, encode error) is
//!   non-fatal — return the original payload and let the caller proceed.
//!   `attach_asset` MUST never fail because of resize logic.

use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ColorType, DynamicImage, ImageFormat};
use std::io::Cursor;

/// When an image's raw bytes exceed this threshold, force a re-encode
/// even if dimensions are within stage_max. Photo-quality PNGs (which
/// the agent often gets from product pages) routinely run 5-10 MB at
/// 2-3 MP — much smaller as JPEG, much smaller still after downscale.
const BYTE_THRESHOLD: usize = 500 * 1024;

/// JPEG quality when converting opaque content. q=85 is the standard
/// "looks identical to source" point; smaller produces visible artifacts
/// on solid color blocks (which a Hello Kitty / merch image has lots of).
const JPEG_QUALITY: u8 = 85;

/// Outcome of a resize attempt, surfaced for logging only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResizeOutcome {
    /// Image was decoded, downscaled, and re-encoded.
    Resized {
        from: (u32, u32),
        to: (u32, u32),
        original_bytes: usize,
        new_bytes: usize,
        format: ImageFormat,
    },
    /// Image was decoded but didn't need resizing (already small).
    AlreadySmall { dims: (u32, u32), bytes: usize },
    /// Couldn't decode (unsupported format, malformed, etc.). Caller
    /// stores the original payload as-is.
    NotAnImage,
    /// Decode succeeded, encode failed. Caller stores the original.
    EncodeFailed { reason: String },
}

/// Maybe downscale a base64-encoded image to fit within a stage of
/// `stage_w × stage_h`. Thin wrapper around `maybe_resize_bytes` —
/// decodes input base64, runs the resize, re-encodes output. Use the
/// `_bytes` form if your caller already has raw bytes (e.g. just read
/// from disk) to avoid a wasteful encode/decode round-trip.
///
/// `stage_w` / `stage_h` come from `composition.meta.json` `spec.width`
/// / `spec.height` — the actual stage dimensions for this composition.
pub fn maybe_resize(payload_b64: &str, stage_w: u32, stage_h: u32) -> (String, ResizeOutcome) {
    let bytes = match STANDARD.decode(payload_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return (payload_b64.to_string(), ResizeOutcome::NotAnImage),
    };
    let (out_bytes, outcome) = maybe_resize_bytes(&bytes, stage_w, stage_h);
    match outcome {
        ResizeOutcome::Resized { .. } => (STANDARD.encode(&out_bytes), outcome),
        // For every other outcome the caller's original payload is
        // semantically unchanged — return it verbatim instead of
        // re-encoding, so we don't perturb whitespace / padding etc.
        _ => (payload_b64.to_string(), outcome),
    }
}

/// Raw-bytes core of the resize logic. Takes input bytes, returns
/// `(possibly new bytes, outcome)`. When `outcome` is anything other
/// than `Resized` the returned `Vec<u8>` is empty (caller must use its
/// original bytes). This shape lets paths that already have raw bytes
/// from disk (e.g., `promote_image_local_files_to_attachments`) avoid
/// a base64 round-trip on multi-megabyte images.
pub fn maybe_resize_bytes(
    bytes: &[u8],
    stage_w: u32,
    stage_h: u32,
) -> (Vec<u8>, ResizeOutcome) {
    let original_bytes = bytes.len();

    // Sniff format from magic bytes.
    let format = match image::guess_format(bytes) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), ResizeOutcome::NotAnImage),
    };
    let img = match image::load_from_memory_with_format(bytes, format) {
        Ok(i) => i,
        Err(_) => return (Vec::new(), ResizeOutcome::NotAnImage),
    };

    let (w, h) = (img.width(), img.height());
    let stage_max = stage_w.max(stage_h);
    let img_max = w.max(h);
    let has_alpha = has_alpha_channel(img.color());

    // Trigger conditions — either oversized dimensions OR oversized
    // bytes. Skipping the byte check let a 2816×1536 / 6.45 MB PNG
    // through unchanged because dimensions sat just inside an old
    // 2× headroom rule; the resulting data: URI then broke the render
    // tab. Two predicates catch both axes.
    let oversize_dims = img_max > stage_max;
    let oversize_bytes = original_bytes > BYTE_THRESHOLD;
    if !oversize_dims && !oversize_bytes {
        return (
            Vec::new(),
            ResizeOutcome::AlreadySmall { dims: (w, h), bytes: original_bytes },
        );
    }

    let new_img = if oversize_dims {
        img.resize(stage_max, stage_max, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let target_format = if has_alpha {
        ImageFormat::Png
    } else if format == ImageFormat::Gif {
        ImageFormat::Gif
    } else {
        ImageFormat::Jpeg
    };

    let mut out = Vec::with_capacity(original_bytes / 2);
    let encode_result = match target_format {
        ImageFormat::Jpeg => {
            let rgb = new_img.to_rgb8();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
            encoder.encode_image(&DynamicImage::ImageRgb8(rgb))
        }
        ImageFormat::Png => new_img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png),
        ImageFormat::Gif => new_img.write_to(&mut Cursor::new(&mut out), ImageFormat::Gif),
        _ => new_img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png),
    };
    if let Err(e) = encode_result {
        return (
            Vec::new(),
            ResizeOutcome::EncodeFailed { reason: e.to_string() },
        );
    }
    let new_bytes = out.len();
    if new_bytes >= original_bytes {
        return (
            Vec::new(),
            ResizeOutcome::AlreadySmall { dims: (w, h), bytes: original_bytes },
        );
    }
    (
        out,
        ResizeOutcome::Resized {
            from: (w, h),
            to: (new_img.width(), new_img.height()),
            original_bytes,
            new_bytes,
            format: target_format,
        },
    )
}

/// Whether a colour type carries an alpha channel. Matches every variant
/// the `image` crate exposes; conservative — when in doubt, treat as
/// alpha-bearing so we don't accidentally drop transparency.
fn has_alpha_channel(c: ColorType) -> bool {
    matches!(
        c,
        ColorType::La8
            | ColorType::La16
            | ColorType::Rgba8
            | ColorType::Rgba16
            | ColorType::Rgba32F
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn solid_rgb_jpeg(w: u32, h: u32) -> String {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
            .unwrap();
        STANDARD.encode(&buf)
    }

    fn solid_rgba_png(w: u32, h: u32) -> String {
        use image::Rgba;
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(w, h, |_, _| Rgba([200, 50, 50, 255]));
        let mut buf = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        STANDARD.encode(&buf)
    }

    /// Per-pixel pseudo-random RGB so PNG predictors can't compress
    /// well and JPEG encoders have to spend bits — simulates real
    /// photographic content. Uses wrapping arithmetic throughout to
    /// avoid u32 overflow at large dimensions.
    fn noisy_rgb_at(x: u32, y: u32) -> Rgb<u8> {
        let r = x.wrapping_mul(2654435761).wrapping_add(y.wrapping_mul(40503)) as u8;
        let g = y.wrapping_mul(2246822519).wrapping_add(x.wrapping_mul(16807)) as u8;
        let b = (x ^ y).wrapping_mul(1597334677) as u8;
        Rgb([r, g, b])
    }

    /// A photo-like RGB JPEG that compresses to many KB even at small
    /// dimensions — useful for byte-size threshold tests.
    fn noisy_rgb_jpeg(w: u32, h: u32) -> String {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, noisy_rgb_at);
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
            .unwrap();
        STANDARD.encode(&buf)
    }

    /// PNG-encoded version of the noisy RGB pattern — exercises the
    /// "PNG → JPEG" conversion path on realistic photographic content.
    fn noisy_rgb_png(w: u32, h: u32) -> String {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, noisy_rgb_at);
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        STANDARD.encode(&buf)
    }

    #[test]
    fn small_image_returns_unchanged() {
        // 800×600 solid color JPEG: well under stage_max AND under
        // BYTE_THRESHOLD ⇒ no resize.
        let b64 = solid_rgb_jpeg(800, 600);
        let (out, outcome) = maybe_resize(&b64, 1080, 1920);
        assert_eq!(out, b64);
        assert!(matches!(outcome, ResizeOutcome::AlreadySmall { .. }));
    }

    #[test]
    fn oversize_jpeg_gets_downscaled_to_stage_max() {
        let b64 = solid_rgb_jpeg(4000, 6000);
        let (out, outcome) = maybe_resize(&b64, 1080, 1920);
        assert_ne!(out, b64);
        match outcome {
            ResizeOutcome::Resized { from, to, original_bytes, new_bytes, format } => {
                assert_eq!(from, (4000, 6000));
                assert!(to.0 <= 1920 && to.1 <= 1920, "to {:?} should fit in stage_max box", to);
                assert!(to.0 == 1920 || to.1 == 1920, "one side should equal stage_max: {:?}", to);
                assert!(new_bytes < original_bytes, "should shrink, got {new_bytes} >= {original_bytes}");
                assert_eq!(format, ImageFormat::Jpeg, "opaque should stay JPEG");
            }
            other => panic!("expected Resized, got {other:?}"),
        }
    }

    #[test]
    fn png_with_alpha_resize_preserves_alpha() {
        // 3000×3000 RGBA PNG → must keep PNG to preserve transparency.
        let b64 = solid_rgba_png(3000, 3000);
        let (out, outcome) = maybe_resize(&b64, 1000, 1000);
        match outcome {
            ResizeOutcome::Resized { format, .. } => {
                assert_eq!(format, ImageFormat::Png, "alpha-bearing must stay PNG");
            }
            other => panic!("expected Resized, got {other:?}"),
        }
        let raw = STANDARD.decode(&out).unwrap();
        let img = image::load_from_memory(&raw).unwrap();
        assert!(img.color().has_alpha(), "PNG should round-trip with alpha");
    }

    #[test]
    fn opaque_png_oversize_converts_to_jpeg() {
        // Noisy RGB PNG at 3000×2000: source compresses poorly (high
        // entropy), so re-encoding as JPEG q=85 at downscaled dims is
        // dramatically smaller. Resize triggered by oversize dimensions.
        let b64 = noisy_rgb_png(3000, 2000);
        let original_bytes = STANDARD.decode(&b64).unwrap().len();
        let (_out, outcome) = maybe_resize(&b64, 1080, 1920);
        match outcome {
            ResizeOutcome::Resized { format, new_bytes, .. } => {
                assert_eq!(format, ImageFormat::Jpeg, "opaque PNG → JPEG");
                assert!(new_bytes < original_bytes / 5,
                    "JPEG should be much smaller ({new_bytes} vs {original_bytes})");
            }
            other => panic!("expected Resized, got {other:?}"),
        }
    }

    #[test]
    fn byte_threshold_triggers_when_dimensions_already_fit() {
        // Source PNG: noisy content at moderate dimensions. PNG of
        // high-entropy content blows past BYTE_THRESHOLD even at
        // dimensions inside stage_max. Re-encoding as JPEG q=85 shrinks
        // dramatically.
        let b64 = noisy_rgb_png(1500, 1500);
        let raw_size = STANDARD.decode(&b64).unwrap().len();
        assert!(
            raw_size > BYTE_THRESHOLD,
            "test fixture only {raw_size} bytes — needs > {BYTE_THRESHOLD}"
        );

        // Stage 1920×1920: dims 1500×1500 fit. Byte trigger fires alone.
        let (_out, outcome) = maybe_resize(&b64, 1920, 1920);
        match outcome {
            ResizeOutcome::Resized { from, to, new_bytes, format, .. } => {
                assert_eq!(from, (1500, 1500));
                assert_eq!(to, (1500, 1500), "dims preserved when only byte trigger fires");
                assert_eq!(format, ImageFormat::Jpeg, "opaque oversize PNG → JPEG");
                assert!(new_bytes < raw_size / 2,
                    "PNG→JPEG should shrink ≥2×, got {new_bytes} vs {raw_size}");
            }
            other => panic!("expected Resized (byte trigger), got {other:?}"),
        }
    }

    #[test]
    fn non_image_payload_returns_unchanged() {
        let b64 = STANDARD.encode(b"this is some text, not an image");
        let (out, outcome) = maybe_resize(&b64, 1920, 1080);
        assert_eq!(out, b64);
        assert!(matches!(outcome, ResizeOutcome::NotAnImage));
    }

    #[test]
    fn malformed_base64_returns_unchanged() {
        let bad = "this~is~not~base64!!!";
        let (out, outcome) = maybe_resize(bad, 1920, 1080);
        assert_eq!(out, bad);
        assert!(matches!(outcome, ResizeOutcome::NotAnImage));
    }

    #[test]
    fn dims_over_stage_max_with_noisy_content_resizes() {
        // Use noisy content so re-encode genuinely shrinks bytes
        // (solid-color JPEGs sometimes round-trip larger and the guard
        // legitimately reverts to AlreadySmall — that's correct
        // behavior, just not what this test wants to exercise).
        let b64 = noisy_rgb_jpeg(2400, 1350);
        let (_out, outcome) = maybe_resize(&b64, 1920, 1080);
        match outcome {
            ResizeOutcome::Resized { from, to, .. } => {
                assert_eq!(from, (2400, 1350));
                assert!(to.0 <= 1920 && to.1 <= 1920);
                // Longer side caps at stage_max=1920.
                assert!(to.0 == 1920 || to.1 == 1920, "to {to:?}");
            }
            other => panic!("expected Resized for oversized dims, got {other:?}"),
        }
    }

    #[test]
    fn aspect_ratio_preserved_after_resize() {
        // Wide image: 8000×2000 (4:1). Stage 1920×1080. After resize:
        // longer side becomes 1920, shorter side proportional.
        let b64 = solid_rgb_jpeg(8000, 2000);
        let (_out, outcome) = maybe_resize(&b64, 1920, 1080);
        match outcome {
            ResizeOutcome::Resized { to, .. } => {
                let aspect_in = 8000.0_f32 / 2000.0;
                let aspect_out = to.0 as f32 / to.1 as f32;
                assert!((aspect_in - aspect_out).abs() < 0.01,
                    "aspect ratio drift: {aspect_in} vs {aspect_out} (to={to:?})");
                assert_eq!(to.0, 1920);
            }
            other => panic!("expected Resized, got {other:?}"),
        }
    }

    #[test]
    fn realistic_hellokitty_case_resizes() {
        // Reproduces the user's failure case: a 6+ MB-class PNG at
        // 2816×1536 (the dimensions logged by canvas_attach_asset on
        // the actual broken render run). Earlier 2× headroom rule let
        // this pass through; new rule must shrink it dramatically.
        let b64 = noisy_rgb_png(2816, 1536);
        let original_bytes = STANDARD.decode(&b64).unwrap().len();
        let (_out, outcome) = maybe_resize(&b64, 1920, 1080);
        match outcome {
            ResizeOutcome::Resized { from, to, new_bytes, format, .. } => {
                assert_eq!(from, (2816, 1536));
                assert!(to.0 <= 1920 && to.1 <= 1920, "should fit stage_max: {to:?}");
                assert_eq!(format, ImageFormat::Jpeg, "opaque photo PNG → JPEG");
                // Use division to avoid u32/u64 multiplication concerns.
                assert!(new_bytes < original_bytes / 5,
                    "should be ≥5× smaller, got {new_bytes} vs {original_bytes}");
            }
            other => panic!("expected Resized for realistic case, got {other:?}"),
        }
    }
}
