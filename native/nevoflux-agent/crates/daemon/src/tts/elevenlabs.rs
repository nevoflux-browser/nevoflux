//! ElevenLabs HTTP API client.
//!
//! POST `/v1/text-to-speech/{voice_id}` with `xi-api-key` header. Returns
//! raw MP3 bytes on success. Maps HTTP status codes to `TtsError` variants
//! so dispatch layers can produce stable error codes for the LLM.
//!
//! Docs: <https://elevenlabs.io/docs/api-reference/text-to-speech/convert>

use super::error::TtsError;
use serde::Serialize;

const BASE: &str = "https://api.elevenlabs.io";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Synthesize speech via ElevenLabs. Returns raw MP3 bytes (the response
/// body — `audio/mpeg`). Caller decides whether to base64-encode for
/// LLM consumption or write to disk / artifact files.
///
/// Errors:
/// - `ConfigMissing` is the caller's responsibility (we receive a non-empty key here)
/// - `AuthFailed` on 401
/// - `RateLimit` on 429
/// - `BackendError { status, body }` on other non-2xx
/// - `Network` on transport failure
pub async fn synthesize(
    api_key: &str,
    voice_id: &str,
    model_id: &str,
    text: &str,
) -> Result<Vec<u8>, TtsError> {
    if api_key.trim().is_empty() {
        return Err(TtsError::ConfigMissing("api_key empty".into()));
    }
    if voice_id.trim().is_empty() {
        return Err(TtsError::InvalidRequest("voice_id empty".into()));
    }

    let url = format!("{BASE}/v1/text-to-speech/{voice_id}");
    let body = SynthesizeBody {
        text,
        model_id,
        voice_settings: VoiceSettings::default(),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| TtsError::Internal(format!("build http client: {e}")))?;

    let resp = client
        .post(&url)
        .header("xi-api-key", api_key)
        .header("accept", "audio/mpeg")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if status.is_success() {
        let bytes = resp.bytes().await?;
        return Ok(bytes.to_vec());
    }

    // Non-2xx — read body for diagnostics, then map by status.
    let status_code = status.as_u16();
    let body_text = resp.text().await.unwrap_or_default();
    Err(match status_code {
        401 => TtsError::AuthFailed(format!("ElevenLabs 401 — check api_key. Body: {body_text}")),
        429 => TtsError::RateLimit(format!(
            "ElevenLabs 429 — quota / concurrent limit exceeded. Body: {body_text}"
        )),
        _ => TtsError::BackendError {
            status: status_code,
            body: body_text,
        },
    })
}

#[derive(Serialize)]
struct SynthesizeBody<'a> {
    text: &'a str,
    model_id: &'a str,
    voice_settings: VoiceSettings,
}

/// Default ElevenLabs voice settings — moderate stability, mid clarity.
/// Documented at <https://elevenlabs.io/docs/api-reference/text-to-speech>.
/// We don't expose these in the tool API yet; users wanting custom prosody
/// can switch voice_id rather than tune low-level knobs.
#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stability: 0.5,
            similarity_boost: 0.75,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke for the request-body shape — without actually hitting the
    /// network. We just need to know our serialized body matches what
    /// ElevenLabs expects (text + model_id + voice_settings).
    #[test]
    fn body_serializes_with_expected_fields() {
        let body = SynthesizeBody {
            text: "hello",
            model_id: "eleven_multilingual_v2",
            voice_settings: VoiceSettings::default(),
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["text"].as_str(), Some("hello"));
        assert_eq!(json["model_id"].as_str(), Some("eleven_multilingual_v2"));
        assert!(json["voice_settings"]["stability"].is_number());
        assert!(json["voice_settings"]["similarity_boost"].is_number());
    }

    #[tokio::test]
    async fn empty_api_key_returns_config_missing() {
        let err = synthesize("", "v", "m", "hi").await.unwrap_err();
        assert!(matches!(err, TtsError::ConfigMissing(_)));
    }

    #[tokio::test]
    async fn empty_voice_id_returns_invalid_request() {
        let err = synthesize("k", "", "m", "hi").await.unwrap_err();
        assert!(matches!(err, TtsError::InvalidRequest(_)));
    }
}
