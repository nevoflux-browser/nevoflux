//! Whisper local transcription (P5b-3) — scaffolding.
//!
//! Mirrors the Kokoro scaffold (`crates/daemon/src/tts/kokoro.rs`):
//! returns a clear `ConfigMissing` until the `nevoflux-tts` workspace
//! crate ships ONNX inference.
//!
//! Target pipeline (umbrella spec §7.4):
//!   audio → ffmpeg-sidecar (16 kHz mono PCM) → Whisper-tiny ONNX
//!   inference (temperature=0, beam=5) → segment list with millisecond
//!   timestamps. Used by P5c auto-captions.

use crate::config::WhisperConfig;
use crate::tts::error::TtsError;
use nevoflux_protocol::tts::{TranscribeRequest, TranscribeResponse};

/// Transcribe audio via local Whisper ONNX. Currently scaffolding —
/// returns a clear ConfigMissing pointing at the config + setup steps.
pub async fn transcribe(
    cfg: &WhisperConfig,
    req: &TranscribeRequest,
) -> Result<TranscribeResponse, TtsError> {
    // Validate request shape: caller must provide either audio_b64 or
    // (composition_id + file_path) — not both, not neither.
    let has_inline = req.audio_b64.is_some();
    let has_artifact = req.composition_id.is_some() && req.file_path.is_some();
    if has_inline == has_artifact {
        return Err(TtsError::InvalidRequest(
            "tts_transcribe: must provide exactly one of `audio_b64` OR \
             (`composition_id` + `file_path`)"
                .into(),
        ));
    }

    let model_path = cfg
        .model_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            TtsError::ConfigMissing(
                "Whisper transcription not configured. Set \
                 `[tts.whisper] model_path` in ~/.config/nevoflux/config.toml \
                 after downloading whisper-{tiny,base}.onnx to \
                 ~/.cache/nevoflux/models/. Auto-captions in P5c depend \
                 on this backend."
                    .into(),
            )
        })?;

    Err(TtsError::ConfigMissing(format!(
        "Whisper transcription inference not yet wired up (model={model_path}). \
         Ships alongside Kokoro in the nevoflux-tts crate milestone."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req_inline(b64: &str) -> TranscribeRequest {
        TranscribeRequest {
            audio_b64: Some(b64.into()),
            composition_id: None,
            file_path: None,
            model_size: None,
        }
    }
    fn req_artifact(comp: &str, path: &str) -> TranscribeRequest {
        TranscribeRequest {
            audio_b64: None,
            composition_id: Some(comp.into()),
            file_path: Some(path.into()),
            model_size: None,
        }
    }

    #[tokio::test]
    async fn rejects_neither_input() {
        let cfg = WhisperConfig::default();
        let r = TranscribeRequest {
            audio_b64: None,
            composition_id: None,
            file_path: None,
            model_size: None,
        };
        let err = transcribe(&cfg, &r).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn rejects_both_inputs() {
        let cfg = WhisperConfig::default();
        let r = TranscribeRequest {
            audio_b64: Some("AAAA".into()),
            composition_id: Some("comp-x".into()),
            file_path: Some("narration.mp3".into()),
            model_size: None,
        };
        let err = transcribe(&cfg, &r).await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn missing_model_path_yields_config_missing_inline() {
        let cfg = WhisperConfig::default();
        let err = transcribe(&cfg, &req_inline("AAAA")).await.unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, TtsError::ConfigMissing(_)), "got: {msg}");
        assert!(msg.contains("Whisper") && msg.contains("model_path"));
    }

    #[tokio::test]
    async fn missing_model_path_yields_config_missing_artifact() {
        let cfg = WhisperConfig::default();
        let err = transcribe(&cfg, &req_artifact("comp-x", "narration.mp3"))
            .await
            .unwrap_err();
        assert!(matches!(err, TtsError::ConfigMissing(_)));
    }

    #[tokio::test]
    async fn model_set_but_inference_not_wired() {
        let cfg = WhisperConfig {
            model_path: Some("/tmp/fake-whisper.onnx".into()),
            default_size: Some("base".into()),
        };
        let err = transcribe(&cfg, &req_inline("AAAA")).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not yet wired"), "{msg}");
    }
}
