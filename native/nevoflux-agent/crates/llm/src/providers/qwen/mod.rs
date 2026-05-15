//! Qwen/DashScope provider implementation.
//!
//! Provides access to Alibaba's Qwen models via the DashScope API.
//!
//! # Example
//! ```ignore
//! use nevoflux_llm::providers::qwen::QwenClient;
//!
//! let client = QwenClient::new("your-api-key");
//! let model = client.completion_model("qwen-turbo");
//! ```

mod client;
mod completion;
mod types;

pub use client::{QwenClient, QWEN_BASE_URL};
pub use completion::{QwenCompletionModel, QwenMessage};
pub use types::*;
