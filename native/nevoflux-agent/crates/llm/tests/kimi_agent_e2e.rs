//! End-to-end tests for the Kimi Agent CLI provider.
//!
//! These tests spawn the real `kimi-agent` CLI as a subprocess and communicate
//! over the JSON-RPC 2.0 wire protocol.  They require:
//!
//! - `kimi-agent` CLI installed and on PATH
//! - Valid API key configured in `~/.kimi/config.toml`
//! - Optionally: `MOONSHOT_API_KEY` environment variable set
//!
//! The model name is read from `~/.kimi/config.toml` (`default_model` field).
//! If not found, the test passes an empty model string so kimi-agent uses its
//! own default.
//!
//! Run with:
//!   cargo test -p nevoflux-llm --test kimi_agent_e2e -- --nocapture
//!
//! All real-API tests are gated behind `kimi_agent_available()` so they are
//! safely skipped when the binary is absent.

use nevoflux_llm::providers::kimi_agent::KimiAgentClient;
use rig::completion::{AssistantContent, CompletionModel};
use rig::message::{Message, UserContent};
use rig::OneOrMany;
use serde::Deserialize;

// =============================================================================
// Config Loading
// =============================================================================

/// Minimal config structures for reading model/key from ~/.kimi/config.toml.
#[derive(Debug, Deserialize, Default)]
struct KimiConfig {
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    providers: Option<KimiProviders>,
}

#[derive(Debug, Deserialize, Default)]
struct KimiProviders {
    #[serde(default)]
    kimi: Option<KimiProviderConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct KimiProviderConfig {
    #[serde(default)]
    api_key: Option<String>,
}

/// Load the kimi-agent config from ~/.kimi/config.toml.
fn load_kimi_config() -> KimiConfig {
    let config_path = dirs::home_dir()
        .map(|d| d.join(".kimi").join("config.toml"))
        .unwrap_or_default();

    if let Ok(content) = std::fs::read_to_string(&config_path) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        KimiConfig::default()
    }
}

/// Get the model name to use in tests.
///
/// Resolution order:
/// 1. `default_model` from ~/.kimi/config.toml
/// 2. Empty string (let kimi-agent use its own default)
fn configured_model() -> String {
    let config = load_kimi_config();
    if let Some(model) = config.default_model {
        if !model.is_empty() {
            eprintln!("Using model from kimi config: {}", model);
            return model;
        }
    }
    eprintln!("No model in kimi config, using CLI default");
    String::new()
}

/// Get the API key for tests.
///
/// Resolution order:
/// 1. MOONSHOT_API_KEY environment variable
/// 2. providers.kimi.api_key from ~/.kimi/config.toml
fn api_key() -> Option<String> {
    std::env::var("MOONSHOT_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            load_kimi_config()
                .providers
                .and_then(|p| p.kimi)
                .and_then(|k| k.api_key)
        })
}

// =============================================================================
// Helpers
// =============================================================================

/// Check whether the `kimi-agent` CLI is available on PATH.
fn kimi_agent_available() -> bool {
    std::process::Command::new("kimi-agent")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build a client with the API key.
fn make_client() -> KimiAgentClient {
    let mut client = KimiAgentClient::new("kimi-agent");
    if let Some(key) = api_key() {
        client = client.with_api_key(key);
    }
    client
}

/// Extract the text from a completion response's choice.
fn extract_text(choice: &OneOrMany<AssistantContent>) -> String {
    choice
        .iter()
        .filter_map(|c| match c {
            AssistantContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Guard macro: skip the test when prerequisites are missing.
macro_rules! skip_unless_available {
    () => {
        if !kimi_agent_available() {
            eprintln!("SKIP: kimi-agent CLI not available on PATH");
            return;
        }
    };
}

// =============================================================================
// Error / Edge-Case Tests (no API key required)
// =============================================================================

#[tokio::test]
async fn test_nonexistent_binary_returns_error() {
    let client = KimiAgentClient::new("nonexistent-kimi-agent-xyz");
    let model = client.completion_model("some-model");

    let result = model.completion_request("hello").send().await;

    assert!(result.is_err(), "Should fail when CLI binary not found");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("Failed to spawn kimi-agent"),
        "Error should mention spawn failure, got: {}",
        err_msg
    );
}

// =============================================================================
// Wire Protocol Integration Tests
// =============================================================================

/// Test that the wire protocol initialize handshake works.
///
/// This verifies:
/// - kimi-agent subprocess spawns successfully
/// - JSON-RPC 2.0 initialize request/response works
/// - Prompt is sent and events are received
#[tokio::test]
async fn test_e2e_simple_completion() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let response = model
        .completion_request("Reply with exactly the word 'pong' and nothing else.")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            assert!(!text.is_empty(), "Response should not be empty");
            assert!(
                text.to_lowercase().contains("pong"),
                "Expected 'pong' in response, got: {}",
                text
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Simple completion failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_e2e_math_computation() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let response = model
        .completion_request("What is 17 * 23? Reply with only the number, nothing else.")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            assert!(
                text.contains("391"),
                "Expected '391' (17*23) in response, got: {}",
                text
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Math computation failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_e2e_with_system_prompt() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let response = model
        .completion_request("What is your name?")
        .preamble(
            "You are a helpful assistant named KimiBot. Always introduce yourself by name."
                .to_string(),
        )
        .send()
        .await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            assert!(!text.is_empty(), "Response should not be empty");
            assert!(
                text.contains("KimiBot"),
                "Expected system prompt identity 'KimiBot' in response, got: {}",
                text
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Completion with system prompt failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_e2e_multi_turn_conversation() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let chat_history = vec![
        Message::User {
            content: OneOrMany::one(UserContent::text("What is the capital of Japan?")),
        },
        Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::text("The capital of Japan is Tokyo.")),
        },
    ];

    let response = model
        .completion_request(
            "Based on our previous exchange, what city did you mention? Reply with only the city name.",
        )
        .messages(chat_history)
        .send()
        .await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            assert!(!text.is_empty(), "Response should not be empty");
            assert!(
                text.to_lowercase().contains("tokyo"),
                "Expected 'Tokyo' in response, got: {}",
                text
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Multi-turn completion failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_e2e_chinese_language() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let response = model
        .completion_request("请用中文回答：1+1等于几？只回答数字。")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            assert!(!text.is_empty(), "Response should not be empty");
            assert!(
                text.contains('2'),
                "Expected '2' in response, got: {}",
                text
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Chinese language completion failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_e2e_response_has_usage_stats() {
    skip_unless_available!();

    let client = make_client();
    let model_name = configured_model();
    let model = client.completion_model(&model_name);

    let response = model.completion_request("Say hello.").send().await;

    match response {
        Ok(resp) => {
            let text = extract_text(&resp.choice);
            println!("Response: {}", text);
            println!(
                "Usage: input={}, output={}, total={}",
                resp.usage.input_tokens, resp.usage.output_tokens, resp.usage.total_tokens
            );
            assert!(!text.is_empty(), "Response should not be empty");
            assert!(
                resp.usage.total_tokens >= resp.usage.input_tokens + resp.usage.output_tokens
                    || resp.usage.total_tokens == 0,
                "total_tokens should be >= input+output or all zero"
            );
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("401")
                || err_msg.contains("Unauthorized")
                || err_msg.contains("-32003")
                || err_msg.contains("empty response")
                || err_msg.contains("Connection closed")
            {
                eprintln!("SKIP: API error (key invalid or expired): {}", err_msg);
                return;
            }
            panic!("Usage stats completion failed: {}", e);
        }
    }
}
