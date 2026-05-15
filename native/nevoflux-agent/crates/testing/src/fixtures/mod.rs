//! Test fixtures for NevoFlux protocol types.
//!
//! Provides builders and sample data generators for testing.
//!
//! # Example
//!
//! ```rust
//! use nevoflux_testing::fixtures::{EnvelopeBuilder, sample_session};
//!
//! // Build a custom envelope
//! let envelope = EnvelopeBuilder::new()
//!     .with_proxy_id("test-proxy")
//!     .with_request_id("req-001")
//!     .build();
//!
//! // Get a sample session
//! let session = sample_session();
//! ```

mod chat;
mod envelopes;
mod messages;
mod sessions;

// Envelope fixtures
pub use envelopes::EnvelopeBuilder;

// Session fixtures
pub use sessions::{sample_agent_session, sample_session, sample_sessions};

// Message fixtures (storage layer)
pub use messages::{
    sample_assistant_message, sample_conversation, sample_tool_result_message,
    sample_tool_use_message, sample_user_message,
};

// Chat fixtures (protocol layer)
pub use chat::{
    sample_chat_message, sample_code_block, sample_error, sample_permission_request,
    sample_stream_chunk, sample_stream_end,
};
