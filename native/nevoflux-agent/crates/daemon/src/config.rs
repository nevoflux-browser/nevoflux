//! Configuration file support for NevoFlux Agent.
//!
//! This module provides TOML-based configuration loading and saving
//! from the standard config directory (~/.config/nevoflux/config.toml).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::warn;

/// Errors that can occur during configuration operations.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("failed to read configuration file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse configuration file.
    #[error("failed to parse configuration file: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Failed to serialize configuration.
    #[error("failed to serialize configuration: {0}")]
    SerializeError(#[from] toml::ser::Error),

    /// No config directory found.
    #[error("could not determine config directory")]
    NoConfigDir,
}

/// Top-level agent configuration.
///
/// This is the root configuration structure that contains all subsystem
/// configurations. It can be loaded from ~/.config/nevoflux/config.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    /// Daemon-specific configuration.
    #[serde(default)]
    pub daemon: DaemonConfig,

    /// LLM provider configuration.
    #[serde(default)]
    pub llm: LlmConfig,

    /// Storage configuration.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,

    /// Authorization configuration.
    #[serde(default)]
    pub auth: AuthConfig,

    /// Learning system configuration.
    #[serde(default)]
    pub learning: LearningConfig,

    /// Embedding provider configuration.
    #[serde(default)]
    pub embedding: EmbeddingConfig,

    /// TTS subsystem configuration (umbrella spec §7).
    #[serde(default)]
    pub tts: TtsConfig,
}

/// TTS subsystem config — backends keyed by provider name.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TtsConfig {
    /// ElevenLabs API path config (P5b-1).
    #[serde(default)]
    pub elevenlabs: ElevenLabsConfig,
    /// Kokoro local ONNX path config (P5b-2). Inference is gated on
    /// the `model_path` and `voices_path` files existing on disk; until
    /// then `tts_synthesize_local` returns ConfigMissing.
    #[serde(default)]
    pub kokoro: KokoroConfig,
    /// Whisper local ONNX path config (P5b-3). Same gating contract as
    /// Kokoro — `tts_transcribe` returns ConfigMissing until
    /// `model_path` resolves.
    #[serde(default)]
    pub whisper: WhisperConfig,
}

/// Kokoro local TTS config.
///
/// `[tts.kokoro]` in `~/.config/nevoflux/config.toml`:
/// ```toml
/// [tts.kokoro]
/// model_path  = "~/.cache/nevoflux/models/kokoro-v1.0.int8.onnx"
/// voices_path = "~/.cache/nevoflux/models/kokoro-voices-v1.0.bin"
/// default_voice = "af"  # American female
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KokoroConfig {
    /// Filesystem path to the Kokoro ONNX model. None → tool returns
    /// ConfigMissing with download instructions.
    #[serde(default)]
    pub model_path: Option<String>,
    /// Filesystem path to the Kokoro voice bank.
    #[serde(default)]
    pub voices_path: Option<String>,
    /// Default voice tag (`af` / `am` / `bf` / `bm` / `zf` / `zm`).
    #[serde(default)]
    pub default_voice: Option<String>,
}

/// Whisper transcription config.
///
/// `[tts.whisper]` in `~/.config/nevoflux/config.toml`:
/// ```toml
/// [tts.whisper]
/// model_path = "~/.cache/nevoflux/models/whisper-base.onnx"
/// default_size = "base"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhisperConfig {
    /// Filesystem path to the Whisper ONNX model. None → tool returns
    /// ConfigMissing.
    #[serde(default)]
    pub model_path: Option<String>,
    /// Default model size (`tiny` / `base` / `small` / `medium`).
    #[serde(default)]
    pub default_size: Option<String>,
}

/// ElevenLabs HTTP API config.
///
/// Source `[tts.elevenlabs]` section in `~/.config/nevoflux/config.toml`:
/// ```toml
/// [tts.elevenlabs]
/// api_key = "sk_..."
/// default_voice_id = "21m00Tcm4TlvDq8ikWAM"  # Rachel
/// default_model_id = "eleven_multilingual_v2"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElevenLabsConfig {
    /// API key (`xi-api-key` header). When `None`, the
    /// `tts_synthesize_api` tool returns ConfigError so the agent can
    /// surface a clear "set ELEVENLABS_API_KEY" message.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Default voice ID used when the tool args don't specify one.
    /// ElevenLabs default: `21m00Tcm4TlvDq8ikWAM` (Rachel, female, en).
    #[serde(default)]
    pub default_voice_id: Option<String>,
    /// Default model ID. ElevenLabs default: `eleven_multilingual_v2`.
    #[serde(default)]
    pub default_model_id: Option<String>,
}

impl AgentConfig {
    /// Returns the default configuration file path.
    ///
    /// This is typically ~/.config/nevoflux/config.toml on Linux/macOS
    /// or %APPDATA%\nevoflux\config.toml on Windows.
    pub fn default_config_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
        let primary = config_dir.join("nevoflux").join("config.toml");

        if primary.exists() {
            return Ok(primary);
        }

        // Fallback: on macOS dirs::config_dir() returns ~/Library/Application Support,
        // but users commonly place config at ~/.config/nevoflux/config.toml.
        if let Some(home) = dirs::home_dir() {
            let xdg_fallback = home.join(".config").join("nevoflux").join("config.toml");
            if xdg_fallback.exists() {
                warn!(
                    "Config not found at {}, using fallback {}",
                    primary.display(),
                    xdg_fallback.display()
                );
                return Ok(xdg_fallback);
            }
        }

        // Neither exists; return the primary path (load_from_path handles missing files).
        Ok(primary)
    }

    /// Load configuration from the default path.
    ///
    /// Returns default configuration if the file doesn't exist.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_config_path()?;
        Self::load_from_path(&path)
    }

    /// Load configuration from a specific path.
    ///
    /// Returns default configuration if the file doesn't exist.
    pub fn load_from_path(path: &PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            let config = Self::default();
            if let Err(e) = config.save_to_path(path) {
                warn!(
                    "Failed to auto-create config file at {}: {}",
                    path.display(),
                    e
                );
            } else {
                tracing::info!("Auto-created config file at {}", path.display());
            }
            return Ok(config);
        }

        let content = std::fs::read_to_string(path)?;
        let config: AgentConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to the default path.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::default_config_path()?;
        self.save_to_path(&path)
    }

    /// Save configuration to a specific path.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save_to_path(&self, path: &PathBuf) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Merge with another configuration, preferring non-default values from other.
    pub fn merge(&mut self, other: &AgentConfig) {
        // Merge daemon config
        if other.daemon.port_range_start != DaemonConfig::default().port_range_start {
            self.daemon.port_range_start = other.daemon.port_range_start;
        }
        if other.daemon.port_range_end != DaemonConfig::default().port_range_end {
            self.daemon.port_range_end = other.daemon.port_range_end;
        }
        if other.daemon.idle_timeout_secs != DaemonConfig::default().idle_timeout_secs {
            self.daemon.idle_timeout_secs = other.daemon.idle_timeout_secs;
        }

        // Merge LLM config
        if other.llm.provider.is_some() {
            self.llm.provider = other.llm.provider.clone();
        }
        if other.llm.default_provider.is_some() {
            self.llm.default_provider = other.llm.default_provider.clone();
        }
        if other.llm.default_model.is_some() {
            self.llm.default_model = other.llm.default_model.clone();
        }
        if other.llm.max_tokens != LlmConfig::default().max_tokens {
            self.llm.max_tokens = other.llm.max_tokens;
        }
        // Merge provider-specific configs
        merge_provider(&mut self.llm.anthropic, &other.llm.anthropic);
        merge_provider(&mut self.llm.openai, &other.llm.openai);
        merge_provider(&mut self.llm.qwen, &other.llm.qwen);
        merge_provider(&mut self.llm.deepseek, &other.llm.deepseek);
        merge_provider(&mut self.llm.openrouter, &other.llm.openrouter);
        merge_provider(&mut self.llm.claude_code, &other.llm.claude_code);
        merge_provider(&mut self.llm.gemini_cli, &other.llm.gemini_cli);
        merge_provider(&mut self.llm.gemini, &other.llm.gemini);
        merge_provider(&mut self.llm.groq, &other.llm.groq);
        merge_provider(&mut self.llm.ollama, &other.llm.ollama);
        merge_provider(&mut self.llm.mistral, &other.llm.mistral);
        merge_provider(&mut self.llm.xai, &other.llm.xai);
        merge_provider(&mut self.llm.cohere, &other.llm.cohere);
        merge_provider(&mut self.llm.perplexity, &other.llm.perplexity);
        merge_provider(&mut self.llm.together, &other.llm.together);
        merge_provider(&mut self.llm.kimi_agent, &other.llm.kimi_agent);

        // Merge storage config
        if other.storage.data_dir.is_some() {
            self.storage.data_dir = other.storage.data_dir.clone();
        }
        if other.storage.max_size_mb != StorageConfig::default().max_size_mb {
            self.storage.max_size_mb = other.storage.max_size_mb;
        }

        // Merge logging config
        if other.logging.level != LoggingConfig::default().level {
            self.logging.level = other.logging.level.clone();
        }
        if other.logging.file.is_some() {
            self.logging.file = other.logging.file.clone();
        }

        // Merge auth config
        if other.auth.workspace_auto_allow != AuthConfig::default().workspace_auto_allow {
            self.auth.workspace_auto_allow = other.auth.workspace_auto_allow;
        }
        if !other.auth.allowed_commands.is_empty()
            && other.auth.allowed_commands != default_allowed_commands()
        {
            self.auth.allowed_commands = other.auth.allowed_commands.clone();
        }
        if !other.auth.sensitive_patterns.is_empty()
            && other.auth.sensitive_patterns != default_sensitive_patterns()
        {
            self.auth.sensitive_patterns = other.auth.sensitive_patterns.clone();
        }
        if !other.auth.denied_commands.is_empty() {
            self.auth.denied_commands = other.auth.denied_commands.clone();
        }

        // Merge embedding config
        if other.embedding.provider != default_embedding_provider() {
            self.embedding.provider = other.embedding.provider.clone();
        }
        if other.embedding.model != default_embedding_model() {
            self.embedding.model = other.embedding.model.clone();
        }
        if other.embedding.enabled != default_embedding_enabled() {
            self.embedding.enabled = other.embedding.enabled;
        }
    }
}

/// Merge a provider config, preferring non-None values from `other`.
fn merge_provider(target: &mut ProviderConfig, other: &ProviderConfig) {
    if other.api_key.is_some() {
        target.api_key = other.api_key.clone();
    }
    if other.model.is_some() {
        target.model = other.model.clone();
    }
    if other.context_window.is_some() {
        target.context_window = other.context_window;
    }
    if other.add_dirs.is_some() {
        target.add_dirs = other.add_dirs.clone();
    }
    if other.base_url.is_some() {
        target.base_url = other.base_url.clone();
    }
    if other.use_streaming.is_some() {
        target.use_streaming = other.use_streaming;
    }
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Active LLM provider (e.g., "anthropic", "openai", "qwen").
    #[serde(default)]
    pub provider: Option<String>,

    /// Default LLM provider (legacy, use `provider` instead).
    #[serde(default)]
    pub default_provider: Option<String>,

    /// Default model name (legacy).
    #[serde(default)]
    pub default_model: Option<String>,

    /// Anthropic-specific configuration.
    #[serde(default)]
    pub anthropic: ProviderConfig,

    /// OpenAI-specific configuration.
    #[serde(default)]
    pub openai: ProviderConfig,

    /// Qwen-specific configuration.
    #[serde(default)]
    pub qwen: ProviderConfig,

    /// DeepSeek-specific configuration.
    #[serde(default)]
    pub deepseek: ProviderConfig,

    /// Claude Code CLI-specific configuration.
    #[serde(default)]
    pub claude_code: ProviderConfig,

    /// OpenRouter-specific configuration.
    #[serde(default)]
    pub openrouter: ProviderConfig,

    /// Gemini CLI-specific configuration.
    #[serde(default)]
    pub gemini_cli: ProviderConfig,

    /// Gemini API-specific configuration.
    #[serde(default)]
    pub gemini: ProviderConfig,

    /// Groq-specific configuration.
    #[serde(default)]
    pub groq: ProviderConfig,

    /// Ollama-specific configuration.
    #[serde(default)]
    pub ollama: ProviderConfig,

    /// Mistral-specific configuration.
    #[serde(default)]
    pub mistral: ProviderConfig,

    /// XAI (Grok)-specific configuration.
    #[serde(default)]
    pub xai: ProviderConfig,

    /// Cohere-specific configuration.
    #[serde(default)]
    pub cohere: ProviderConfig,

    /// Perplexity-specific configuration.
    #[serde(default)]
    pub perplexity: ProviderConfig,

    /// Together AI-specific configuration.
    #[serde(default)]
    pub together: ProviderConfig,

    /// Kimi Agent CLI-specific configuration.
    #[serde(default)]
    pub kimi_agent: ProviderConfig,

    /// OpenClaw ACP-specific configuration.
    #[serde(default)]
    pub openclaw: ProviderConfig,

    /// Maximum tokens for responses.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Temperature for generation.
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum retries for failed requests.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

/// Provider-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// API key for this provider.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Model name for this provider.
    #[serde(default)]
    pub model: Option<String>,

    /// Context window size in tokens (overrides provider default).
    #[serde(default)]
    pub context_window: Option<u32>,

    /// Additional directories to pass via `--add-dir` (Claude Code CLI only).
    #[serde(default)]
    pub add_dirs: Option<Vec<String>>,

    /// Custom base URL for the API endpoint.
    #[serde(default)]
    pub base_url: Option<String>,

    /// Whether to use streaming for this provider.
    /// Set to `false` if the provider doesn't support SSE streaming properly.
    /// Defaults to `true` when not specified.
    #[serde(default)]
    pub use_streaming: Option<bool>,
}

impl LlmConfig {
    /// Get the active provider name.
    pub fn active_provider(&self) -> Option<&str> {
        self.provider
            .as_deref()
            .or(self.default_provider.as_deref())
    }

    /// Get the API key for the active provider.
    pub fn active_api_key(&self) -> Option<&str> {
        match self.active_provider()? {
            "anthropic" => self.anthropic.api_key.as_deref(),
            "openai" => self.openai.api_key.as_deref(),
            "qwen" => self.qwen.api_key.as_deref(),
            "deepseek" => self.deepseek.api_key.as_deref(),
            "openrouter" => self.openrouter.api_key.as_deref(),
            "claude-code" | "claude_code" => self
                .claude_code
                .api_key
                .as_deref()
                .or(Some("claude-code-cli")),
            "gemini-cli" | "gemini_cli" => {
                self.gemini_cli.api_key.as_deref().or(Some("gemini-cli"))
            }
            "gemini" => self.gemini.api_key.as_deref(),
            "groq" => self.groq.api_key.as_deref(),
            "ollama" => self.ollama.api_key.as_deref().or(Some("ollama-local")),
            "mistral" => self.mistral.api_key.as_deref(),
            "xai" | "grok" => self.xai.api_key.as_deref(),
            "cohere" => self.cohere.api_key.as_deref(),
            "perplexity" => self.perplexity.api_key.as_deref(),
            "together" => self.together.api_key.as_deref(),
            "kimi-agent" | "kimi_agent" | "kimi" => self
                .kimi_agent
                .api_key
                .as_deref()
                .or(Some("kimi-agent-cli")),
            "openclaw" | "open_claw" | "open-claw" => {
                self.openclaw.api_key.as_deref().or(Some("openclaw-acp"))
            }
            _ => None,
        }
    }

    /// Get the model for the active provider.
    pub fn active_model(&self) -> Option<&str> {
        match self.active_provider()? {
            "anthropic" => self.anthropic.model.as_deref(),
            "openai" => self.openai.model.as_deref(),
            "qwen" => self.qwen.model.as_deref(),
            "deepseek" => self.deepseek.model.as_deref(),
            "openrouter" => self.openrouter.model.as_deref(),
            "claude-code" | "claude_code" => self.claude_code.model.as_deref(),
            "gemini-cli" | "gemini_cli" => self.gemini_cli.model.as_deref(),
            "gemini" => self.gemini.model.as_deref(),
            "groq" => self.groq.model.as_deref(),
            "ollama" => self.ollama.model.as_deref(),
            "mistral" => self.mistral.model.as_deref(),
            "xai" | "grok" => self.xai.model.as_deref(),
            "cohere" => self.cohere.model.as_deref(),
            "perplexity" => self.perplexity.model.as_deref(),
            "together" => self.together.model.as_deref(),
            "kimi-agent" | "kimi_agent" | "kimi" => self.kimi_agent.model.as_deref(),
            "openclaw" | "open_claw" | "open-claw" => self.openclaw.model.as_deref(),
            _ => self.default_model.as_deref(),
        }
    }

    /// Get the configured model for a specific provider name.
    pub fn model_for_provider(&self, provider: &str) -> Option<&str> {
        match provider {
            "anthropic" => self.anthropic.model.as_deref(),
            "openai" => self.openai.model.as_deref(),
            "qwen" => self.qwen.model.as_deref(),
            "deepseek" => self.deepseek.model.as_deref(),
            "openrouter" => self.openrouter.model.as_deref(),
            "claude-code" | "claude_code" => self.claude_code.model.as_deref(),
            "gemini-cli" | "gemini_cli" => self.gemini_cli.model.as_deref(),
            "gemini" => self.gemini.model.as_deref(),
            "groq" => self.groq.model.as_deref(),
            "ollama" => self.ollama.model.as_deref(),
            "mistral" => self.mistral.model.as_deref(),
            "xai" | "grok" => self.xai.model.as_deref(),
            "cohere" => self.cohere.model.as_deref(),
            "perplexity" => self.perplexity.model.as_deref(),
            "together" => self.together.model.as_deref(),
            "kimi-agent" | "kimi_agent" | "kimi" => self.kimi_agent.model.as_deref(),
            "openclaw" | "open_claw" | "open-claw" => self.openclaw.model.as_deref(),
            _ => None,
        }
    }

    /// Get the base URL for the active provider.
    pub fn active_base_url(&self) -> Option<&str> {
        match self.active_provider()? {
            "anthropic" => self.anthropic.base_url.as_deref(),
            "openai" => self.openai.base_url.as_deref(),
            "qwen" => self.qwen.base_url.as_deref(),
            "deepseek" => self.deepseek.base_url.as_deref(),
            "openrouter" => self.openrouter.base_url.as_deref(),
            "claude-code" | "claude_code" => self.claude_code.base_url.as_deref(),
            "gemini-cli" | "gemini_cli" => self.gemini_cli.base_url.as_deref(),
            "gemini" => self.gemini.base_url.as_deref(),
            "groq" => self.groq.base_url.as_deref(),
            "ollama" => self.ollama.base_url.as_deref(),
            "mistral" => self.mistral.base_url.as_deref(),
            "xai" | "grok" => self.xai.base_url.as_deref(),
            "cohere" => self.cohere.base_url.as_deref(),
            "perplexity" => self.perplexity.base_url.as_deref(),
            "together" => self.together.base_url.as_deref(),
            "kimi-agent" | "kimi_agent" | "kimi" => self.kimi_agent.base_url.as_deref(),
            "openclaw" | "open_claw" | "open-claw" => self.openclaw.base_url.as_deref(),
            _ => None,
        }
    }

    /// Get the base URL for a specific provider by name.
    pub fn base_url_for_provider(&self, provider: &str) -> Option<&str> {
        match provider {
            "anthropic" => self.anthropic.base_url.as_deref(),
            "openai" => self.openai.base_url.as_deref(),
            "qwen" => self.qwen.base_url.as_deref(),
            "deepseek" => self.deepseek.base_url.as_deref(),
            "openrouter" => self.openrouter.base_url.as_deref(),
            "claude-code" | "claude_code" => self.claude_code.base_url.as_deref(),
            "gemini-cli" | "gemini_cli" => self.gemini_cli.base_url.as_deref(),
            "gemini" => self.gemini.base_url.as_deref(),
            "groq" => self.groq.base_url.as_deref(),
            "ollama" => self.ollama.base_url.as_deref(),
            "mistral" => self.mistral.base_url.as_deref(),
            "xai" | "grok" => self.xai.base_url.as_deref(),
            "cohere" => self.cohere.base_url.as_deref(),
            "perplexity" => self.perplexity.base_url.as_deref(),
            "together" => self.together.base_url.as_deref(),
            "kimi-agent" | "kimi_agent" | "kimi" => self.kimi_agent.base_url.as_deref(),
            "openclaw" | "open_claw" | "open-claw" => self.openclaw.base_url.as_deref(),
            _ => None,
        }
    }

    /// Get use_streaming for the active provider. Defaults to true.
    pub fn active_use_streaming(&self) -> bool {
        match self.active_provider() {
            Some(p) => self.use_streaming_for_provider(p),
            None => true,
        }
    }

    /// Get use_streaming for a specific provider.
    /// Defaults to `false` for providers that don't support streaming (Qwen, Ollama),
    /// `true` for all others.
    pub fn use_streaming_for_provider(&self, provider: &str) -> bool {
        let value = match provider {
            "anthropic" => self.anthropic.use_streaming,
            "openai" => self.openai.use_streaming,
            "qwen" => self.qwen.use_streaming,
            "deepseek" => self.deepseek.use_streaming,
            "openrouter" => self.openrouter.use_streaming,
            "claude-code" | "claude_code" => self.claude_code.use_streaming,
            "gemini-cli" | "gemini_cli" => self.gemini_cli.use_streaming,
            "gemini" => self.gemini.use_streaming,
            "groq" => self.groq.use_streaming,
            "ollama" => self.ollama.use_streaming,
            "mistral" => self.mistral.use_streaming,
            "xai" | "grok" => self.xai.use_streaming,
            "cohere" => self.cohere.use_streaming,
            "perplexity" => self.perplexity.use_streaming,
            "together" => self.together.use_streaming,
            "kimi-agent" | "kimi_agent" | "kimi" => self.kimi_agent.use_streaming,
            "openclaw" | "open_claw" | "open-claw" => self.openclaw.use_streaming,
            _ => None,
        };
        // Providers that don't support streaming default to false
        let default = !matches!(provider, "ollama");
        value.unwrap_or(default)
    }

    /// Get list of configured providers with their model names.
    /// Returns (provider_name, model_name) pairs for all providers with API keys.
    pub fn configured_providers(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let active = self.active_provider();
        let providers: [(&str, &ProviderConfig); 17] = [
            ("anthropic", &self.anthropic),
            ("openai", &self.openai),
            ("openrouter", &self.openrouter),
            ("qwen", &self.qwen),
            ("deepseek", &self.deepseek),
            ("claude-code", &self.claude_code),
            ("gemini-cli", &self.gemini_cli),
            ("gemini", &self.gemini),
            ("groq", &self.groq),
            ("ollama", &self.ollama),
            ("mistral", &self.mistral),
            ("xai", &self.xai),
            ("cohere", &self.cohere),
            ("perplexity", &self.perplexity),
            ("together", &self.together),
            ("kimi-agent", &self.kimi_agent),
            ("openclaw", &self.openclaw),
        ];

        for (name, config) in &providers {
            if config.api_key.is_some() {
                let model = config.model.clone().unwrap_or_else(|| match name
                    .parse::<nevoflux_llm::ProviderType>(
                ) {
                    Ok(pt) => nevoflux_llm::default_model_for(pt).to_string(),
                    Err(_) => name.to_string(),
                });
                let is_active = active == Some(*name);
                let suffix = if is_active { " (active)" } else { "" };
                result.push((name.to_string(), format!("{}{}", model, suffix)));
            }
        }
        result
    }

    /// Get the context window size for the active provider.
    ///
    /// Resolution order:
    /// 1. Provider-specific `context_window` from config
    /// 2. Known default for the provider type
    /// 3. Fallback: 128,000 tokens
    pub fn context_window(&self) -> u32 {
        use nevoflux_llm::ProviderType;

        // Check provider-specific config override
        let provider_config_window = match self.active_provider() {
            Some("anthropic") => self.anthropic.context_window,
            Some("openai") => self.openai.context_window,
            Some("qwen") => self.qwen.context_window,
            Some("deepseek") => self.deepseek.context_window,
            Some("claude-code") | Some("claude_code") => self.claude_code.context_window,
            Some("gemini-cli") | Some("gemini_cli") => self.gemini_cli.context_window,
            Some("gemini") => self.gemini.context_window,
            Some("groq") => self.groq.context_window,
            Some("ollama") => self.ollama.context_window,
            Some("mistral") => self.mistral.context_window,
            Some("xai") | Some("grok") => self.xai.context_window,
            Some("cohere") => self.cohere.context_window,
            Some("perplexity") => self.perplexity.context_window,
            Some("together") => self.together.context_window,
            Some("kimi-agent") | Some("kimi_agent") | Some("kimi") => {
                self.kimi_agent.context_window
            }
            Some("openclaw") | Some("open_claw") | Some("open-claw") => {
                self.openclaw.context_window
            }
            _ => None,
        };

        if let Some(window) = provider_config_window {
            return window;
        }

        // Fall back to known provider default
        if let Some(provider_name) = self.active_provider() {
            if let Ok(provider_type) = provider_name.parse::<ProviderType>() {
                return nevoflux_llm::default_context_window_for(provider_type);
            }
        }

        // Ultimate fallback
        128_000
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: None,
            default_provider: None,
            default_model: None,
            anthropic: ProviderConfig::default(),
            openai: ProviderConfig::default(),
            qwen: ProviderConfig::default(),
            deepseek: ProviderConfig::default(),
            openrouter: ProviderConfig::default(),
            claude_code: ProviderConfig::default(),
            gemini_cli: ProviderConfig::default(),
            gemini: ProviderConfig::default(),
            groq: ProviderConfig::default(),
            ollama: ProviderConfig::default(),
            mistral: ProviderConfig::default(),
            xai: ProviderConfig::default(),
            cohere: ProviderConfig::default(),
            perplexity: ProviderConfig::default(),
            together: ProviderConfig::default(),
            kimi_agent: ProviderConfig::default(),
            openclaw: ProviderConfig::default(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            timeout_secs: default_timeout_secs(),
            max_retries: default_max_retries(),
        }
    }
}

fn default_max_tokens() -> u32 {
    // 32768 covers reasoning-style models (Anthropic Sonnet 4.5 thinking,
    // mimo-v2.5-pro, etc.) where the same `max_tokens` budget pays for
    // BOTH internal thinking AND visible output. With 4096 the model can
    // burn the whole budget on thinking and emit zero visible content
    // (observed in /tmp/nevoflux-debug.log: round 3 streamed for 75s
    // with 0 text + 0 tool calls). 32768 leaves headroom for chain-of-
    // thought + a meaningful tool-calling response. Modern Claude
    // models support this size; Anthropic's docs recommend setting the
    // model's max output cap when in doubt.
    32_768
}

fn default_temperature() -> f32 {
    0.7
}

fn default_timeout_secs() -> u64 {
    120
}

fn default_max_retries() -> u32 {
    3
}

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Custom data directory path.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,

    /// Maximum storage size in MB.
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u64,

    /// Whether to enable WAL mode for SQLite.
    #[serde(default = "default_true")]
    pub wal_mode: bool,

    /// Whether to vacuum database on startup.
    #[serde(default)]
    pub vacuum_on_startup: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            max_size_mb: default_max_size_mb(),
            wal_mode: default_true(),
            vacuum_on_startup: false,
        }
    }
}

fn default_max_size_mb() -> u64 {
    1024
}

fn default_true() -> bool {
    true
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Optional log file path.
    #[serde(default)]
    pub file: Option<PathBuf>,

    /// Whether to log to stdout.
    #[serde(default = "default_true")]
    pub stdout: bool,

    /// Whether to use JSON format.
    #[serde(default)]
    pub json_format: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
            stdout: true,
            json_format: false,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Configuration for the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Port range start for daemon server.
    pub port_range_start: u16,
    /// Port range end for daemon server.
    pub port_range_end: u16,
    /// Idle timeout in seconds before daemon shuts down.
    pub idle_timeout_secs: u64,
    /// Heartbeat timeout in seconds for proxy connections.
    pub heartbeat_timeout_secs: u64,
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,
    /// Maximum number of concurrent requests.
    pub max_concurrent_requests: usize,
    /// Whether to keep alive for MCP connections.
    pub keep_alive_for_mcp: bool,
    /// Session configuration.
    pub session: SessionConfig,
    /// Context configuration.
    pub context: ContextConfig,
    /// Subagent configuration for WASM sandboxed sub-agents.
    #[serde(default)]
    pub subagent: SubagentConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port_range_start: 19500,
            port_range_end: 19600,
            idle_timeout_secs: 1800, // 30 minutes
            heartbeat_timeout_secs: 30,
            heartbeat_interval_secs: 10,
            max_concurrent_requests: 100,
            keep_alive_for_mcp: true,
            session: SessionConfig::default(),
            context: ContextConfig::default(),
            subagent: SubagentConfig::default(),
        }
    }
}

impl DaemonConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the idle timeout.
    pub fn with_idle_timeout(mut self, secs: u64) -> Self {
        self.idle_timeout_secs = secs;
        self
    }

    /// Set the heartbeat timeout.
    pub fn with_heartbeat_timeout(mut self, secs: u64) -> Self {
        self.heartbeat_timeout_secs = secs;
        self
    }

    /// Set keep alive for MCP.
    pub fn with_keep_alive_for_mcp(mut self, keep_alive: bool) -> Self {
        self.keep_alive_for_mcp = keep_alive;
        self
    }
}

/// Session management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maximum number of sessions to keep.
    pub max_sessions: u32,
    /// Days after which inactive sessions are cleaned up.
    pub inactive_days: u32,
    /// Maximum storage size in MB.
    pub max_storage_mb: u32,
    /// Whether to auto-create sessions.
    pub auto_create: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_sessions: 500,
            inactive_days: 90,
            max_storage_mb: 500,
            auto_create: true,
        }
    }
}

/// Context building configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Reserved tokens for system prompt.
    pub system_prompt_reserve: u32,
    /// Safety margin tokens.
    pub safety_margin: u32,
    /// Maximum history messages to include.
    pub max_history_messages: u32,
    /// Whether to include memory in context.
    pub include_memory: bool,
    /// Whether to include current page info.
    pub include_current_page: bool,
    /// Enable automatic context compression.
    #[serde(default = "default_enable_compression")]
    pub enable_compression: bool,
    /// Token threshold to trigger compression (% of history budget).
    #[serde(default = "default_compression_threshold")]
    pub compression_threshold_percent: u32,
    /// Number of recent messages to keep uncompressed.
    #[serde(default = "default_keep_recent")]
    pub keep_recent_messages: u32,
    /// Model for summarization (default: gpt-4o-mini).
    #[serde(default)]
    pub summarization_model: Option<String>,
    /// Max tokens for summary output.
    #[serde(default = "default_summary_max_tokens")]
    pub summary_max_tokens: u32,
    /// Maximum consecutive compression failures before circuit breaker opens.
    #[serde(default = "default_max_compression_failures")]
    pub max_compression_failures: u32,
    /// Cooldown in seconds before circuit breaker allows a probe attempt.
    #[serde(default = "default_compression_cooldown_secs")]
    pub compression_cooldown_secs: u64,
    /// Number of recent large tool results to keep during microcompaction.
    #[serde(default = "default_microcompact_keep_recent")]
    pub microcompact_keep_recent: usize,
    /// Minimum content length (chars) for a tool result to be eligible for clearing.
    #[serde(default = "default_microcompact_content_threshold")]
    pub microcompact_content_threshold: usize,
    /// Minutes of inactivity before forcing full microcompact (0 = disabled).
    #[serde(default = "default_time_gap_threshold_minutes")]
    pub time_gap_threshold_minutes: u64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            system_prompt_reserve: 2000,
            safety_margin: 500,
            max_history_messages: 50,
            include_memory: true,
            include_current_page: true,
            enable_compression: default_enable_compression(),
            compression_threshold_percent: default_compression_threshold(),
            keep_recent_messages: default_keep_recent(),
            summarization_model: None,
            summary_max_tokens: default_summary_max_tokens(),
            max_compression_failures: default_max_compression_failures(),
            compression_cooldown_secs: default_compression_cooldown_secs(),
            microcompact_keep_recent: default_microcompact_keep_recent(),
            microcompact_content_threshold: default_microcompact_content_threshold(),
            time_gap_threshold_minutes: default_time_gap_threshold_minutes(),
        }
    }
}

fn default_enable_compression() -> bool {
    true
}

fn default_compression_threshold() -> u32 {
    80
}

fn default_keep_recent() -> u32 {
    6
}

fn default_summary_max_tokens() -> u32 {
    500
}

fn default_max_compression_failures() -> u32 {
    3
}

fn default_compression_cooldown_secs() -> u64 {
    300
}

fn default_microcompact_keep_recent() -> usize {
    5
}

fn default_microcompact_content_threshold() -> usize {
    1000
}

fn default_time_gap_threshold_minutes() -> u64 {
    30
}

// ==================== Subagent Configuration ====================

/// Subagent resource limits and configuration.
///
/// This configuration controls how sub-agents are executed in isolated
/// WASM instances with resource constraints for security and stability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum concurrent subagents per session.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Execution timeout in seconds.
    #[serde(default = "default_subagent_timeout_secs")]
    pub timeout_secs: u64,

    /// Memory limit in WASM pages (64KB each).
    /// Default: 4096 pages = 256MB.
    #[serde(default = "default_memory_pages")]
    pub memory_pages: u32,

    /// Fuel limit for execution (None = unlimited).
    /// Fuel is consumed by WASM instructions and provides CPU limiting.
    #[serde(default)]
    pub fuel_limit: Option<u64>,
}

fn default_max_concurrent() -> usize {
    5
}

fn default_subagent_timeout_secs() -> u64 {
    300
}

fn default_memory_pages() -> u32 {
    4096
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            timeout_secs: default_subagent_timeout_secs(),
            memory_pages: default_memory_pages(),
            fuel_limit: None,
        }
    }
}

impl SubagentConfig {
    /// Create a new SubagentConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum concurrent subagents.
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Set the execution timeout in seconds.
    pub fn with_timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set the memory limit in WASM pages.
    pub fn with_memory_pages(mut self, pages: u32) -> Self {
        self.memory_pages = pages;
        self
    }

    /// Set the fuel limit for execution.
    pub fn with_fuel_limit(mut self, fuel: u64) -> Self {
        self.fuel_limit = Some(fuel);
        self
    }

    /// Get memory limit in bytes.
    pub fn memory_bytes(&self) -> u64 {
        self.memory_pages as u64 * 65536 // 64KB per page
    }
}

// ==================== LearningConfig ====================

/// Configuration for the self-learning system.
///
/// Controls how the agent learns from interactions, validates learned
/// patterns, and promotes them to long-term memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LearningConfig {
    /// Whether the learning system is enabled.
    pub enabled: bool,
    /// Number of pending observations before flushing to storage.
    pub flush_threshold: usize,
    /// Interval in seconds between automatic flushes.
    pub flush_interval_secs: u64,
    /// Maximum number of learning events per hour.
    pub rate_limit_per_hour: u32,
    /// Optional custom directory for soul/memory files.
    pub soul_dir: Option<String>,
    /// Validation thresholds for learned patterns.
    pub validation: ValidationConfig,
    /// Promotion thresholds for graduating patterns.
    pub promotion: PromotionConfig,
    /// Enable automatic session memory extraction via LLM.
    #[serde(default = "default_enable_session_extraction")]
    pub enable_session_extraction: bool,
    /// Extract knowledge every N user messages.
    #[serde(default = "default_extraction_interval")]
    pub extraction_interval: u32,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            flush_threshold: 20,
            flush_interval_secs: 30,
            rate_limit_per_hour: 5,
            soul_dir: None,
            validation: ValidationConfig::default(),
            promotion: PromotionConfig::default(),
            enable_session_extraction: default_enable_session_extraction(),
            extraction_interval: default_extraction_interval(),
        }
    }
}

/// Validation thresholds for learned patterns.
///
/// A pattern must meet these criteria before being considered valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationConfig {
    /// Minimum hours a pattern must survive before validation.
    pub min_alive_hours: u64,
    /// Minimum number of occurrences before validation.
    pub min_occurrences: u32,
    /// Minimum confidence score (0.0 - 1.0) before validation.
    pub min_confidence: f64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            min_alive_hours: 12,
            min_occurrences: 2,
            min_confidence: 0.6,
        }
    }
}

/// Promotion thresholds for graduating learned patterns to long-term memory.
///
/// Different pattern categories have different promotion criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PromotionConfig {
    /// Minimum hits for site interaction patterns.
    pub site_interaction_min_hits: u32,
    /// Minimum effectiveness for site interaction patterns.
    pub site_interaction_min_effectiveness: f64,
    /// Minimum hits for tool optimization patterns.
    pub tool_optimization_min_hits: u32,
    /// Minimum effectiveness for tool optimization patterns.
    pub tool_optimization_min_effectiveness: f64,
    /// Minimum hits for user preference patterns.
    pub user_preference_min_hits: u32,
    /// Minimum days a pattern must survive before promotion.
    pub min_alive_days: u64,
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            site_interaction_min_hits: 3,
            site_interaction_min_effectiveness: 0.6,
            tool_optimization_min_hits: 5,
            tool_optimization_min_effectiveness: 0.6,
            user_preference_min_hits: 2,
            min_alive_days: 3,
        }
    }
}

fn default_enable_session_extraction() -> bool {
    true
}

fn default_extraction_interval() -> u32 {
    5
}

// ==================== EmbeddingConfig ====================

/// Configuration for the embedding provider.
///
/// Controls which embedding provider and model the daemon uses for
/// generating vector embeddings (e.g., for semantic search in the
/// learning system).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// The embedding provider to use (e.g. "fastembed").
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    /// The embedding model name.
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Whether embedding generation is enabled.
    #[serde(default = "default_embedding_enabled")]
    pub enabled: bool,
}

fn default_embedding_provider() -> String {
    "fastembed".into()
}
fn default_embedding_model() -> String {
    "multilingual-e5-small".into()
}
fn default_embedding_enabled() -> bool {
    true
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_embedding_provider(),
            model: default_embedding_model(),
            enabled: default_embedding_enabled(),
        }
    }
}

// ==================== AuthConfig ====================

/// Authorization configuration for tool access control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Auto-allow Read/Grep inside working directory.
    #[serde(default = "default_true")]
    pub workspace_auto_allow: bool,
    /// Global command whitelist patterns (e.g. "cargo *", "git *").
    #[serde(default = "default_allowed_commands")]
    pub allowed_commands: Vec<String>,
    /// Sensitive file patterns (e.g. ".env*", "*credential*").
    #[serde(default = "default_sensitive_patterns")]
    pub sensitive_patterns: Vec<String>,
    /// Denied command patterns (e.g. "rm -rf *", "sudo *").
    #[serde(default)]
    pub denied_commands: Vec<String>,
}

fn default_allowed_commands() -> Vec<String> {
    vec![
        "cargo *".to_string(),
        "git *".to_string(),
        "npm *".to_string(),
        "just *".to_string(),
    ]
}

fn default_sensitive_patterns() -> Vec<String> {
    vec![
        ".env*".to_string(),
        "*credential*".to_string(),
        "*secret*".to_string(),
        "*_key*".to_string(),
        "*.pem".to_string(),
    ]
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            workspace_auto_allow: true,
            allowed_commands: default_allowed_commands(),
            sensitive_patterns: default_sensitive_patterns(),
            denied_commands: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();

        assert_eq!(config.port_range_start, 19500);
        assert_eq!(config.port_range_end, 19600);
        assert_eq!(config.idle_timeout_secs, 1800);
        assert_eq!(config.heartbeat_timeout_secs, 30);
    }

    #[test]
    fn test_daemon_config_builder() {
        let config = DaemonConfig::new()
            .with_idle_timeout(3600)
            .with_heartbeat_timeout(60)
            .with_keep_alive_for_mcp(false);

        assert_eq!(config.idle_timeout_secs, 3600);
        assert_eq!(config.heartbeat_timeout_secs, 60);
        assert!(!config.keep_alive_for_mcp);
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();

        assert_eq!(config.max_sessions, 500);
        assert_eq!(config.inactive_days, 90);
        assert!(config.auto_create);
    }

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();

        assert_eq!(config.system_prompt_reserve, 2000);
        assert!(config.include_memory);
        assert!(config.enable_compression);
        assert_eq!(config.compression_threshold_percent, 80);
        assert_eq!(config.keep_recent_messages, 6);
        assert!(config.summarization_model.is_none());
        assert_eq!(config.summary_max_tokens, 500);
    }

    #[test]
    fn test_config_serialization() {
        let config = DaemonConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: DaemonConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.idle_timeout_secs, decoded.idle_timeout_secs);
    }

    // New tests for AgentConfig and file operations

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();

        // Check daemon defaults are applied
        assert_eq!(config.daemon.port_range_start, 19500);
        assert_eq!(config.daemon.idle_timeout_secs, 1800);

        // Check LLM defaults
        assert_eq!(config.llm.max_tokens, 32_768);
        assert_eq!(config.llm.temperature, 0.7);
        assert!(config.llm.provider.is_none());
        assert!(config.llm.default_provider.is_none());

        // Check storage defaults
        assert_eq!(config.storage.max_size_mb, 1024);
        assert!(config.storage.wal_mode);

        // Check logging defaults
        assert_eq!(config.logging.level, "info");
        assert!(config.logging.stdout);
    }

    #[test]
    fn test_config_load_from_nonexistent_returns_default() {
        let path = PathBuf::from("/nonexistent/path/config.toml");
        let config = AgentConfig::load_from_path(&path).unwrap();

        assert_eq!(config.daemon.port_range_start, 19500);
        assert_eq!(config.llm.max_tokens, 32_768);
    }

    #[test]
    fn test_config_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Create a config with custom values
        let mut config = AgentConfig::default();
        config.daemon.port_range_start = 20000;
        config.daemon.idle_timeout_secs = 3600;
        config.llm.default_provider = Some("anthropic".to_string());
        config.llm.default_model = Some("claude-3".to_string());
        config.llm.max_tokens = 8192;
        config.storage.data_dir = Some(PathBuf::from("/custom/data"));
        config.logging.level = "debug".to_string();

        // Save the config
        config.save_to_path(&config_path).unwrap();

        // Verify file exists
        assert!(config_path.exists());

        // Load it back
        let loaded = AgentConfig::load_from_path(&config_path).unwrap();

        assert_eq!(loaded.daemon.port_range_start, 20000);
        assert_eq!(loaded.daemon.idle_timeout_secs, 3600);
        assert_eq!(loaded.llm.default_provider, Some("anthropic".to_string()));
        assert_eq!(loaded.llm.default_model, Some("claude-3".to_string()));
        assert_eq!(loaded.llm.max_tokens, 8192);
        assert_eq!(loaded.storage.data_dir, Some(PathBuf::from("/custom/data")));
        assert_eq!(loaded.logging.level, "debug");
    }

    #[test]
    fn test_config_toml_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let mut config = AgentConfig::default();
        config.llm.default_provider = Some("openai".to_string());
        config.save_to_path(&config_path).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();

        // Verify TOML structure
        assert!(content.contains("[daemon]"));
        assert!(content.contains("[llm]"));
        assert!(content.contains("[storage]"));
        assert!(content.contains("[logging]"));
        assert!(content.contains("default_provider = \"openai\""));
    }

    #[test]
    fn test_config_partial_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Write a partial config (only LLM section)
        let partial_config = r#"
[llm]
default_provider = "qwen"
max_tokens = 2048

[logging]
level = "warn"
"#;
        std::fs::write(&config_path, partial_config).unwrap();

        // Load it - should use defaults for missing sections
        let config = AgentConfig::load_from_path(&config_path).unwrap();

        // Custom values should be loaded
        assert_eq!(config.llm.default_provider, Some("qwen".to_string()));
        assert_eq!(config.llm.max_tokens, 2048);
        assert_eq!(config.logging.level, "warn");

        // Default values should be applied for missing fields
        assert_eq!(config.daemon.port_range_start, 19500);
        assert_eq!(config.storage.max_size_mb, 1024);
    }

    #[test]
    fn test_config_creates_parent_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir
            .path()
            .join("nested")
            .join("dirs")
            .join("config.toml");

        let config = AgentConfig::default();
        config.save_to_path(&config_path).unwrap();

        assert!(config_path.exists());
    }

    #[test]
    fn test_config_merge() {
        let mut base = AgentConfig::default();
        let mut other = AgentConfig::default();

        // Set some non-default values in other
        other.daemon.port_range_start = 21000;
        other.llm.provider = Some("anthropic".to_string());
        other.storage.data_dir = Some(PathBuf::from("/merged/path"));
        other.logging.level = "trace".to_string();

        base.merge(&other);

        // Merged values should be applied
        assert_eq!(base.daemon.port_range_start, 21000);
        assert_eq!(base.llm.provider, Some("anthropic".to_string()));
        assert_eq!(base.storage.data_dir, Some(PathBuf::from("/merged/path")));
        assert_eq!(base.logging.level, "trace");

        // Values that weren't changed should keep their defaults
        assert_eq!(base.daemon.idle_timeout_secs, 1800);
        assert_eq!(base.llm.max_tokens, 32_768);
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();

        assert!(config.provider.is_none());
        assert!(config.default_provider.is_none());
        assert!(config.default_model.is_none());
        assert_eq!(config.max_tokens, 32_768);
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_llm_config_active_provider() {
        let mut config = LlmConfig::default();
        config.provider = Some("openai".to_string());
        config.openai.api_key = Some("test-key".to_string());
        config.openai.model = Some("gpt-4o".to_string());

        assert_eq!(config.active_provider(), Some("openai"));
        assert_eq!(config.active_api_key(), Some("test-key"));
        assert_eq!(config.active_model(), Some("gpt-4o"));
    }

    #[test]
    fn test_llm_config_fallback_to_default_provider() {
        let mut config = LlmConfig::default();
        config.default_provider = Some("anthropic".to_string());
        config.anthropic.api_key = Some("sk-ant-xxx".to_string());

        assert_eq!(config.active_provider(), Some("anthropic"));
        assert_eq!(config.active_api_key(), Some("sk-ant-xxx"));
    }

    #[test]
    fn test_storage_config_defaults() {
        let config = StorageConfig::default();

        assert!(config.data_dir.is_none());
        assert_eq!(config.max_size_mb, 1024);
        assert!(config.wal_mode);
        assert!(!config.vacuum_on_startup);
    }

    #[test]
    fn test_logging_config_defaults() {
        let config = LoggingConfig::default();

        assert_eq!(config.level, "info");
        assert!(config.file.is_none());
        assert!(config.stdout);
        assert!(!config.json_format);
    }

    #[test]
    fn test_default_config_path() {
        // This test just verifies the path logic works
        let result = AgentConfig::default_config_path();

        // On most systems this should succeed
        if let Ok(path) = result {
            assert!(path.ends_with("config.toml"));
            assert!(path.to_string_lossy().contains("nevoflux"));
        }
    }

    #[test]
    fn test_config_invalid_toml_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Write invalid TOML
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(b"this is not valid toml {{{{").unwrap();

        let result = AgentConfig::load_from_path(&config_path);
        assert!(result.is_err());

        match result {
            Err(ConfigError::ParseError(_)) => (),
            _ => panic!("Expected ParseError"),
        }
    }

    // ==================== SubagentConfig Tests ====================

    #[test]
    fn test_subagent_config_defaults() {
        let config = SubagentConfig::default();

        assert_eq!(config.max_concurrent, 5);
        assert_eq!(config.timeout_secs, 300);
        assert_eq!(config.memory_pages, 4096);
        assert!(config.fuel_limit.is_none());
    }

    #[test]
    fn test_subagent_config_builder() {
        let config = SubagentConfig::new()
            .with_max_concurrent(10)
            .with_timeout_secs(600)
            .with_memory_pages(8192)
            .with_fuel_limit(1_000_000);

        assert_eq!(config.max_concurrent, 10);
        assert_eq!(config.timeout_secs, 600);
        assert_eq!(config.memory_pages, 8192);
        assert_eq!(config.fuel_limit, Some(1_000_000));
    }

    #[test]
    fn test_subagent_config_memory_bytes() {
        let config = SubagentConfig::default();
        // 4096 pages * 64KB = 256MB
        assert_eq!(config.memory_bytes(), 256 * 1024 * 1024);
    }

    #[test]
    fn test_daemon_config_includes_subagent() {
        let config = DaemonConfig::default();
        assert_eq!(config.subagent.max_concurrent, 5);
        assert_eq!(config.subagent.timeout_secs, 300);
    }

    #[test]
    fn test_subagent_config_serialization() {
        let config = SubagentConfig::new()
            .with_max_concurrent(3)
            .with_fuel_limit(500_000);

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"max_concurrent\":3"));
        assert!(json.contains("\"fuel_limit\":500000"));

        let decoded: SubagentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_concurrent, 3);
        assert_eq!(decoded.fuel_limit, Some(500_000));
    }

    #[test]
    fn test_subagent_config_toml_parsing() {
        // Parse just the subagent config section
        let toml_str = r#"
max_concurrent = 8
timeout_secs = 120
memory_pages = 2048
fuel_limit = 10000000
"#;
        let config: SubagentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.max_concurrent, 8);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.memory_pages, 2048);
        assert_eq!(config.fuel_limit, Some(10_000_000));
    }

    #[test]
    fn test_configured_providers() {
        let mut config = LlmConfig::default();
        config.provider = Some("anthropic".to_string());
        config.anthropic.api_key = Some("sk-test".to_string());
        config.anthropic.model = Some("claude-sonnet-4-20250514".to_string());
        config.openai.api_key = Some("sk-openai".to_string());

        let providers = config.configured_providers();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].0, "anthropic");
        assert!(providers[0].1.contains("(active)"));
        assert!(providers[0].1.contains("claude-sonnet-4-20250514"));
        assert_eq!(providers[1].0, "openai");
        assert!(!providers[1].1.contains("(active)"));
    }

    #[test]
    fn test_configured_providers_empty() {
        let config = LlmConfig::default();
        let providers = config.configured_providers();
        assert!(providers.is_empty());
    }

    #[test]
    fn test_configured_providers_default_model() {
        let mut config = LlmConfig::default();
        config.provider = Some("openai".to_string());
        config.openai.api_key = Some("sk-openai".to_string());
        // No model specified, should use default

        let providers = config.configured_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].0, "openai");
        assert!(providers[0].1.contains("gpt-4o-mini"));
        assert!(providers[0].1.contains("(active)"));
    }

    #[test]
    fn test_subagent_config_partial_toml() {
        // Only specify some fields, others should use defaults
        let toml_str = r#"
max_concurrent = 2
"#;
        let config: SubagentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.max_concurrent, 2);
        assert_eq!(config.timeout_secs, 300); // default
        assert_eq!(config.memory_pages, 4096); // default
        assert!(config.fuel_limit.is_none()); // default
    }

    // ==================== AuthConfig Tests ====================

    #[test]
    fn test_auth_config_defaults() {
        let config = AuthConfig::default();

        assert!(config.workspace_auto_allow);
        assert_eq!(
            config.allowed_commands,
            vec!["cargo *", "git *", "npm *", "just *"]
        );
        assert_eq!(
            config.sensitive_patterns,
            vec![".env*", "*credential*", "*secret*", "*_key*", "*.pem"]
        );
        assert!(config.denied_commands.is_empty());
    }

    #[test]
    fn test_auth_config_toml_parsing() {
        let toml_str = r#"
workspace_auto_allow = false
allowed_commands = ["cargo *", "git *", "make *"]
sensitive_patterns = [".env*", "*.key"]
denied_commands = ["rm -rf *", "sudo *"]
"#;
        let config: AuthConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.workspace_auto_allow);
        assert_eq!(config.allowed_commands, vec!["cargo *", "git *", "make *"]);
        assert_eq!(config.sensitive_patterns, vec![".env*", "*.key"]);
        assert_eq!(config.denied_commands, vec!["rm -rf *", "sudo *"]);
    }

    #[test]
    fn test_auth_config_partial_toml() {
        let toml_str = r#"
denied_commands = ["sudo *"]
"#;
        let config: AuthConfig = toml::from_str(toml_str).unwrap();
        // Defaults should be used for unspecified fields
        assert!(config.workspace_auto_allow);
        assert_eq!(
            config.allowed_commands,
            vec!["cargo *", "git *", "npm *", "just *"]
        );
        assert_eq!(
            config.sensitive_patterns,
            vec![".env*", "*credential*", "*secret*", "*_key*", "*.pem"]
        );
        assert_eq!(config.denied_commands, vec!["sudo *"]);
    }

    #[test]
    fn test_agent_config_includes_auth() {
        let config = AgentConfig::default();

        assert!(config.auth.workspace_auto_allow);
        assert_eq!(config.auth.allowed_commands.len(), 4);
        assert_eq!(config.auth.sensitive_patterns.len(), 5);
        assert!(config.auth.denied_commands.is_empty());
    }

    // ==================== LearningConfig Tests ====================

    #[test]
    fn learning_config_defaults() {
        let config = LearningConfig::default();
        assert!(config.enabled);
        assert_eq!(config.flush_threshold, 20);
        assert_eq!(config.flush_interval_secs, 30);
        assert_eq!(config.validation.min_alive_hours, 12);
        assert_eq!(config.validation.min_occurrences, 2);
        assert_eq!(config.validation.min_confidence, 0.6);
        assert_eq!(config.promotion.site_interaction_min_hits, 3);
        assert_eq!(config.promotion.min_alive_days, 3);
    }

    #[test]
    fn test_auth_config_merge() {
        let mut base = AgentConfig::default();
        let mut other = AgentConfig::default();

        // Modify auth in other
        other.auth.workspace_auto_allow = false;
        other.auth.allowed_commands = vec!["cargo *".to_string(), "make *".to_string()];
        other.auth.sensitive_patterns = vec![".env*".to_string()];
        other.auth.denied_commands = vec!["sudo *".to_string()];

        base.merge(&other);

        assert!(!base.auth.workspace_auto_allow);
        assert_eq!(base.auth.allowed_commands, vec!["cargo *", "make *"]);
        assert_eq!(base.auth.sensitive_patterns, vec![".env*"]);
        assert_eq!(base.auth.denied_commands, vec!["sudo *"]);
    }
}
