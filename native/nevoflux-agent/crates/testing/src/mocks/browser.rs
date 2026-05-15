//! Mock browser actions for testing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A mock browser action handler for testing.
///
/// Simulates browser interactions without requiring a real browser.
#[derive(Debug, Clone)]
pub struct MockBrowserActions {
    responses: Arc<Mutex<HashMap<String, MockBrowserResponse>>>,
    action_log: Arc<Mutex<Vec<BrowserActionRecord>>>,
    default_success: bool,
}

/// A recorded browser action for verification.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserActionRecord {
    pub action: String,
    pub params: serde_json::Value,
}

/// Response from a mock browser action.
#[derive(Debug, Clone)]
pub struct MockBrowserResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl Default for MockBrowserActions {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBrowserActions {
    /// Create a new mock browser with default success responses.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            action_log: Arc::new(Mutex::new(Vec::new())),
            default_success: true,
        }
    }

    /// Create a mock browser that fails by default.
    pub fn failing() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            action_log: Arc::new(Mutex::new(Vec::new())),
            default_success: false,
        }
    }

    /// Configure a response for a specific action.
    pub fn with_response(self, action: &str, response: MockBrowserResponse) -> Self {
        self.responses
            .lock()
            .unwrap()
            .insert(action.to_string(), response);
        self
    }

    /// Configure a successful response with result data.
    pub fn with_success(self, action: &str, result: serde_json::Value) -> Self {
        self.with_response(
            action,
            MockBrowserResponse {
                success: true,
                result: Some(result),
                error: None,
            },
        )
    }

    /// Configure a failure response.
    pub fn with_failure(self, action: &str, error: &str) -> Self {
        self.with_response(
            action,
            MockBrowserResponse {
                success: false,
                result: None,
                error: Some(error.to_string()),
            },
        )
    }

    /// Execute a browser action (mock).
    pub fn execute(&self, action: &str, params: serde_json::Value) -> MockBrowserResponse {
        // Record the action
        self.action_log.lock().unwrap().push(BrowserActionRecord {
            action: action.to_string(),
            params: params.clone(),
        });

        // Return configured or default response
        self.responses
            .lock()
            .unwrap()
            .get(action)
            .cloned()
            .unwrap_or_else(|| {
                if self.default_success {
                    MockBrowserResponse {
                        success: true,
                        result: Some(serde_json::json!({})),
                        error: None,
                    }
                } else {
                    MockBrowserResponse {
                        success: false,
                        result: None,
                        error: Some("Mock browser action failed".to_string()),
                    }
                }
            })
    }

    /// Get all recorded actions.
    pub fn recorded_actions(&self) -> Vec<BrowserActionRecord> {
        self.action_log.lock().unwrap().clone()
    }

    /// Check if a specific action was called.
    pub fn was_called(&self, action: &str) -> bool {
        self.action_log
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.action == action)
    }

    /// Get the number of times an action was called.
    pub fn call_count(&self, action: &str) -> usize {
        self.action_log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.action == action)
            .count()
    }

    /// Clear the action log.
    pub fn clear_log(&self) {
        self.action_log.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_browser_default_success() {
        let browser = MockBrowserActions::new();

        let response = browser.execute("click", serde_json::json!({"selector": "#btn"}));
        assert!(response.success);
    }

    #[test]
    fn test_mock_browser_failing() {
        let browser = MockBrowserActions::failing();

        let response = browser.execute("click", serde_json::json!({}));
        assert!(!response.success);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_mock_browser_configured_response() {
        let browser = MockBrowserActions::new()
            .with_success(
                "navigate",
                serde_json::json!({"url": "https://example.com"}),
            )
            .with_failure("click", "Element not found");

        let nav_response = browser.execute("navigate", serde_json::json!({}));
        assert!(nav_response.success);

        let click_response = browser.execute("click", serde_json::json!({}));
        assert!(!click_response.success);
        assert_eq!(click_response.error, Some("Element not found".to_string()));
    }

    #[test]
    fn test_mock_browser_action_recording() {
        let browser = MockBrowserActions::new();

        browser.execute(
            "navigate",
            serde_json::json!({"url": "https://example.com"}),
        );
        browser.execute("click", serde_json::json!({"selector": "#btn"}));
        browser.execute("click", serde_json::json!({"selector": "#other"}));

        assert!(browser.was_called("navigate"));
        assert!(browser.was_called("click"));
        assert!(!browser.was_called("type"));

        assert_eq!(browser.call_count("navigate"), 1);
        assert_eq!(browser.call_count("click"), 2);

        let actions = browser.recorded_actions();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].action, "navigate");
    }

    #[test]
    fn test_mock_browser_clear_log() {
        let browser = MockBrowserActions::new();

        browser.execute("click", serde_json::json!({}));
        assert_eq!(browser.call_count("click"), 1);

        browser.clear_log();
        assert_eq!(browser.call_count("click"), 0);
    }
}
