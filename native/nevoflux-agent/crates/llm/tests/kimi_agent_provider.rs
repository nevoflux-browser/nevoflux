//! Integration tests for the Kimi Agent CLI provider.
//!
//! These tests verify client construction, factory integration,
//! and trait implementations for the kimi-agent wire protocol provider.
//!
//! Tests marked with #[ignore] require the `kimi-agent` CLI installed and authenticated.
//!
//! Run with: `cargo test -p nevoflux-llm --test kimi_agent_provider -- --nocapture`

use std::str::FromStr;

use nevoflux_llm::providers::kimi_agent::{
    KimiAgentClient, KimiAgentCompletionModel, KimiAgentStreamingResponse,
};
use nevoflux_llm::{api_key_env_var, default_context_window_for, default_model_for, ProviderType};
use rig::completion::{CompletionModel, GetTokenUsage};

// =============================================================================
// Factory / ProviderType Integration Tests
// =============================================================================

#[test]
fn test_kimi_agent_provider_type_from_str() {
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
}

#[test]
fn test_kimi_agent_provider_type_case_insensitive() {
    assert_eq!(
        ProviderType::from_str("Kimi-Agent").unwrap(),
        ProviderType::KimiAgent
    );
    assert_eq!(
        ProviderType::from_str("KIMI").unwrap(),
        ProviderType::KimiAgent
    );
}

#[test]
fn test_kimi_agent_default_model() {
    let model = default_model_for(ProviderType::KimiAgent);
    assert_eq!(model, "kimi-latest");
}

#[test]
fn test_kimi_agent_default_context_window() {
    let window = default_context_window_for(ProviderType::KimiAgent);
    assert_eq!(window, 128_000);
}

#[test]
fn test_kimi_agent_api_key_env_var() {
    let env_var = api_key_env_var(ProviderType::KimiAgent);
    assert_eq!(env_var, "MOONSHOT_API_KEY");
}

#[test]
fn test_kimi_agent_provider_type_debug() {
    assert_eq!(format!("{:?}", ProviderType::KimiAgent), "KimiAgent");
}

#[test]
fn test_kimi_agent_provider_type_copy_clone() {
    let p = ProviderType::KimiAgent;
    let copied = p;
    let cloned = p;
    assert_eq!(p, copied);
    assert_eq!(p, cloned);
}

// =============================================================================
// Client Construction Tests
// =============================================================================

#[test]
fn test_client_default_command() {
    let client = KimiAgentClient::new("kimi-agent");
    assert_eq!(client.command(), "kimi-agent");
    let debug = format!("{:?}", client);
    assert!(debug.contains("None"), "Expected no api_key by default");
}

#[test]
fn test_client_custom_command_path() {
    let client = KimiAgentClient::new("/usr/local/bin/kimi-agent");
    assert_eq!(client.command(), "/usr/local/bin/kimi-agent");
}

#[test]
fn test_client_with_api_key_chain() {
    let client = KimiAgentClient::new("kimi-agent").with_api_key("moonshot-key-test123");
    let debug = format!("{:?}", client);
    assert!(debug.contains("REDACTED"), "API key should be redacted");
    assert!(
        !debug.contains("moonshot-key-test123"),
        "API key must not leak"
    );
    assert_eq!(client.command(), "kimi-agent");
}

#[test]
fn test_client_clone_preserves_state() {
    let client = KimiAgentClient::new("kimi-agent")
        .with_api_key("key-abc")
        .with_model("kimi-latest")
        .with_working_dir("/tmp/workspace")
        .with_thinking(true);
    let cloned = client.clone();
    assert_eq!(cloned.command(), "kimi-agent");
    let debug = format!("{:?}", cloned);
    assert!(debug.contains("REDACTED"));
    assert!(debug.contains("kimi-latest"));
    assert!(debug.contains("/tmp/workspace"));
}

#[test]
fn test_client_with_working_dir_builder() {
    let client = KimiAgentClient::new("kimi-agent").with_working_dir("/tmp/workspace");
    let debug = format!("{:?}", client);
    assert!(debug.contains("/tmp/workspace"));
}

#[test]
fn test_client_with_thinking_modes() {
    let thinking_on = KimiAgentClient::new("kimi-agent").with_thinking(true);
    let debug = format!("{:?}", thinking_on);
    assert!(debug.contains("thinking: Some(true)"));

    let thinking_off = KimiAgentClient::new("kimi-agent").with_thinking(false);
    let debug = format!("{:?}", thinking_off);
    assert!(debug.contains("thinking: Some(false)"));

    let thinking_default = KimiAgentClient::new("kimi-agent");
    let debug = format!("{:?}", thinking_default);
    assert!(debug.contains("thinking: None"));
}

#[test]
fn test_client_debug_never_leaks_api_key() {
    let client = KimiAgentClient::new("kimi-agent").with_api_key("super-secret-moonshot-key-12345");
    let debug = format!("{:?}", client);
    assert!(
        !debug.contains("super-secret-moonshot-key-12345"),
        "API key must not appear in Debug output"
    );
    assert!(debug.contains("REDACTED"));
    assert!(debug.contains("kimi-agent"));
}

// =============================================================================
// CompletionModel Construction Tests
// =============================================================================

#[test]
fn test_completion_model_from_client() {
    let client = KimiAgentClient::new("kimi-agent");
    let model = client.completion_model("kimi-latest");
    assert_eq!(model.model(), "kimi-latest");
}

#[test]
fn test_completion_model_custom_model_name() {
    let client = KimiAgentClient::new("kimi-agent");
    let model = client.completion_model("k2-0411-preview");
    assert_eq!(model.model(), "k2-0411-preview");
}

#[test]
fn test_completion_model_clone() {
    let client = KimiAgentClient::new("kimi-agent");
    let model = client.completion_model("kimi-latest");
    let cloned = model.clone();
    assert_eq!(cloned.model(), "kimi-latest");
}

#[test]
fn test_completion_model_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<KimiAgentCompletionModel>();
}

#[test]
fn test_completion_model_implements_trait() {
    fn assert_completion_model<T: CompletionModel>() {}
    assert_completion_model::<KimiAgentCompletionModel>();
}

// =============================================================================
// StreamingResponse Tests
// =============================================================================

#[test]
fn test_streaming_response_with_usage() {
    use nevoflux_llm::providers::kimi_agent::types::KimiUsage;

    let resp = KimiAgentStreamingResponse {
        usage: Some(KimiUsage {
            input_tokens: 200,
            output_tokens: 100,
        }),
    };
    let usage = resp.token_usage().unwrap();
    assert_eq!(usage.input_tokens, 200);
    assert_eq!(usage.output_tokens, 100);
    assert_eq!(usage.total_tokens, 300);
}

#[test]
fn test_streaming_response_without_usage() {
    let resp = KimiAgentStreamingResponse { usage: None };
    assert!(resp.token_usage().is_none());
}

// =============================================================================
// Full Builder Chain Tests
// =============================================================================

#[test]
fn test_full_builder_chain() {
    let client = KimiAgentClient::new("kimi-agent")
        .with_api_key("test-key")
        .with_model("kimi-latest")
        .with_working_dir("/workspace")
        .with_thinking(true);

    assert_eq!(client.command(), "kimi-agent");
    // api_key, model, working_dir, thinking are pub(crate); verify via Debug
    let debug = format!("{:?}", client);
    assert!(debug.contains("REDACTED")); // api_key is redacted
    assert!(debug.contains("kimi-latest"));
    assert!(debug.contains("/workspace"));
    assert!(debug.contains("thinking: Some(true)"));

    let model = client.completion_model("k2-0411-preview");
    assert_eq!(model.model(), "k2-0411-preview");
}
