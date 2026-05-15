//! QwenClient implementation for DashScope API.

use reqwest::Client as HttpClient;

use super::completion::QwenCompletionModel;

/// DashScope API base URL (OpenAI-compatible endpoint)
pub const QWEN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

/// Client for interacting with Alibaba's Qwen models via DashScope API.
///
/// # Example
/// ```ignore
/// use nevoflux_llm::providers::qwen::QwenClient;
///
/// let client = QwenClient::new("your-api-key");
/// let model = client.completion_model("qwen-turbo");
/// ```
#[derive(Clone)]
pub struct QwenClient {
    api_key: String,
    http_client: HttpClient,
    base_url: String,
}

impl QwenClient {
    /// Create a new QwenClient with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            http_client: HttpClient::new(),
            base_url: QWEN_BASE_URL.to_string(),
        }
    }

    /// Set a custom base URL (useful for testing or proxies).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Get the current base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the API key (for internal use in requests).
    pub(crate) fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Get the HTTP client (for internal use).
    pub(crate) fn http_client(&self) -> &HttpClient {
        &self.http_client
    }

    /// Create a completion model for the specified model name.
    ///
    /// # Example
    /// ```ignore
    /// use nevoflux_llm::providers::qwen::QwenClient;
    ///
    /// let client = QwenClient::new("your-api-key");
    /// let model = client.completion_model("qwen-turbo");
    /// ```
    pub fn completion_model(&self, model: impl Into<String>) -> QwenCompletionModel {
        QwenCompletionModel::new(self.clone(), model)
    }
}

impl std::fmt::Debug for QwenClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QwenClient")
            .field("api_key", &"<REDACTED>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let client = QwenClient::new("test-api-key");
        assert_eq!(client.base_url(), QWEN_BASE_URL);
    }

    #[test]
    fn test_client_with_base_url() {
        let client = QwenClient::new("test-api-key").with_base_url("https://custom.example.com/v1");
        assert_eq!(client.base_url(), "https://custom.example.com/v1");
    }

    #[test]
    fn test_client_debug_redacts_api_key() {
        let client = QwenClient::new("super-secret-key");
        let debug_output = format!("{:?}", client);
        assert!(!debug_output.contains("super-secret-key"));
        assert!(debug_output.contains("<REDACTED>"));
    }

    #[test]
    fn test_client_is_clone() {
        let client = QwenClient::new("test-key");
        let _cloned = client.clone();
    }

    #[test]
    fn test_api_key_accessible_internally() {
        let client = QwenClient::new("my-secret-key");
        assert_eq!(client.api_key(), "my-secret-key");
    }

    #[test]
    fn test_completion_model_creation() {
        let client = QwenClient::new("test-key");
        let model = client.completion_model("qwen-turbo");
        assert_eq!(model.model(), "qwen-turbo");
    }

    #[test]
    fn test_completion_model_with_custom_base_url() {
        let client = QwenClient::new("test-key").with_base_url("https://custom.example.com/v1");
        let model = client.completion_model("qwen-plus");
        assert_eq!(model.model(), "qwen-plus");
    }
}
