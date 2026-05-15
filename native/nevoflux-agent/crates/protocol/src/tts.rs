//! TTS subsystem protocol types.
//!
//! Backs the three TTS tools per umbrella spec §7:
//! - `tts_synthesize_api`   (P5b-1, ElevenLabs HTTP) — wire types here.
//! - `tts_synthesize_local` (P5b-2, Kokoro local ONNX) — same `SynthesizeRequest`.
//! - `tts_transcribe`       (P5b-3, Whisper local ONNX) — separate request type.
//!
//! Auth/config is server-side (daemon reads `~/.config/nevoflux/config.toml`);
//! the LLM-facing tool args don't carry secrets.

use serde::{Deserialize, Serialize};

/// `tts_synthesize_*` request.
///
/// `composition_id` is optional: when present, the daemon writes the
/// synthesized audio into the artifact's files map as `narration.<ext>`
/// (mp3 for ElevenLabs, wav for Kokoro). When absent, audio bytes are
/// returned base64-encoded for the LLM to forward where it likes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesizeRequest {
    /// Text to speak. Must be ≤ 600 chars (~60 s of speech) per
    /// umbrella §7.8 hard limit.
    pub text: String,
    /// Voice identifier. Format depends on backend:
    /// - ElevenLabs: 20-char voice ID (e.g. `21m00Tcm4TlvDq8ikWAM`)
    /// - Kokoro: short tag (e.g. `af`, `am`, `zf`, `zm`)
    /// When omitted, daemon falls back to the backend's default voice
    /// from config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
    /// Model identifier (ElevenLabs only — e.g. `eleven_multilingual_v2`).
    /// Kokoro ignores this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// If set, daemon writes the audio bytes into this artifact's files
    /// map as `narration.<ext>` (where ext = mp3 for ElevenLabs, wav for
    /// Kokoro). The audio_b64 field in the response is also populated for
    /// callers that want both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_id: Option<String>,
}

/// `tts_synthesize_*` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeResponse {
    /// Base64-encoded audio bytes (MP3 or WAV depending on backend).
    pub audio_b64: String,
    /// Audio mime type — `audio/mpeg` for ElevenLabs MP3, `audio/wav`
    /// for Kokoro WAV.
    pub mime_type: String,
    /// Estimated duration in seconds. May be slightly off from actual
    /// playback duration; the renderer should use the artifact's
    /// `<audio data-duration>` attribute as the source of truth.
    pub duration_sec: f32,
    /// Voice ID actually used (after default-fallback resolution).
    pub voice_id: String,
    /// File path written into the artifact's files map, if
    /// `composition_id` was provided. None otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrote_to_files: Option<String>,
}

/// One TTS voice descriptor — listed by `tts_voices` (future P5b-2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    /// `"male"` / `"female"` / `"neutral"`.
    pub gender: String,
    /// BCP-47 language code (`"en-US"`, `"zh-CN"`, etc.).
    pub language: String,
    /// `"elevenlabs"` / `"kokoro"`.
    pub backend: String,
}

/// `tts_transcribe` request (P5b-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscribeRequest {
    /// Either `audio_b64` (raw audio bytes) or `composition_id` + `file_path`
    /// (read audio from artifact's files map). Caller MUST set exactly one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Whisper model size: `"tiny"` / `"base"` / `"small"` / `"medium"`.
    /// Defaults to `"base"` if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_size: Option<String>,
}

/// `tts_transcribe` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResponse {
    /// Full transcript text.
    pub text: String,
    /// Per-segment timestamps (millisecond precision).
    pub segments: Vec<TranscribeSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeSegment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_request_minimal_deserializes() {
        let json = r#"{"text":"hello"}"#;
        let req: SynthesizeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.text, "hello");
        assert!(req.voice_id.is_none());
        assert!(req.composition_id.is_none());
    }

    #[test]
    fn synthesize_request_full_deserializes() {
        let json = r#"{
            "text":"Welcome to NevoFlux",
            "voice_id":"21m00Tcm4TlvDq8ikWAM",
            "model_id":"eleven_multilingual_v2",
            "composition_id":"comp-abc"
        }"#;
        let req: SynthesizeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.voice_id.as_deref(), Some("21m00Tcm4TlvDq8ikWAM"));
        assert_eq!(req.composition_id.as_deref(), Some("comp-abc"));
    }

    #[test]
    fn synthesize_request_rejects_unknown_field() {
        let json = r#"{"text":"x","emotion":"happy"}"#;
        assert!(serde_json::from_str::<SynthesizeRequest>(json).is_err());
    }

    #[test]
    fn synthesize_response_round_trip() {
        let resp = SynthesizeResponse {
            audio_b64: "AAAA".into(),
            mime_type: "audio/mpeg".into(),
            duration_sec: 12.4,
            voice_id: "Rachel".into(),
            wrote_to_files: Some("narration.mp3".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: SynthesizeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.duration_sec, 12.4);
        assert_eq!(back.wrote_to_files.as_deref(), Some("narration.mp3"));
    }
}
