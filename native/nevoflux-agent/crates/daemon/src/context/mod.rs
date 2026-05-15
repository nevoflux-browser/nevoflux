//! Context building for LLM requests.

mod builder;
mod circuit_breaker;
mod compressor;
mod microcompact;

pub use builder::{Context, ContextBuilder, ContextMessage, TokenBudget};
pub use circuit_breaker::{CircuitState, CompressionCircuitBreaker};
pub use compressor::{get_summarization_provider, CompressionResult, ContextCompressor};
pub use microcompact::{MicroCompactResult, MicroCompactor};
