//! Kokoro local TTS (P5b-2) — scaffolding.
//!
//! The umbrella spec (§7.2) commits to Kokoro-82M ONNX inference via
//! `ort-rs` with G2P phonemization, voice-bank lookup, and WAV encoding.
//! That stack lands as a dedicated `nevoflux-tts` workspace crate in a
//! follow-up session — pulling `ort` + a phonemizer into the daemon
//! tree without an actual model file present makes for unverifiable
//! changes. Until that crate ships, this module provides:
//!
//! 1. The dispatch entry point `synthesize_local` that the daemon's
//!    three tool surfaces (direct API / MCP / ACP) all call into.
//! 2. A clear `ConfigMissing` error pointing the user at the download
//!    + config steps, so the LLM can surface useful guidance instead
//!    of "internal error".
//!
//! When the real backend lands, `synthesize_local` will resolve the
//! ONNX session lazily on first call and cache it in a `OnceCell`.
//! The error path below is the contract that gates that work.
//!
//! See umbrella spec §7.2 for the target pipeline:
//!   text cleanup → G2P → Kokoro ONNX inference → WAV encode →
//!   transcript assembly.

use crate::config::KokoroConfig;
use crate::tts::error::TtsError;
use nevoflux_protocol::tts::{SynthesizeRequest, SynthesizeResponse};

/// Synthesize speech via the local Kokoro ONNX backend.
///
/// Currently always returns `TtsError::ConfigMissing` describing how
/// to enable the backend. Real inference lands in the `nevoflux-tts`
/// workspace crate.
pub async fn synthesize_local(
    cfg: &KokoroConfig,
    req: &SynthesizeRequest,
) -> Result<SynthesizeResponse, TtsError> {
    if req.text.trim().is_empty() {
        return Err(TtsError::InvalidRequest(
            "tts_synthesize_local: text is empty".into(),
        ));
    }
    if req.text.chars().count() > super::MAX_TEXT_LEN {
        return Err(TtsError::InvalidRequest(format!(
            "tts_synthesize_local: text length {} exceeds {} char limit (≈60s of speech)",
            req.text.chars().count(),
            super::MAX_TEXT_LEN
        )));
    }

    let model_path = cfg
        .model_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            TtsError::ConfigMissing(
                "Kokoro local TTS not configured. Set `[tts.kokoro] model_path` and \
                 `voices_path` in ~/.config/nevoflux/config.toml after downloading \
                 kokoro-v1.0.int8.onnx + kokoro-voices-v1.0.bin to \
                 ~/.cache/nevoflux/models/. Until then use `tts_synthesize_api` \
                 (ElevenLabs) for narration."
                    .into(),
            )
        })?;
    let voices_path = cfg
        .voices_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            TtsError::ConfigMissing(
                "Kokoro voices file not configured. Add `[tts.kokoro] voices_path` \
                 to your config — points to kokoro-voices-v1.0.bin."
                    .into(),
            )
        })?;

    // Backend not yet implemented — even with paths configured, surface a
    // distinct error so users know it's not just a config issue.
    Err(TtsError::ConfigMissing(format!(
        "Kokoro local TTS inference not yet wired up (model={model_path}, voices={voices_path}). \
         The ONNX runtime integration ships in the next nevoflux-tts crate milestone; \
         for now use `tts_synthesize_api` (ElevenLabs)."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> SynthesizeRequest {
        SynthesizeRequest {
            text: text.into(),
            voice_id: None,
            model_id: None,
            composition_id: None,
        }
    }

    #[tokio::test]
    async fn rejects_empty_text() {
        let cfg = KokoroConfig::default();
        let err = synthesize_local(&cfg, &req("   ")).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn rejects_oversize_text() {
        let cfg = KokoroConfig::default();
        let big = "a".repeat(super::super::MAX_TEXT_LEN + 1);
        let err = synthesize_local(&cfg, &req(&big)).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn missing_model_path_yields_config_missing() {
        let cfg = KokoroConfig::default();
        let err = synthesize_local(&cfg, &req("hello")).await.unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, TtsError::ConfigMissing(_)), "got: {msg}");
        assert!(
            msg.contains("Kokoro") && msg.contains("model_path"),
            "msg should point user at config: {msg}"
        );
    }

    #[tokio::test]
    async fn paths_set_but_inference_not_wired() {
        let cfg = KokoroConfig {
            model_path: Some("/tmp/fake-kokoro.onnx".into()),
            voices_path: Some("/tmp/fake-voices.bin".into()),
            default_voice: Some("af".into()),
        };
        let err = synthesize_local(&cfg, &req("hi")).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not yet wired"),
            "msg should distinguish 'config ok but backend stub': {msg}"
        );
    }
}
