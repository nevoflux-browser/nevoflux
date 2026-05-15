//! Mock implementations for external dependencies.
//!
//! Provides mock LLM providers, MCP clients, browser actions, and permission checkers.
//!
//! # Example
//!
//! ```rust
//! use nevoflux_testing::mocks::{MockLlmProvider, MockPermissionChecker, MockBrowserActions};
//!
//! // Create a mock LLM that returns a fixed response
//! let llm = MockLlmProvider::with_fixed_response("Hello, world!");
//!
//! // Create a permission checker that allows specific actions
//! let perms = MockPermissionChecker::deny_all()
//!     .with_decision("file", "read", "/safe/path", true);
//!
//! // Create a mock browser
//! let browser = MockBrowserActions::new()
//!     .with_success("navigate", serde_json::json!({"url": "https://example.com"}));
//! ```

mod browser;
mod llm;
mod mcp_client;
mod permission;

pub use browser::{BrowserActionRecord, MockBrowserActions, MockBrowserResponse};
pub use llm::{CompletionRecord, MockLlmProvider, MockMessage, MockResponse, MockToolCall};
pub use mcp_client::{MockMcpClient, MockMcpError, MockResource, ToolCallRecord};
pub use permission::MockPermissionChecker;
