//! Mock LLM provider for testing.

use std::sync::{Arc, Mutex};

/// Record of a completion request made to the mock provider.
#[derive(Debug, Clone)]
pub struct CompletionRecord {
    /// The messages sent in the request.
    pub messages: Vec<MockMessage>,
    /// Whether streaming was requested.
    pub stream: bool,
    /// Any tools that were provided.
    pub tools: Vec<String>,
}

/// A simple message representation for recording.
#[derive(Debug, Clone)]
pub struct MockMessage {
    /// Role of the message sender.
    pub role: String,
    /// Content of the message.
    pub content: String,
}

impl MockMessage {
    /// Create a new mock message.
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new("assistant", content)
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }
}

/// A mock tool call response.
#[derive(Debug, Clone)]
pub struct MockToolCall {
    /// The tool name.
    pub name: String,
    /// The tool arguments as JSON.
    pub arguments: serde_json::Value,
}

impl MockToolCall {
    /// Create a new mock tool call.
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// A mock LLM provider for testing.
///
/// Allows configuring fixed responses, streaming chunks, and error conditions
/// for testing without making real API calls.
#[derive(Debug, Clone)]
pub struct MockLlmProvider {
    responses: Arc<Mutex<Vec<MockResponse>>>,
    call_history: Arc<Mutex<Vec<CompletionRecord>>>,
    response_index: Arc<Mutex<usize>>,
    default_response: MockResponse,
    configured: bool,
}

/// Response type for the mock LLM provider.
#[derive(Debug, Clone)]
pub enum MockResponse {
    /// A fixed text response.
    Fixed(String),
    /// A streaming response with multiple chunks.
    Stream(Vec<String>),
    /// An error response.
    Error(String),
    /// A tool call response.
    ToolCall(Vec<MockToolCall>),
    /// A text response with tool calls.
    TextWithToolCalls(String, Vec<MockToolCall>),
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            response_index: Arc::new(Mutex::new(0)),
            default_response: MockResponse::Fixed("Mock response".into()),
            configured: false,
        }
    }
}

impl MockLlmProvider {
    /// Create a new empty mock provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mock provider that returns a fixed response.
    pub fn with_fixed_response(text: impl Into<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            response_index: Arc::new(Mutex::new(0)),
            default_response: MockResponse::Fixed(text.into()),
            configured: true,
        }
    }

    /// Create a mock provider that streams multiple chunks.
    pub fn with_stream_chunks(chunks: Vec<impl Into<String>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            response_index: Arc::new(Mutex::new(0)),
            default_response: MockResponse::Stream(chunks.into_iter().map(Into::into).collect()),
            configured: true,
        }
    }

    /// Create a mock provider that returns an error.
    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            response_index: Arc::new(Mutex::new(0)),
            default_response: MockResponse::Error(message.into()),
            configured: true,
        }
    }

    /// Create a mock provider that returns tool calls.
    pub fn with_tool_calls(calls: Vec<MockToolCall>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            response_index: Arc::new(Mutex::new(0)),
            default_response: MockResponse::ToolCall(calls),
            configured: true,
        }
    }

    /// Add a response to the sequence (for multiple calls).
    pub fn then_respond(self, text: impl Into<String>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::Fixed(text.into()));
        self
    }

    /// Add an error response to the sequence.
    pub fn then_error(self, message: impl Into<String>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::Error(message.into()));
        self
    }

    /// Add a tool call response to the sequence.
    pub fn then_tool_call(self, calls: Vec<MockToolCall>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::ToolCall(calls));
        self
    }

    /// Add a response with both text and tool calls.
    pub fn then_text_with_tools(self, text: impl Into<String>, calls: Vec<MockToolCall>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::TextWithToolCalls(text.into(), calls));
        self
    }

    /// Record a completion request (simulates an API call).
    pub fn record_completion(&self, messages: Vec<MockMessage>, stream: bool, tools: Vec<String>) {
        self.call_history.lock().unwrap().push(CompletionRecord {
            messages,
            stream,
            tools,
        });
    }

    /// Get the next response (advances the response index).
    ///
    /// For the first call, returns the default response.
    /// For subsequent calls, returns responses from the sequence (added via `then_*` methods).
    /// When the sequence is exhausted, returns the default response again.
    pub fn next_response(&self) -> MockResponse {
        let mut index = self.response_index.lock().unwrap();
        let responses = self.responses.lock().unwrap();

        // First call (index 0) returns default
        if *index == 0 {
            *index += 1;
            return self.default_response.clone();
        }

        // Subsequent calls use the sequence (offset by 1 since index 0 was default)
        let seq_index = *index - 1;
        if seq_index < responses.len() {
            let response = responses[seq_index].clone();
            *index += 1;
            response
        } else {
            // Sequence exhausted, return default
            self.default_response.clone()
        }
    }

    /// Get the call history.
    pub fn call_history(&self) -> Vec<CompletionRecord> {
        self.call_history.lock().unwrap().clone()
    }

    /// Get the number of calls made.
    pub fn call_count(&self) -> usize {
        self.call_history.lock().unwrap().len()
    }

    /// Clear the call history.
    pub fn clear_history(&self) {
        self.call_history.lock().unwrap().clear();
    }

    /// Reset the response index (for reusing the mock).
    pub fn reset(&self) {
        *self.response_index.lock().unwrap() = 0;
        self.call_history.lock().unwrap().clear();
    }

    /// Check if the mock is configured.
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// Get the configured response text (for fixed responses).
    pub fn response_text(&self) -> Option<String> {
        match &self.default_response {
            MockResponse::Fixed(text) => Some(text.clone()),
            MockResponse::TextWithToolCalls(text, _) => Some(text.clone()),
            _ => None,
        }
    }

    /// Get the configured stream chunks.
    pub fn stream_chunks(&self) -> Option<Vec<String>> {
        match &self.default_response {
            MockResponse::Stream(chunks) => Some(chunks.clone()),
            _ => None,
        }
    }

    /// Get the configured error message.
    pub fn error_message(&self) -> Option<String> {
        match &self.default_response {
            MockResponse::Error(msg) => Some(msg.clone()),
            _ => None,
        }
    }

    /// Check if this mock will return an error.
    pub fn will_error(&self) -> bool {
        matches!(self.default_response, MockResponse::Error(_))
    }

    /// Get configured tool calls.
    pub fn tool_calls(&self) -> Option<Vec<MockToolCall>> {
        match &self.default_response {
            MockResponse::ToolCall(calls) => Some(calls.clone()),
            MockResponse::TextWithToolCalls(_, calls) => Some(calls.clone()),
            _ => None,
        }
    }

    /// Wrap in an Arc for shared use.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_llm_provider_default() {
        let mock = MockLlmProvider::default();
        assert!(!mock.is_configured());
    }

    #[test]
    fn test_mock_llm_provider_fixed_response() {
        let mock = MockLlmProvider::with_fixed_response("Hello, world!");

        assert!(mock.is_configured());
        assert_eq!(mock.response_text(), Some("Hello, world!".to_string()));
        assert!(!mock.will_error());
    }

    #[test]
    fn test_mock_llm_provider_stream_chunks() {
        let chunks = vec!["Hello", " ", "world", "!"];
        let mock = MockLlmProvider::with_stream_chunks(chunks);

        assert!(mock.is_configured());
        let expected: Vec<String> = vec!["Hello", " ", "world", "!"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(mock.stream_chunks(), Some(expected));
    }

    #[test]
    fn test_mock_llm_provider_error() {
        let mock = MockLlmProvider::with_error("API rate limit exceeded");

        assert!(mock.is_configured());
        assert!(mock.will_error());
        assert_eq!(
            mock.error_message(),
            Some("API rate limit exceeded".to_string())
        );
    }

    #[test]
    fn test_mock_llm_provider_into_arc() {
        let mock = MockLlmProvider::with_fixed_response("Test");
        let arc_mock = mock.into_arc();

        assert!(arc_mock.is_configured());
    }

    #[test]
    fn test_mock_llm_provider_tool_calls() {
        let calls = vec![
            MockToolCall::new("read_file", serde_json::json!({"path": "/tmp/test.txt"})),
            MockToolCall::new(
                "write_file",
                serde_json::json!({"path": "/tmp/out.txt", "content": "hello"}),
            ),
        ];
        let mock = MockLlmProvider::with_tool_calls(calls);

        assert!(mock.is_configured());
        let tool_calls = mock.tool_calls().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].name, "read_file");
    }

    #[test]
    fn test_mock_llm_provider_response_sequence() {
        let mock = MockLlmProvider::with_fixed_response("First")
            .then_respond("Second")
            .then_respond("Third");

        // First call returns default
        let r1 = mock.next_response();
        assert!(matches!(r1, MockResponse::Fixed(s) if s == "First"));

        // Second call returns first in sequence
        let r2 = mock.next_response();
        assert!(matches!(r2, MockResponse::Fixed(s) if s == "Second"));

        // Third call returns second in sequence
        let r3 = mock.next_response();
        assert!(matches!(r3, MockResponse::Fixed(s) if s == "Third"));

        // Fourth call returns default again (sequence exhausted)
        let r4 = mock.next_response();
        assert!(matches!(r4, MockResponse::Fixed(s) if s == "First"));
    }

    #[test]
    fn test_mock_llm_provider_call_history() {
        let mock = MockLlmProvider::with_fixed_response("Response");

        assert_eq!(mock.call_count(), 0);

        mock.record_completion(vec![MockMessage::user("Hello")], false, vec![]);

        assert_eq!(mock.call_count(), 1);

        let history = mock.call_history();
        assert_eq!(history[0].messages[0].content, "Hello");
        assert_eq!(history[0].messages[0].role, "user");
    }

    #[test]
    fn test_mock_llm_provider_reset() {
        let mock = MockLlmProvider::with_fixed_response("First").then_respond("Second");

        // Use up the sequence
        mock.next_response();
        mock.record_completion(vec![], false, vec![]);

        assert_eq!(mock.call_count(), 1);

        // Reset
        mock.reset();

        assert_eq!(mock.call_count(), 0);
        // Response index also reset - first call returns default again
        let r = mock.next_response();
        assert!(matches!(r, MockResponse::Fixed(s) if s == "First"));
    }

    #[test]
    fn test_mock_message_constructors() {
        let user = MockMessage::user("Hello");
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "Hello");

        let assistant = MockMessage::assistant("Hi there");
        assert_eq!(assistant.role, "assistant");

        let system = MockMessage::system("You are helpful");
        assert_eq!(system.role, "system");
    }
}
