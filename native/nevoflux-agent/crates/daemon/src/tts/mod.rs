//! TTS subsystem (umbrella spec §7).
//!
//! P5b-1 ships the ElevenLabs HTTP API path; P5b-2 / P5b-3 add Kokoro
//! local + Whisper local stubs that surface ConfigMissing until the
//! `nevoflux-tts` workspace crate (ort + phonemizer + model fetch)
//! lands. Splitting that out keeps ort linker complexity isolated
//! from the daemon binary.
//!
//! Module layout:
//! - [`error`]      shared error type with mapping to `HostError` codes.
//! - [`elevenlabs`] HTTP client.
//! - [`kokoro`]     local TTS scaffold (P5b-2).
//! - [`whisper`]    local transcription scaffold (P5b-3).
//!
//! Dispatch entries (called by all three tool surfaces):
//! - [`synthesize_api`]
//! - [`synthesize_local`]
//! - [`transcribe`]

pub mod elevenlabs;
pub mod error;
pub mod kokoro;
pub mod whisper;

use crate::config::ElevenLabsConfig;
use error::TtsError;
use nevoflux_protocol::tts::{SynthesizeRequest, SynthesizeResponse};

/// Re-export so dispatch arms can call `tts::synthesize_local` /
/// `tts::transcribe` symmetric to `tts::synthesize_api`.
pub use kokoro::synthesize_local;
pub use whisper::transcribe;

/// Hard limits per umbrella §7.8.
pub const MAX_TEXT_LEN: usize = 600;

/// Synthesize speech via the ElevenLabs HTTP API. Returns audio bytes
/// + metadata; caller decides whether to also write to a composition's
/// files map (handled by the dispatch arm in agent_host / mcp_tool_executor).
///
/// Validates request shape, resolves voice_id from config defaults if
/// unspecified, and rejects oversize text upfront.
pub async fn synthesize_api(
    cfg: &ElevenLabsConfig,
    req: &SynthesizeRequest,
) -> Result<SynthesizeResponse, TtsError> {
    if req.text.trim().is_empty() {
        return Err(TtsError::InvalidRequest(
            "tts_synthesize_api: text is empty".into(),
        ));
    }
    if req.text.chars().count() > MAX_TEXT_LEN {
        return Err(TtsError::InvalidRequest(format!(
            "tts_synthesize_api: text length {} exceeds {} char limit (≈60s of speech)",
            req.text.chars().count(),
            MAX_TEXT_LEN
        )));
    }
    let api_key = cfg.api_key.as_deref().filter(|s| !s.is_empty()).ok_or(
        TtsError::ConfigMissing(
            "ELEVENLABS_API_KEY not set — add `[tts.elevenlabs] api_key = \"sk_...\"` to ~/.config/nevoflux/config.toml".into(),
        ),
    )?;
    let voice_id = req
        .voice_id
        .as_deref()
        .or(cfg.default_voice_id.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("21m00Tcm4TlvDq8ikWAM"); // Rachel — ElevenLabs catalog default
    let model_id = req
        .model_id
        .as_deref()
        .or(cfg.default_model_id.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("eleven_multilingual_v2");

    let bytes = elevenlabs::synthesize(api_key, voice_id, model_id, &req.text).await?;

    // Estimate duration: rough ratio of ~150 chars/min ≈ 2.5 chars/s for
    // English. For other languages this is off but the renderer treats
    // duration as a hint anyway.
    let duration_sec = (req.text.chars().count() as f32 / 2.5).max(0.5);

    Ok(SynthesizeResponse {
        audio_b64: base64_encode(&bytes),
        mime_type: "audio/mpeg".into(),
        duration_sec,
        voice_id: voice_id.to_string(),
        wrote_to_files: None, // dispatch layer fills this if composition_id set
    })
}

/// Standard base64 encoder (no line wrapping). Inlined to avoid pulling
/// in a base64 crate just for this one call site.
fn base64_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push_str("==");
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    let _ = write!(out, ""); // suppress unused-import lint when no write! used
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors() {
        // RFC 4648 test vectors
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[tokio::test]
    async fn rejects_empty_text() {
        let cfg = ElevenLabsConfig {
            api_key: Some("sk_test".into()),
            ..Default::default()
        };
        let req = SynthesizeRequest {
            text: "  ".into(),
            voice_id: None,
            model_id: None,
            composition_id: None,
        };
        let err = synthesize_api(&cfg, &req).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn rejects_oversize_text() {
        let cfg = ElevenLabsConfig {
            api_key: Some("sk_test".into()),
            ..Default::default()
        };
        let req = SynthesizeRequest {
            text: "a".repeat(MAX_TEXT_LEN + 1),
            voice_id: None,
            model_id: None,
            composition_id: None,
        };
        let err = synthesize_api(&cfg, &req).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn rejects_missing_api_key() {
        let cfg = ElevenLabsConfig::default();
        let req = SynthesizeRequest {
            text: "hello".into(),
            voice_id: None,
            model_id: None,
            composition_id: None,
        };
        let err = synthesize_api(&cfg, &req).await.unwrap_err();
        assert!(matches!(err, TtsError::ConfigMissing(_)));
    }
}
