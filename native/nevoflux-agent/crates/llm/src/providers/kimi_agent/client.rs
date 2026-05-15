//! KimiAgentClient implementation for the kimi-agent CLI.

use super::completion::KimiAgentCompletionModel;

/// Client for interacting with Kimi models via the kimi-agent CLI.
///
/// The CLI communicates over stdin/stdout using a JSON-RPC 2.0 wire
/// protocol. An optional API key can be passed via environment variable.
///
/// # Example
/// ```ignore
/// use nevoflux_llm::providers::kimi_agent::KimiAgentClient;
///
/// let client = KimiAgentClient::new("kimi-agent");
/// let model = client.completion_model("kimi-latest");
/// ```
#[derive(Clone)]
pub struct KimiAgentClient {
    /// Path to the kimi-agent binary.
    command: String,
    /// Optional API key, passed via environment variable.
    api_key: Option<String>,
    /// Default model name.
    model: Option<String>,
    /// Working directory for the CLI subprocess (--work-dir).
    working_dir: Option<String>,
    /// Enable or disable thinking mode (--thinking / --no-thinking).
    thinking: Option<bool>,
}

impl KimiAgentClient {
    /// Create a new KimiAgentClient with the given command path.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            api_key: None,
            model: None,
            working_dir: None,
            thinking: None,
        }
    }

    /// Set an API key to pass to the CLI via environment variable.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the default model name.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the working directory for the CLI subprocess.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Set the thinking mode flag.
    ///
    /// - `true` passes `--thinking` to the CLI.
    /// - `false` passes `--no-thinking`.
    /// - `None` (default) omits the flag entirely.
    pub fn with_thinking(mut self, thinking: bool) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// Get the command path.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Get the API key (for internal use).
    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Get the default model name.
    pub(crate) fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Get the working directory.
    pub(crate) fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    /// Get the thinking mode flag.
    pub(crate) fn thinking(&self) -> Option<bool> {
        self.thinking
    }

    /// Create a completion model for the specified model name.
    ///
    /// # Example
    /// ```ignore
    /// use nevoflux_llm::providers::kimi_agent::KimiAgentClient;
    ///
    /// let client = KimiAgentClient::new("kimi-agent");
    /// let model = client.completion_model("kimi-latest");
    /// ```
    pub fn completion_model(&self, model: impl Into<String>) -> KimiAgentCompletionModel {
        KimiAgentCompletionModel::new(self.clone(), model)
    }
}

impl std::fmt::Debug for KimiAgentClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KimiAgentClient")
            .field("command", &self.command)
            .field("api_key", &self.api_key.as_ref().map(|_| "<REDACTED>"))
            .field("model", &self.model)
            .field("working_dir", &self.working_dir)
            .field("thinking", &self.thinking)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let client = KimiAgentClient::new("kimi-agent");
        assert_eq!(client.command(), "kimi-agent");
    }

    #[test]
    fn test_client_with_api_key() {
        let client = KimiAgentClient::new("kimi-agent").with_api_key("sk-test-key");
        assert_eq!(client.api_key(), Some("sk-test-key"));
    }

    #[test]
    fn test_client_with_model() {
        let client = KimiAgentClient::new("kimi-agent").with_model("kimi-latest");
        assert_eq!(client.model(), Some("kimi-latest"));
    }

    #[test]
    fn test_client_with_working_dir() {
        let client = KimiAgentClient::new("kimi-agent").with_working_dir("/tmp/workspace");
        assert_eq!(client.working_dir(), Some("/tmp/workspace"));
    }

    #[test]
    fn test_client_with_thinking() {
        let client = KimiAgentClient::new("kimi-agent").with_thinking(true);
        assert_eq!(client.thinking(), Some(true));

        let client2 = KimiAgentClient::new("kimi-agent").with_thinking(false);
        assert_eq!(client2.thinking(), Some(false));
    }

    #[test]
    fn test_client_debug_redacts_api_key() {
        let client = KimiAgentClient::new("kimi-agent").with_api_key("super-secret-key");
        let debug_output = format!("{:?}", client);
        assert!(!debug_output.contains("super-secret-key"));
        assert!(debug_output.contains("<REDACTED>"));
    }

    #[test]
    fn test_client_defaults() {
        let client = KimiAgentClient::new("kimi-agent");
        assert_eq!(client.command(), "kimi-agent");
        assert!(client.api_key().is_none());
        assert!(client.model().is_none());
        assert!(client.working_dir().is_none());
        assert!(client.thinking().is_none());
    }

    #[test]
    fn test_client_clone() {
        let client = KimiAgentClient::new("kimi-agent")
            .with_api_key("key")
            .with_model("kimi-latest")
            .with_working_dir("/tmp")
            .with_thinking(true);
        let cloned = client.clone();
        assert_eq!(cloned.command(), "kimi-agent");
        assert_eq!(cloned.api_key(), Some("key"));
        assert_eq!(cloned.model(), Some("kimi-latest"));
        assert_eq!(cloned.working_dir(), Some("/tmp"));
        assert_eq!(cloned.thinking(), Some(true));
    }

    #[test]
    fn test_completion_model_creation() {
        let client = KimiAgentClient::new("kimi-agent");
        let model = client.completion_model("kimi-latest");
        assert_eq!(model.model(), "kimi-latest");
    }
}
