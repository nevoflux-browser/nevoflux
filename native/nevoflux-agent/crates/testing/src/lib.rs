//! NevoFlux Testing - Shared testing infrastructure
//!
//! This crate provides mocks, fixtures, and helpers for testing NevoFlux Agent components.
//!
//! # Modules
//!
//! - [`mocks`] - Mock implementations for external dependencies (LLM, Browser, Permissions)
//! - [`fixtures`] - Sample data generators for protocol types
//! - [`helpers`] - Test utilities, assertions, and storage helpers
//!
//! # Example
//!
//! ```rust
//! use nevoflux_testing::{
//!     fixtures::{sample_session, sample_chat_message, EnvelopeBuilder},
//!     mocks::{MockLlmProvider, MockPermissionChecker},
//!     helpers::{TestStorage, assert_ok},
//! };
//!
//! // Create test fixtures
//! let session = sample_session();
//! let message = sample_chat_message(&session.id, "Hello!");
//!
//! // Create mocks
//! let llm = MockLlmProvider::with_fixed_response("Hi there!");
//! let perms = MockPermissionChecker::allow_all();
//!
//! // Use test storage
//! let storage = TestStorage::new();
//! assert!(storage.is_empty());
//! ```

pub mod fixtures;
pub mod helpers;
pub mod mocks;

// Re-export commonly used types for convenience
pub use fixtures::{sample_session, EnvelopeBuilder};
pub use helpers::TestStorage;
pub use mocks::MockLlmProvider;

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_protocol::Channel;

    #[test]
    fn test_fixtures_module_exports_envelope_builder() {
        let envelope = fixtures::EnvelopeBuilder::new()
            .with_proxy_id("proxy-001")
            .with_channel(Channel::Chat)
            .build();

        assert_eq!(envelope.proxy_id, "proxy-001");
    }

    #[test]
    fn test_fixtures_module_exports_session_fixtures() {
        let session = fixtures::sample_session();
        assert!(!session.id.is_empty());
    }

    #[test]
    fn test_helpers_module_exports_test_storage() {
        let storage = helpers::TestStorage::new();
        assert!(storage.is_empty());
    }

    #[test]
    fn test_mocks_module_exports_mock_llm_provider() {
        let mock = mocks::MockLlmProvider::with_fixed_response("Hello!");
        assert!(mock.is_configured());
    }

    #[test]
    fn test_mocks_module_exports_permission_checker() {
        let checker = mocks::MockPermissionChecker::allow_all();
        assert!(checker.check("file", "read", "/any/path"));
    }

    #[test]
    fn test_mocks_module_exports_browser_actions() {
        let browser = mocks::MockBrowserActions::new();
        let response = browser.execute("click", serde_json::json!({}));
        assert!(response.success);
    }

    #[test]
    fn test_fixtures_chat_messages() {
        let msg = fixtures::sample_chat_message("sess-001", "Test");
        assert_eq!(msg.session_id, "sess-001");
        assert_eq!(msg.text, "Test");
    }

    #[test]
    fn test_fixtures_conversations() {
        let conv = fixtures::sample_conversation("sess-001", 3);
        assert_eq!(conv.len(), 6);
    }

    #[test]
    fn test_helpers_assertions() {
        let result: Result<i32, &str> = Ok(42);
        let value = helpers::assert_ok(result);
        assert_eq!(value, 42);
    }
}
