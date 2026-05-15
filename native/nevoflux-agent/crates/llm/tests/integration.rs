//! Integration tests for nevoflux-llm crate.
//!
//! Tests marked with #[ignore] require API keys set in environment variables:
//! - ANTHROPIC_API_KEY for Anthropic tests
//! - OPENAI_API_KEY for OpenAI tests
//! - DASHSCOPE_API_KEY for Qwen tests

use nevoflux_llm::providers::qwen::{QwenClient, QwenCompletionModel, QWEN_BASE_URL};
use nevoflux_llm::{default_model_for, ProviderConfig, ProviderType};

// =============================================================================
// Configuration Tests
// =============================================================================

#[test]
fn test_provider_config_creation() {
    let config = ProviderConfig::new(ProviderType::Anthropic, "test-key");
    assert_eq!(config.provider, ProviderType::Anthropic);
    assert_eq!(config.api_key, "test-key");
}

#[test]
fn test_all_provider_types() {
    let providers = [
        ProviderType::Anthropic,
        ProviderType::OpenAi,
        ProviderType::OpenRouter,
        ProviderType::DeepSeek,
        ProviderType::Qwen,
    ];

    for provider in providers {
        let model = default_model_for(provider);
        assert!(
            !model.is_empty(),
            "Default model for {:?} should not be empty",
            provider
        );
    }
}

#[test]
fn test_config_builder_pattern() {
    let config = ProviderConfig::new(ProviderType::Qwen, "test-key")
        .with_base_url("https://custom.example.com")
        .with_default_model("qwen-plus");

    assert_eq!(
        config.base_url,
        Some("https://custom.example.com".to_string())
    );
    assert_eq!(config.default_model, Some("qwen-plus".to_string()));
}

// =============================================================================
// Qwen Client Tests
// =============================================================================

#[test]
fn test_qwen_client_creation() {
    let client = QwenClient::new("test-key");
    assert_eq!(client.base_url(), QWEN_BASE_URL);
}

#[test]
fn test_qwen_client_custom_base_url() {
    let client = QwenClient::new("test-key").with_base_url("https://custom.dashscope.com/v1");
    assert_eq!(client.base_url(), "https://custom.dashscope.com/v1");
}

#[test]
fn test_qwen_completion_model_creation() {
    let client = QwenClient::new("test-key");
    let model = client.completion_model("qwen-turbo");
    assert_eq!(model.model(), "qwen-turbo");
}

#[test]
fn test_qwen_completion_model_different_models() {
    let client = QwenClient::new("test-key");

    let turbo = client.completion_model("qwen-turbo");
    let plus = client.completion_model("qwen-plus");
    let max = client.completion_model("qwen-max");

    assert_eq!(turbo.model(), "qwen-turbo");
    assert_eq!(plus.model(), "qwen-plus");
    assert_eq!(max.model(), "qwen-max");
}

#[test]
fn test_qwen_client_is_clonable() {
    let client = QwenClient::new("test-key");
    let cloned = client.clone();
    assert_eq!(cloned.base_url(), client.base_url());
}

#[test]
fn test_qwen_completion_model_is_clonable() {
    let client = QwenClient::new("test-key");
    let model = client.completion_model("qwen-turbo");
    let cloned = model.clone();
    assert_eq!(cloned.model(), model.model());
}

// =============================================================================
// Real API Tests (marked #[ignore])
// =============================================================================

#[tokio::test]
#[ignore = "Requires DASHSCOPE_API_KEY environment variable"]
async fn test_qwen_real_api() {
    let api_key = std::env::var("DASHSCOPE_API_KEY").expect("DASHSCOPE_API_KEY must be set");

    let client = QwenClient::new(api_key);
    let model = client.completion_model("qwen-turbo");

    // Note: This would make a real API call
    // The test verifies the client can be constructed with real credentials
    assert_eq!(model.model(), "qwen-turbo");
}

#[test]
#[ignore = "Requires ANTHROPIC_API_KEY environment variable"]
fn test_anthropic_config() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");

    let config = ProviderConfig::new(ProviderType::Anthropic, api_key);
    // With rig-core, you would create the client like:
    // let client = rig::providers::anthropic::Client::new(&config.api_key);
    assert_eq!(config.provider, ProviderType::Anthropic);
}

#[test]
#[ignore = "Requires OPENAI_API_KEY environment variable"]
fn test_openai_config() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");

    let config = ProviderConfig::new(ProviderType::OpenAi, api_key);
    assert_eq!(config.provider, ProviderType::OpenAi);
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_llm_error_types() {
    use nevoflux_llm::LlmError;

    let err = LlmError::Api {
        status: 401,
        message: "Unauthorized".to_string(),
    };
    assert!(err.to_string().contains("401"));
    assert!(err.to_string().contains("Unauthorized"));
}

#[test]
fn test_llm_error_rate_limited() {
    use nevoflux_llm::LlmError;

    let err = LlmError::RateLimited {
        retry_after_ms: 5000,
    };
    assert!(err.to_string().contains("5000"));
}

#[test]
fn test_llm_error_authentication() {
    use nevoflux_llm::LlmError;

    let err = LlmError::Authentication("Invalid API key".to_string());
    assert!(err.to_string().contains("Invalid API key"));
}

#[test]
fn test_llm_error_unsupported_provider() {
    use nevoflux_llm::LlmError;

    let err = LlmError::UnsupportedProvider("FakeProvider".to_string());
    assert!(err.to_string().contains("FakeProvider"));
}

#[test]
fn test_llm_error_stream() {
    use nevoflux_llm::LlmError;

    let err = LlmError::Stream("Connection reset".to_string());
    assert!(err.to_string().contains("Connection reset"));
}

// =============================================================================
// Provider Type Tests
// =============================================================================

#[test]
fn test_provider_type_equality() {
    assert_eq!(ProviderType::Anthropic, ProviderType::Anthropic);
    assert_ne!(ProviderType::Anthropic, ProviderType::OpenAi);
    assert_ne!(ProviderType::Qwen, ProviderType::DeepSeek);
}

#[test]
fn test_provider_type_copy() {
    let provider = ProviderType::DeepSeek;
    let copied = provider;
    assert_eq!(provider, copied);
}

#[test]
fn test_provider_type_debug() {
    assert_eq!(format!("{:?}", ProviderType::Anthropic), "Anthropic");
    assert_eq!(format!("{:?}", ProviderType::OpenAi), "OpenAi");
    assert_eq!(format!("{:?}", ProviderType::OpenRouter), "OpenRouter");
    assert_eq!(format!("{:?}", ProviderType::DeepSeek), "DeepSeek");
    assert_eq!(format!("{:?}", ProviderType::Qwen), "Qwen");
}

// =============================================================================
// Default Model Tests
// =============================================================================

#[test]
fn test_default_model_anthropic() {
    let model = default_model_for(ProviderType::Anthropic);
    assert!(model.contains("claude"));
}

#[test]
fn test_default_model_openai() {
    let model = default_model_for(ProviderType::OpenAi);
    assert!(model.contains("gpt"));
}

#[test]
fn test_default_model_qwen() {
    let model = default_model_for(ProviderType::Qwen);
    assert!(model.contains("qwen"));
}

#[test]
fn test_default_model_deepseek() {
    let model = default_model_for(ProviderType::DeepSeek);
    assert!(model.contains("deepseek"));
}

#[test]
fn test_default_model_openrouter() {
    let model = default_model_for(ProviderType::OpenRouter);
    // OpenRouter uses provider/model format
    assert!(model.contains("/"));
}

// =============================================================================
// Rig Re-export Tests
// =============================================================================

#[test]
fn test_rig_is_reexported() {
    // Verify that rig is available via nevoflux_llm::rig
    use nevoflux_llm::rig;

    // Access something from rig to verify it's properly re-exported
    fn assert_rig_available<T: rig::completion::CompletionModel>() {}
    assert_rig_available::<QwenCompletionModel>();
}

// =============================================================================
// Config with All Options Tests
// =============================================================================

#[test]
fn test_provider_config_with_all_options() {
    let config = ProviderConfig::new(ProviderType::OpenRouter, "router-key")
        .with_base_url("https://api.openrouter.ai/api/v1")
        .with_default_model("anthropic/claude-3-opus");

    assert_eq!(config.provider, ProviderType::OpenRouter);
    assert_eq!(config.api_key, "router-key");
    assert_eq!(
        config.base_url,
        Some("https://api.openrouter.ai/api/v1".to_string())
    );
    assert_eq!(
        config.default_model,
        Some("anthropic/claude-3-opus".to_string())
    );
}

#[test]
fn test_provider_config_clone() {
    let config = ProviderConfig::new(ProviderType::OpenAi, "key")
        .with_base_url("https://custom.api.com")
        .with_default_model("gpt-4");

    let cloned = config.clone();
    assert_eq!(cloned.provider, config.provider);
    assert_eq!(cloned.api_key, config.api_key);
    assert_eq!(cloned.base_url, config.base_url);
    assert_eq!(cloned.default_model, config.default_model);
}
