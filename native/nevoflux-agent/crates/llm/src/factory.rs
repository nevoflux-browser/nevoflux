//! Provider factory for easy provider instantiation.

use std::str::FromStr;

/// Supported LLM provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    /// Anthropic Claude models
    Anthropic,
    /// OpenAI GPT models
    OpenAi,
    /// OpenRouter (multi-provider gateway)
    OpenRouter,
    /// DeepSeek models
    DeepSeek,
    /// Alibaba Qwen models (via DashScope)
    Qwen,
    /// Google Gemini models
    Gemini,
    /// Groq (fast inference)
    Groq,
    /// Ollama (local models)
    Ollama,
    /// Mistral AI models
    Mistral,
    /// xAI Grok models
    XAi,
    /// Cohere models
    Cohere,
    /// Perplexity models
    Perplexity,
    /// Together AI models
    Together,
    /// Claude Code CLI (subprocess)
    ClaudeCode,
    /// Gemini CLI (subprocess)
    GeminiCli,
    /// Kimi Agent CLI (subprocess, wire mode)
    KimiAgent,
    /// OpenClaw ACP agent (subprocess)
    OpenClaw,
}

impl FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(ProviderType::Anthropic),
            "openai" => Ok(ProviderType::OpenAi),
            "openrouter" => Ok(ProviderType::OpenRouter),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "qwen" => Ok(ProviderType::Qwen),
            "gemini" => Ok(ProviderType::Gemini),
            "groq" => Ok(ProviderType::Groq),
            "ollama" => Ok(ProviderType::Ollama),
            "mistral" => Ok(ProviderType::Mistral),
            "xai" | "grok" => Ok(ProviderType::XAi),
            "cohere" => Ok(ProviderType::Cohere),
            "perplexity" => Ok(ProviderType::Perplexity),
            "together" => Ok(ProviderType::Together),
            "claude-code" | "claude_code" => Ok(ProviderType::ClaudeCode),
            "gemini-cli" | "gemini_cli" => Ok(ProviderType::GeminiCli),
            "kimi-agent" | "kimi_agent" | "kimi" => Ok(ProviderType::KimiAgent),
            "openclaw" | "open_claw" | "open-claw" => Ok(ProviderType::OpenClaw),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

/// Configuration for creating an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// The type of provider
    pub provider: ProviderType,
    /// API key for authentication
    pub api_key: String,
    /// Optional custom base URL
    pub base_url: Option<String>,
    /// Optional default model name
    pub default_model: Option<String>,
}

impl ProviderConfig {
    /// Create a new provider configuration.
    pub fn new(provider: ProviderType, api_key: impl Into<String>) -> Self {
        Self {
            provider,
            api_key: api_key.into(),
            base_url: None,
            default_model: None,
        }
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the default model name.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }
}

/// Common Gemini model names for reference.
pub mod gemini_models {
    /// Gemini 3 Flash - Fast model with Pro-level intelligence (has free tier)
    pub const GEMINI_3_FLASH: &str = "gemini-3-flash-preview";
    /// Gemini 3 Pro - Reasoning-first model for complex agentic workflows
    pub const GEMINI_3_PRO: &str = "gemini-3-pro-preview";
    /// Gemini 2.5 Flash - Previous generation fast model
    pub const GEMINI_2_5_FLASH: &str = "gemini-2.5-flash";
    /// Gemini 2.5 Pro - Previous generation pro model
    pub const GEMINI_2_5_PRO: &str = "gemini-2.5-pro-preview-06-05";
    /// Gemini 2.0 Flash - Stable 2.0 model
    pub const GEMINI_2_0_FLASH: &str = "gemini-2.0-flash";
    /// Gemini 1.5 Flash - Legacy fast model
    pub const GEMINI_1_5_FLASH: &str = "gemini-1.5-flash";
}

/// Get the default model name for a provider type.
pub fn default_model_for(provider: ProviderType) -> &'static str {
    match provider {
        ProviderType::Anthropic => "claude-sonnet-4-20250514",
        ProviderType::OpenAi => "gpt-4o-mini",
        ProviderType::OpenRouter => "anthropic/claude-3-haiku",
        ProviderType::DeepSeek => "deepseek-chat",
        ProviderType::Qwen => "qwen-turbo",
        ProviderType::Gemini => gemini_models::GEMINI_3_FLASH,
        ProviderType::Groq => "llama-3.3-70b-versatile",
        ProviderType::Ollama => "llama3.2",
        ProviderType::Mistral => "mistral-small-latest",
        ProviderType::XAi => "grok-2-latest",
        ProviderType::Cohere => "command-r-plus",
        ProviderType::Perplexity => "llama-3.1-sonar-small-128k-online",
        ProviderType::Together => "meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo",
        ProviderType::ClaudeCode => "sonnet",
        ProviderType::GeminiCli => "gemini-2.5-pro",
        ProviderType::KimiAgent => "kimi-latest",
        ProviderType::OpenClaw => "default",
    }
}

/// Get the default context window size (in tokens) for a provider's default model.
pub fn default_context_window_for(provider: ProviderType) -> u32 {
    match provider {
        ProviderType::Anthropic => 200_000,
        ProviderType::OpenAi => 128_000,
        ProviderType::OpenRouter => 200_000,
        ProviderType::DeepSeek => 64_000,
        ProviderType::Qwen => 32_000,
        ProviderType::Gemini => 1_000_000,
        ProviderType::Groq => 128_000,
        ProviderType::Ollama => 8_000,
        ProviderType::Mistral => 32_000,
        ProviderType::XAi => 131_072,
        ProviderType::Cohere => 128_000,
        ProviderType::Perplexity => 128_000,
        ProviderType::Together => 8_000,
        ProviderType::ClaudeCode => 200_000,
        ProviderType::GeminiCli => 1_000_000,
        ProviderType::KimiAgent => 128_000,
        ProviderType::OpenClaw => 200_000,
    }
}

/// Get the environment variable name for a provider's API key.
pub fn api_key_env_var(provider: ProviderType) -> &'static str {
    match provider {
        ProviderType::Anthropic => "ANTHROPIC_API_KEY",
        ProviderType::OpenAi => "OPENAI_API_KEY",
        ProviderType::OpenRouter => "OPENROUTER_API_KEY",
        ProviderType::DeepSeek => "DEEPSEEK_API_KEY",
        ProviderType::Qwen => "DASHSCOPE_API_KEY",
        ProviderType::Gemini => "GEMINI_API_KEY",
        ProviderType::Groq => "GROQ_API_KEY",
        ProviderType::Ollama => "OLLAMA_API_KEY", // Usually not needed for local
        ProviderType::Mistral => "MISTRAL_API_KEY",
        ProviderType::XAi => "XAI_API_KEY",
        ProviderType::Cohere => "COHERE_API_KEY",
        ProviderType::Perplexity => "PERPLEXITY_API_KEY",
        ProviderType::Together => "TOGETHER_API_KEY",
        ProviderType::ClaudeCode => "ANTHROPIC_API_KEY",
        ProviderType::GeminiCli => "GEMINI_API_KEY",
        ProviderType::KimiAgent => "MOONSHOT_API_KEY",
        ProviderType::OpenClaw => "OPENCLAW_API_KEY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_equality() {
        assert_eq!(ProviderType::Anthropic, ProviderType::Anthropic);
        assert_ne!(ProviderType::Anthropic, ProviderType::OpenAi);
    }

    #[test]
    fn test_provider_config_new() {
        let config = ProviderConfig::new(ProviderType::Anthropic, "test-key");
        assert_eq!(config.provider, ProviderType::Anthropic);
        assert_eq!(config.api_key, "test-key");
        assert!(config.base_url.is_none());
        assert!(config.default_model.is_none());
    }

    #[test]
    fn test_provider_config_builder() {
        let config = ProviderConfig::new(ProviderType::Qwen, "qwen-key")
            .with_base_url("https://custom.example.com")
            .with_default_model("qwen-plus");

        assert_eq!(config.provider, ProviderType::Qwen);
        assert_eq!(
            config.base_url,
            Some("https://custom.example.com".to_string())
        );
        assert_eq!(config.default_model, Some("qwen-plus".to_string()));
    }

    #[test]
    fn test_default_model_for_anthropic() {
        assert_eq!(
            default_model_for(ProviderType::Anthropic),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_default_model_for_openai() {
        assert_eq!(default_model_for(ProviderType::OpenAi), "gpt-4o-mini");
    }

    #[test]
    fn test_default_model_for_qwen() {
        assert_eq!(default_model_for(ProviderType::Qwen), "qwen-turbo");
    }

    #[test]
    fn test_default_model_for_openrouter() {
        assert_eq!(
            default_model_for(ProviderType::OpenRouter),
            "anthropic/claude-3-haiku"
        );
    }

    #[test]
    fn test_default_model_for_deepseek() {
        assert_eq!(default_model_for(ProviderType::DeepSeek), "deepseek-chat");
    }

    #[test]
    fn test_default_model_for_gemini() {
        assert_eq!(
            default_model_for(ProviderType::Gemini),
            "gemini-3-flash-preview"
        );
    }

    #[test]
    fn test_default_model_for_groq() {
        assert_eq!(
            default_model_for(ProviderType::Groq),
            "llama-3.3-70b-versatile"
        );
    }

    #[test]
    fn test_default_model_for_ollama() {
        assert_eq!(default_model_for(ProviderType::Ollama), "llama3.2");
    }

    #[test]
    fn test_default_model_for_mistral() {
        assert_eq!(
            default_model_for(ProviderType::Mistral),
            "mistral-small-latest"
        );
    }

    #[test]
    fn test_default_model_for_xai() {
        assert_eq!(default_model_for(ProviderType::XAi), "grok-2-latest");
    }

    #[test]
    fn test_default_model_for_cohere() {
        assert_eq!(default_model_for(ProviderType::Cohere), "command-r-plus");
    }

    #[test]
    fn test_default_model_for_perplexity() {
        assert_eq!(
            default_model_for(ProviderType::Perplexity),
            "llama-3.1-sonar-small-128k-online"
        );
    }

    #[test]
    fn test_default_model_for_together() {
        assert_eq!(
            default_model_for(ProviderType::Together),
            "meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo"
        );
    }

    #[test]
    fn test_default_model_for_claude_code() {
        assert_eq!(default_model_for(ProviderType::ClaudeCode), "sonnet");
    }

    #[test]
    fn test_default_model_for_gemini_cli() {
        assert_eq!(default_model_for(ProviderType::GeminiCli), "gemini-2.5-pro");
    }

    #[test]
    fn test_provider_type_debug() {
        assert_eq!(format!("{:?}", ProviderType::Anthropic), "Anthropic");
        assert_eq!(format!("{:?}", ProviderType::Gemini), "Gemini");
        assert_eq!(format!("{:?}", ProviderType::XAi), "XAi");
    }

    #[test]
    fn test_provider_config_clone() {
        let config = ProviderConfig::new(ProviderType::OpenAi, "key");
        let cloned = config.clone();
        assert_eq!(cloned.provider, ProviderType::OpenAi);
    }

    #[test]
    fn test_provider_type_copy() {
        let provider = ProviderType::DeepSeek;
        let copied = provider;
        assert_eq!(provider, copied);
    }

    #[test]
    fn test_api_key_env_var() {
        assert_eq!(
            api_key_env_var(ProviderType::Anthropic),
            "ANTHROPIC_API_KEY"
        );
        assert_eq!(api_key_env_var(ProviderType::Gemini), "GEMINI_API_KEY");
        assert_eq!(api_key_env_var(ProviderType::Groq), "GROQ_API_KEY");
        assert_eq!(api_key_env_var(ProviderType::XAi), "XAI_API_KEY");
    }

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
    fn test_all_providers_have_default_models() {
        // Ensure no panic for all providers
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAi,
            ProviderType::OpenRouter,
            ProviderType::DeepSeek,
            ProviderType::Qwen,
            ProviderType::Gemini,
            ProviderType::Groq,
            ProviderType::Ollama,
            ProviderType::Mistral,
            ProviderType::XAi,
            ProviderType::Cohere,
            ProviderType::Perplexity,
            ProviderType::Together,
            ProviderType::ClaudeCode,
            ProviderType::GeminiCli,
            ProviderType::KimiAgent,
            ProviderType::OpenClaw,
        ];

        for provider in providers {
            let model = default_model_for(provider);
            assert!(
                !model.is_empty(),
                "Provider {:?} has empty default model",
                provider
            );
        }
    }

    #[test]
    fn test_all_providers_have_env_vars() {
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAi,
            ProviderType::OpenRouter,
            ProviderType::DeepSeek,
            ProviderType::Qwen,
            ProviderType::Gemini,
            ProviderType::Groq,
            ProviderType::Ollama,
            ProviderType::Mistral,
            ProviderType::XAi,
            ProviderType::Cohere,
            ProviderType::Perplexity,
            ProviderType::Together,
            ProviderType::ClaudeCode,
            ProviderType::GeminiCli,
            ProviderType::KimiAgent,
            ProviderType::OpenClaw,
        ];

        for provider in providers {
            let env_var = api_key_env_var(provider);
            assert!(
                !env_var.is_empty(),
                "Provider {:?} has empty env var",
                provider
            );
            assert!(
                env_var.ends_with("_API_KEY"),
                "Provider {:?} env var should end with _API_KEY",
                provider
            );
        }
    }

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(
            ProviderType::from_str("anthropic").unwrap(),
            ProviderType::Anthropic
        );
        assert_eq!(
            ProviderType::from_str("openai").unwrap(),
            ProviderType::OpenAi
        );
        assert_eq!(
            ProviderType::from_str("OPENAI").unwrap(),
            ProviderType::OpenAi
        );
        assert_eq!(ProviderType::from_str("qwen").unwrap(), ProviderType::Qwen);
        assert_eq!(
            ProviderType::from_str("deepseek").unwrap(),
            ProviderType::DeepSeek
        );
        assert_eq!(
            ProviderType::from_str("gemini").unwrap(),
            ProviderType::Gemini
        );
        assert_eq!(ProviderType::from_str("groq").unwrap(), ProviderType::Groq);
        assert_eq!(
            ProviderType::from_str("ollama").unwrap(),
            ProviderType::Ollama
        );
        assert_eq!(
            ProviderType::from_str("mistral").unwrap(),
            ProviderType::Mistral
        );
        assert_eq!(ProviderType::from_str("xai").unwrap(), ProviderType::XAi);
        assert_eq!(ProviderType::from_str("grok").unwrap(), ProviderType::XAi);
        assert_eq!(
            ProviderType::from_str("cohere").unwrap(),
            ProviderType::Cohere
        );
        assert_eq!(
            ProviderType::from_str("perplexity").unwrap(),
            ProviderType::Perplexity
        );
        assert_eq!(
            ProviderType::from_str("together").unwrap(),
            ProviderType::Together
        );
        assert_eq!(
            ProviderType::from_str("claude-code").unwrap(),
            ProviderType::ClaudeCode
        );
        assert_eq!(
            ProviderType::from_str("claude_code").unwrap(),
            ProviderType::ClaudeCode
        );
        assert_eq!(
            ProviderType::from_str("gemini-cli").unwrap(),
            ProviderType::GeminiCli
        );
        assert_eq!(
            ProviderType::from_str("gemini_cli").unwrap(),
            ProviderType::GeminiCli
        );
        assert_eq!(
            ProviderType::from_str("kimi-agent").unwrap(),
            ProviderType::KimiAgent
        );
        assert_eq!(
            ProviderType::from_str("kimi_agent").unwrap(),
            ProviderType::KimiAgent
        );
        assert_eq!(
            ProviderType::from_str("kimi").unwrap(),
            ProviderType::KimiAgent
        );
        assert_eq!(
            ProviderType::from_str("openclaw").unwrap(),
            ProviderType::OpenClaw
        );
        assert_eq!(
            ProviderType::from_str("open_claw").unwrap(),
            ProviderType::OpenClaw
        );
        assert_eq!(
            ProviderType::from_str("open-claw").unwrap(),
            ProviderType::OpenClaw
        );
    }

    #[test]
    fn test_provider_type_from_str_unknown() {
        let result = ProviderType::from_str("unknown_provider");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown provider"));
    }
}
