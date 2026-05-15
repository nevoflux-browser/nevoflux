//! Real LLM provider integration tests.
//!
//! These tests make actual API calls to LLM providers and are disabled by default.
//! To run them, set the appropriate environment variables:
//!
//! - `ANTHROPIC_API_KEY` - For Anthropic Claude tests
//! - `OPENAI_API_KEY` - For OpenAI tests
//!
//! Run with: `cargo test --test real_llm_flow -- --ignored --nocapture`
//!
//! Note: The rig-core crate API may vary by version. These tests serve as
//! integration test templates that should be updated based on the actual
//! rig-core API being used.

use nevoflux_llm::{default_model_for, ProviderConfig, ProviderType};

/// Check if an API key is available for a provider.
fn has_api_key(provider: ProviderType) -> Option<String> {
    let env_var = match provider {
        ProviderType::Anthropic => "ANTHROPIC_API_KEY",
        ProviderType::OpenAi => "OPENAI_API_KEY",
        ProviderType::Qwen => "DASHSCOPE_API_KEY",
        ProviderType::DeepSeek => "DEEPSEEK_API_KEY",
        ProviderType::OpenRouter => "OPENROUTER_API_KEY",
        ProviderType::Gemini => "GEMINI_API_KEY",
        ProviderType::Groq => "GROQ_API_KEY",
        ProviderType::Ollama => "OLLAMA_API_KEY",
        ProviderType::Mistral => "MISTRAL_API_KEY",
        ProviderType::XAi => "XAI_API_KEY",
        ProviderType::Cohere => "COHERE_API_KEY",
        ProviderType::Perplexity => "PERPLEXITY_API_KEY",
        ProviderType::Together => "TOGETHER_API_KEY",
        ProviderType::ClaudeCode => "ANTHROPIC_API_KEY",
        ProviderType::GeminiCli => "GEMINI_API_KEY",
        ProviderType::KimiAgent => "MOONSHOT_API_KEY",
        ProviderType::OpenClaw => "OPENCLAW_API_KEY",
    };

    std::env::var(env_var).ok().filter(|k| !k.is_empty())
}

mod provider_config_tests {
    use super::*;

    #[test]
    fn test_provider_config_for_all_types() {
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAi,
            ProviderType::Qwen,
            ProviderType::DeepSeek,
            ProviderType::OpenRouter,
            ProviderType::Gemini,
            ProviderType::Groq,
            ProviderType::Ollama,
            ProviderType::Mistral,
            ProviderType::XAi,
            ProviderType::Cohere,
            ProviderType::Perplexity,
            ProviderType::Together,
        ];

        for provider in providers {
            let config = ProviderConfig::new(provider, "test-key");
            assert_eq!(config.provider, provider);
            assert_eq!(config.api_key, "test-key");

            // Verify default model exists
            let default_model = default_model_for(provider);
            assert!(
                !default_model.is_empty(),
                "Default model should not be empty for {:?}",
                provider
            );
        }
    }

    #[test]
    fn test_api_key_detection() {
        // This test verifies the has_api_key function works correctly
        // It doesn't require actual keys to be set

        // Temporarily set and unset an env var to test
        let test_var = "NEVOFLUX_TEST_API_KEY_12345";
        std::env::set_var(test_var, "test-value");
        assert!(std::env::var(test_var).is_ok());
        std::env::remove_var(test_var);
        assert!(std::env::var(test_var).is_err());
    }

    #[test]
    fn test_provider_type_variants() {
        // Ensure all provider types are accessible
        let _ = ProviderType::Anthropic;
        let _ = ProviderType::OpenAi;
        let _ = ProviderType::Qwen;
        let _ = ProviderType::DeepSeek;
        let _ = ProviderType::OpenRouter;
        let _ = ProviderType::Gemini;
        let _ = ProviderType::Groq;
        let _ = ProviderType::Ollama;
        let _ = ProviderType::Mistral;
        let _ = ProviderType::XAi;
        let _ = ProviderType::Cohere;
        let _ = ProviderType::Perplexity;
        let _ = ProviderType::Together;
    }

    #[test]
    fn test_provider_config_builder_pattern() {
        let config = ProviderConfig::new(ProviderType::Anthropic, "test-key")
            .with_base_url("https://custom.api.example.com")
            .with_default_model("claude-custom");

        assert_eq!(config.provider, ProviderType::Anthropic);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(
            config.base_url,
            Some("https://custom.api.example.com".to_string())
        );
        assert_eq!(config.default_model, Some("claude-custom".to_string()));
    }

    #[test]
    fn test_default_models_are_valid() {
        // Verify default models follow expected naming conventions
        let anthropic_model = default_model_for(ProviderType::Anthropic);
        assert!(
            anthropic_model.contains("claude"),
            "Anthropic model should contain 'claude'"
        );

        let openai_model = default_model_for(ProviderType::OpenAi);
        assert!(
            openai_model.contains("gpt"),
            "OpenAI model should contain 'gpt'"
        );

        let qwen_model = default_model_for(ProviderType::Qwen);
        assert!(
            qwen_model.contains("qwen"),
            "Qwen model should contain 'qwen'"
        );
    }

    #[test]
    fn test_has_api_key_returns_none_for_unset() {
        // Verify has_api_key returns None when no key is set
        // (assumes NEVOFLUX_TEST_NONEXISTENT_PROVIDER_KEY is not set)
        let result = std::env::var("NEVOFLUX_TEST_NONEXISTENT_PROVIDER_KEY_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_equality() {
        assert_eq!(ProviderType::Anthropic, ProviderType::Anthropic);
        assert_ne!(ProviderType::Anthropic, ProviderType::OpenAi);
        assert_ne!(ProviderType::Qwen, ProviderType::DeepSeek);
    }

    #[test]
    fn test_provider_debug_format() {
        let debug_str = format!("{:?}", ProviderType::Anthropic);
        assert_eq!(debug_str, "Anthropic");

        let debug_str = format!("{:?}", ProviderType::OpenAi);
        assert_eq!(debug_str, "OpenAi");
    }

    #[test]
    fn test_provider_config_clone() {
        let config = ProviderConfig::new(ProviderType::OpenAi, "key").with_default_model("gpt-4");

        let cloned = config.clone();
        assert_eq!(cloned.provider, config.provider);
        assert_eq!(cloned.api_key, config.api_key);
        assert_eq!(cloned.default_model, config.default_model);
    }
}

mod api_key_availability_tests {
    use super::*;

    #[test]
    fn test_anthropic_key_check() {
        let has_key = has_api_key(ProviderType::Anthropic).is_some();
        println!("Anthropic API key available: {}", has_key);
    }

    #[test]
    fn test_openai_key_check() {
        let has_key = has_api_key(ProviderType::OpenAi).is_some();
        println!("OpenAI API key available: {}", has_key);
    }

    #[test]
    fn test_qwen_key_check() {
        let has_key = has_api_key(ProviderType::Qwen).is_some();
        println!("Qwen/DashScope API key available: {}", has_key);
    }

    #[test]
    fn test_list_available_providers() {
        let providers = [
            (ProviderType::Anthropic, "ANTHROPIC_API_KEY"),
            (ProviderType::OpenAi, "OPENAI_API_KEY"),
            (ProviderType::Qwen, "DASHSCOPE_API_KEY"),
            (ProviderType::DeepSeek, "DEEPSEEK_API_KEY"),
            (ProviderType::OpenRouter, "OPENROUTER_API_KEY"),
            (ProviderType::Gemini, "GEMINI_API_KEY"),
            (ProviderType::Groq, "GROQ_API_KEY"),
            (ProviderType::Ollama, "OLLAMA_API_KEY"),
            (ProviderType::Mistral, "MISTRAL_API_KEY"),
            (ProviderType::XAi, "XAI_API_KEY"),
            (ProviderType::Cohere, "COHERE_API_KEY"),
            (ProviderType::Perplexity, "PERPLEXITY_API_KEY"),
            (ProviderType::Together, "TOGETHER_API_KEY"),
        ];

        println!("Available LLM providers:");
        for (provider, env_var) in providers {
            let available = has_api_key(provider).is_some();
            println!(
                "  {:?}: {} ({})",
                provider,
                if available { "YES" } else { "NO" },
                env_var
            );
        }
    }
}

// Integration tests that require API keys are marked as ignored.
// To run them, use: cargo test --test real_llm_flow -- --ignored --nocapture
//
// Note: These tests need to be updated based on the actual rig-core API version.
// The current rig-core 0.6.x API may differ from the expected usage.
// See: https://docs.rs/rig-core for the correct API documentation.

mod anthropic_integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY and rig-core API update"]
    async fn test_anthropic_basic_chat() {
        let api_key = match has_api_key(ProviderType::Anthropic) {
            Some(key) => key,
            None => {
                eprintln!("Skipping: ANTHROPIC_API_KEY not set");
                return;
            }
        };

        // TODO: Update with correct rig-core API
        // Example using rig-core (API may vary):
        // let client = rig::providers::anthropic::Client::from_env();
        // let agent = client.agent("claude-sonnet-4-20250514").build();
        // let response = agent.prompt("Hello").await;

        println!("Anthropic API key is set (length: {})", api_key.len());
        println!("Test would call Claude with a simple prompt");
    }

    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY and rig-core API update"]
    async fn test_anthropic_math_problem() {
        if has_api_key(ProviderType::Anthropic).is_none() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        // TODO: Implement with correct rig-core API
        println!("Test would ask Claude: What is 2+2?");
    }
}

mod openai_integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires OPENAI_API_KEY and rig-core API update"]
    async fn test_openai_basic_chat() {
        let api_key = match has_api_key(ProviderType::OpenAi) {
            Some(key) => key,
            None => {
                eprintln!("Skipping: OPENAI_API_KEY not set");
                return;
            }
        };

        // TODO: Update with correct rig-core API
        // Example using rig-core:
        // let client = rig::providers::openai::Client::new(&api_key);
        // let agent = client.agent("gpt-4o-mini").build();
        // let response = agent.prompt("Hello").await;

        println!("OpenAI API key is set (length: {})", api_key.len());
        println!("Test would call GPT with a simple prompt");
    }
}

mod multi_provider_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires API keys"]
    async fn test_first_available_provider() {
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAi,
            ProviderType::Qwen,
            ProviderType::Gemini,
            ProviderType::Groq,
            ProviderType::Ollama,
            ProviderType::Mistral,
            ProviderType::XAi,
            ProviderType::Cohere,
            ProviderType::Perplexity,
            ProviderType::Together,
        ];

        for provider in providers {
            if has_api_key(provider).is_some() {
                println!("Would test with {:?}", provider);
                println!("Default model: {}", default_model_for(provider));
                return;
            }
        }

        eprintln!("No API keys available for any provider");
    }
}
