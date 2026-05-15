//! Integration tests for agent execution loop.
//!
//! These tests verify the agent runner and its interaction with
//! the Wasm runtime, including iteration handling and output structures.

use nevoflux_daemon::{
    AgentContent, AgentInput, AgentMode, AgentOutput, AgentProcessInput, AgentProcessOutput,
    AgentRunner, AgentRunnerConfig, HistoryEntry,
};

/// Create a minimal Wasm module with the required exports for the agent runner.
fn create_minimal_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (func (export "get_abi_version") (result i32) i32.const 1)
            (memory (export "memory") 1)
        )"#,
    )
    .unwrap()
}

#[tokio::test]
async fn test_agent_loop_single_iteration() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "test".into(),
        mode: AgentMode::Chat,
        user_message: "Hello".into(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert_eq!(output.iterations, 1);
    assert!(!output.continue_loop);
}

#[tokio::test]
async fn test_agent_loop_with_history() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "test-with-history".into(),
        mode: AgentMode::Chat,
        user_message: "What did I say?".into(),
        history: vec![
            nevoflux_protocol::ChatMessage {
                session_id: "test-with-history".into(),
                message_id: "msg-001".into(),
                text: "Hello there!".to_string(),
                attachments: vec![],
                tab_id: None,
                tab_ids: vec![],
            },
            nevoflux_protocol::ChatMessage {
                session_id: "test-with-history".into(),
                message_id: "msg-002".into(),
                text: "Hi! How can I help?".to_string(),
                attachments: vec![],
                tab_id: None,
                tab_ids: vec![],
            },
        ],
    };

    let output = runner.run(input).await.unwrap();
    assert_eq!(output.iterations, 1);
    assert!(!output.text.is_empty());
}

#[test]
fn test_agent_process_input_serialization() {
    let input = AgentProcessInput {
        session_id: "sess-001".into(),
        iteration: 0,
        content: AgentContent::UserMessage {
            text: "Hello, agent!".into(),
        },
        history: vec![HistoryEntry {
            role: "user".into(),
            content: "Previous message".into(),
        }],
        trace_summary: None,
    };

    let json = serde_json::to_string(&input).unwrap();
    assert!(json.contains("sess-001"));
    assert!(json.contains("Hello, agent!"));
    assert!(json.contains("Previous message"));

    let parsed: AgentProcessInput = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "sess-001");
    assert_eq!(parsed.iteration, 0);
    assert_eq!(parsed.history.len(), 1);
}

#[test]
fn test_agent_process_output_serialization() {
    let output = AgentProcessOutput {
        text: "I'll help you with that.".into(),
        tool_calls: vec![],
        complete: true,
        plan_proposal: None,
        artifact: None,
    };

    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("I'll help you with that."));
    assert!(json.contains("true"));

    let parsed: AgentProcessOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.text, "I'll help you with that.");
    assert!(parsed.complete);
    assert!(parsed.tool_calls.is_empty());
}

#[test]
fn test_agent_content_user_message() {
    let content = AgentContent::UserMessage {
        text: "Test message".into(),
    };

    let json = serde_json::to_string(&content).unwrap();
    assert!(json.contains("user_message"));
    assert!(json.contains("Test message"));

    let parsed: AgentContent = serde_json::from_str(&json).unwrap();
    if let AgentContent::UserMessage { text } = parsed {
        assert_eq!(text, "Test message");
    } else {
        panic!("Expected UserMessage variant");
    }
}

#[test]
fn test_history_entry_serialization() {
    let entry = HistoryEntry {
        role: "assistant".into(),
        content: "How can I help you?".into(),
    };

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("assistant"));
    assert!(json.contains("How can I help you?"));

    let parsed: HistoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, "assistant");
    assert_eq!(parsed.content, "How can I help you?");
}

#[tokio::test]
async fn test_agent_runner_with_custom_config() {
    let wasm = create_minimal_wasm();
    let config = AgentRunnerConfig {
        max_iterations: 10,
        iteration_timeout_ms: 5000,
    };
    let runner = AgentRunner::with_config(&wasm, config).unwrap();

    assert_eq!(runner.config().max_iterations, 10);
    assert_eq!(runner.config().iteration_timeout_ms, 5000);

    let input = AgentInput {
        session_id: "test-config".into(),
        mode: AgentMode::Code,
        user_message: "Run tests".into(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert!(!output.text.is_empty());
}

#[test]
fn test_agent_runner_config_defaults() {
    let config = AgentRunnerConfig::default();

    assert_eq!(config.max_iterations, 50);
    assert_eq!(config.iteration_timeout_ms, 30_000);
}

#[tokio::test]
async fn test_agent_runner_all_modes() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    for mode in [
        AgentMode::Chat,
        AgentMode::Plan,
        AgentMode::Code,
        AgentMode::Browser,
    ] {
        let input = AgentInput {
            session_id: "test-mode".into(),
            mode,
            user_message: "Test".into(),
            history: vec![],
        };

        let result = runner.run(input).await;
        assert!(result.is_ok(), "Mode {:?} should work", mode);
    }
}

#[test]
fn test_agent_mode_serialization() {
    assert_eq!(
        serde_json::to_string(&AgentMode::Chat).unwrap(),
        r#""chat""#
    );
    assert_eq!(
        serde_json::to_string(&AgentMode::Plan).unwrap(),
        r#""plan""#
    );
    assert_eq!(
        serde_json::to_string(&AgentMode::Code).unwrap(),
        r#""code""#
    );
    assert_eq!(
        serde_json::to_string(&AgentMode::Browser).unwrap(),
        r#""browser""#
    );

    let mode: AgentMode = serde_json::from_str(r#""chat""#).unwrap();
    assert_eq!(mode, AgentMode::Chat);

    let mode: AgentMode = serde_json::from_str(r#""browser""#).unwrap();
    assert_eq!(mode, AgentMode::Browser);
}

#[test]
fn test_agent_mode_default() {
    let mode = AgentMode::default();
    assert_eq!(mode, AgentMode::Chat);
}

#[tokio::test]
async fn test_agent_output_structure() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "test-output".into(),
        mode: AgentMode::Chat,
        user_message: "Test output structure".into(),
        history: vec![],
    };

    let output: AgentOutput = runner.run(input).await.unwrap();

    // Verify all fields are accessible
    assert!(!output.text.is_empty());
    assert!(!output.continue_loop);
    assert!(output.tool_calls.is_empty());
    assert!(output.iterations > 0);
}

#[test]
fn test_agent_runner_invalid_wasm() {
    let invalid_wasm = b"not valid wasm";
    let result = AgentRunner::new(invalid_wasm);

    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_runner_wrong_abi_version() {
    // Create a Wasm module with unsupported ABI version
    let wasm_v99 = wat::parse_str(
        r#"(module
            (func (export "get_abi_version") (result i32) i32.const 99)
            (memory (export "memory") 1)
        )"#,
    )
    .unwrap();

    let runner = AgentRunner::new(&wasm_v99).unwrap();

    let input = AgentInput {
        session_id: "test-abi".into(),
        mode: AgentMode::Chat,
        user_message: "Test".into(),
        history: vec![],
    };

    let result = runner.run(input).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unsupported ABI version"));
}

#[test]
fn test_agent_process_input_with_tool_results() {
    use nevoflux_daemon::agent::abi::ToolResult;

    let input = AgentProcessInput {
        session_id: "sess-tools".into(),
        iteration: 1,
        content: AgentContent::ToolResults {
            results: vec![
                ToolResult {
                    call_id: "call-001".into(),
                    name: "read_file".into(),
                    content: Some("file contents".into()),
                    error: None,
                },
                ToolResult {
                    call_id: "call-002".into(),
                    name: "write_file".into(),
                    content: None,
                    error: Some("Permission denied".into()),
                },
            ],
        },
        history: vec![],
        trace_summary: None,
    };

    let json = serde_json::to_string(&input).unwrap();
    assert!(json.contains("tool_results"));
    assert!(json.contains("call-001"));
    assert!(json.contains("Permission denied"));

    let parsed: AgentProcessInput = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.iteration, 1);

    if let AgentContent::ToolResults { results } = parsed.content {
        assert_eq!(results.len(), 2);
        assert!(results[0].content.is_some());
        assert!(results[1].error.is_some());
    } else {
        panic!("Expected ToolResults variant");
    }
}

#[test]
fn test_agent_process_output_with_tool_calls() {
    use nevoflux_daemon::agent::abi::PendingToolCall;

    let output = AgentProcessOutput {
        text: "I need to read a file.".into(),
        tool_calls: vec![PendingToolCall {
            id: "tc-001".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        }],
        complete: false,
        plan_proposal: None,
        artifact: None,
    };

    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("tc-001"));
    assert!(json.contains("read_file"));
    assert!(json.contains("/tmp/test.txt"));

    let parsed: AgentProcessOutput = serde_json::from_str(&json).unwrap();
    assert!(!parsed.complete);
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].name, "read_file");
}
