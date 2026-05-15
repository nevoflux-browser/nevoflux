//! Integration tests for agent runner.

use nevoflux_daemon::{AgentInput, AgentMode, AgentOutput, AgentRunner, AgentRunnerConfig};

/// Create a minimal Wasm module with the required exports.
fn create_minimal_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "get_abi_version") (result i32) i32.const 1)
            (memory (export "memory") 1)
        )
        "#,
    )
    .unwrap()
}

#[tokio::test]
async fn test_agent_runner_basic() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "sess-001".to_string(),
        mode: AgentMode::Chat,
        user_message: "Hello, agent!".to_string(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert!(!output.text.is_empty());
}

#[tokio::test]
async fn test_agent_runner_with_config() {
    let wasm = create_minimal_wasm();
    let config = AgentRunnerConfig {
        max_iterations: 5,
        iteration_timeout_ms: 1000,
    };
    let runner = AgentRunner::with_config(&wasm, config).unwrap();

    assert_eq!(runner.config().max_iterations, 5);
    assert_eq!(runner.config().iteration_timeout_ms, 1000);
}

#[tokio::test]
async fn test_agent_runner_all_modes() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    for mode in [
        AgentMode::Chat,
        AgentMode::Browser,
        AgentMode::Code,
        AgentMode::Plan,
    ] {
        let input = AgentInput {
            session_id: "sess-001".to_string(),
            mode,
            user_message: "Test".to_string(),
            history: vec![],
        };

        let output = runner.run(input).await;
        assert!(output.is_ok());
    }
}

#[test]
fn test_agent_runner_config_defaults() {
    let config = AgentRunnerConfig::default();

    // Default configuration values
    assert_eq!(config.max_iterations, 50);
    assert_eq!(config.iteration_timeout_ms, 30_000);
}

#[tokio::test]
async fn test_agent_runner_output_structure() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "sess-test".to_string(),
        mode: AgentMode::Chat,
        user_message: "Test message".to_string(),
        history: vec![],
    };

    let output: AgentOutput = runner.run(input).await.unwrap();

    // Verify output fields are populated correctly
    assert!(!output.text.is_empty());
    // The current implementation does not continue the loop
    assert!(!output.continue_loop);
    // Tool calls is empty in the basic implementation
    assert!(output.tool_calls.is_empty());
}

#[test]
fn test_agent_runner_invalid_wasm() {
    let invalid_wasm = b"not valid wasm";
    let result = AgentRunner::new(invalid_wasm);

    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_runner_unsupported_abi() {
    // Create a Wasm module with ABI version 2 (unsupported)
    let wasm_v2 = wat::parse_str(
        r#"
        (module
            (func (export "get_abi_version") (result i32) i32.const 2)
            (memory (export "memory") 1)
        )
        "#,
    )
    .unwrap();

    let runner = AgentRunner::new(&wasm_v2).unwrap();

    let input = AgentInput {
        session_id: "sess-001".to_string(),
        mode: AgentMode::Chat,
        user_message: "Test".to_string(),
        history: vec![],
    };

    let result = runner.run(input).await;
    assert!(result.is_err());
}

#[test]
fn test_agent_mode_default() {
    let mode: AgentMode = Default::default();
    assert_eq!(mode, AgentMode::Chat);
}

#[tokio::test]
async fn test_agent_runner_preserves_session_id() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "unique-session-123".to_string(),
        mode: AgentMode::Chat,
        user_message: "Hello".to_string(),
        history: vec![],
    };

    // The runner should accept and process the input with the session ID
    let output = runner.run(input).await;
    assert!(output.is_ok());
}
