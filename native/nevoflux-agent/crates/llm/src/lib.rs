//! NevoFlux LLM - LLM Provider abstraction layer
//!
//! Built on rig-core with additional providers (Qwen/DashScope).
//!
//! # Built-in Providers (via rig)
//! - Anthropic: `rig::providers::anthropic`
//! - OpenAI: `rig::providers::openai`
//! - OpenRouter: `rig::providers::openrouter`
//! - DeepSeek: `rig::providers::deepseek`
//!
//! # Custom Providers
//! - Qwen (DashScope): `nevoflux_llm::providers::qwen`

pub mod embedding;
pub mod error;
pub mod factory;
pub mod providers;
pub mod util;

// Re-export rig for convenience
pub use rig;

#[cfg(feature = "embedding")]
pub use embedding::FastEmbedProvider;
pub use embedding::{EmbeddingConfig, EmbeddingError, EmbeddingModel, EmbeddingProvider};
pub use error::{LlmError, Result};
pub use factory::{
    api_key_env_var, default_context_window_for, default_model_for, gemini_models, ProviderConfig,
    ProviderType,
};
