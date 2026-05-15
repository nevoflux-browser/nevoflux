//! Error types for the LLM crate.

use thiserror::Error;

/// Errors that can occur when interacting with LLM providers.
#[derive(Error, Debug)]
pub enum LlmError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Rig framework error
    #[error("Rig error: {0}")]
    Rig(String),

    /// API returned an error response
    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    /// Rate limited by the provider
    #[error("Rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Provider not supported
    #[error("Provider not supported: {0}")]
    UnsupportedProvider(String),

    /// Stream processing error
    #[error("Stream error: {0}")]
    Stream(String),
}

/// Result type alias using [`LlmError`]
pub type Result<T> = std::result::Result<T, LlmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_display() {
        let err = LlmError::Api {
            status: 429,
            message: "Rate limited".to_string(),
        };
        assert_eq!(err.to_string(), "API error: 429 - Rate limited");
    }

    #[test]
    fn test_rate_limited_display() {
        let err = LlmError::RateLimited {
            retry_after_ms: 1000,
        };
        assert_eq!(err.to_string(), "Rate limited: retry after 1000ms");
    }

    #[test]
    fn test_authentication_error_display() {
        let err = LlmError::Authentication("Invalid API key".to_string());
        assert_eq!(err.to_string(), "Authentication failed: Invalid API key");
    }

    #[test]
    fn test_http_error_from() {
        // Test that reqwest::Error can be converted
        // (We can't easily create a reqwest::Error, so just verify the From impl exists)
        fn assert_from<T: From<reqwest::Error>>() {}
        assert_from::<LlmError>();
    }

    #[test]
    fn test_json_error_from() {
        // Test that serde_json::Error can be converted
        fn assert_from<T: From<serde_json::Error>>() {}
        assert_from::<LlmError>();
    }
}
