//! Kimi-agent CLI provider implementation (wire mode).
//!
//! Provides access to LLMs via the kimi-agent CLI subprocess,
//! communicating over JSON-RPC 2.0 stdin/stdout (wire protocol).

mod client;
mod completion;
pub mod types;
pub mod wire;

pub use client::KimiAgentClient;
pub use completion::{KimiAgentCompletionModel, KimiAgentStreamingResponse};
