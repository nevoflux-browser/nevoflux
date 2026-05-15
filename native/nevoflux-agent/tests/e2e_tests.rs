//! End-to-end integration tests for NevoFlux Agent.
//!
//! These tests verify the complete flow of operations across multiple crates,
//! including the daemon, bridge, and protocol layers.

use nevoflux_bridge::StreamAccumulator;
use nevoflux_daemon::{
    create_stream_channel, with_retry, AgentInput, AgentMode, AgentRunner, AgentRunnerConfig,
    RetryConfig, Retryable, StreamEvent, StreamHandle, DEFAULT_STREAM_BUFFER_SIZE,
};
use nevoflux_protocol::{StreamChunk, StreamFormat, StreamMetadata};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

// ============================================================================
// Agent Runner E2E Tests
// ============================================================================

/// Create a minimal WASM module with the required exports.
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

/// Test agent runner with minimal WASM.
#[tokio::test]
async fn test_agent_runner_minimal_wasm() {
    let wasm = create_minimal_wasm();

    let runner = AgentRunner::new(&wasm).unwrap();

    let input = AgentInput {
        session_id: "e2e-test".into(),
        mode: AgentMode::Chat,
        user_message: "Hello from E2E test".into(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert!(!output.text.is_empty());
}

/// Test agent runner with custom configuration.
#[tokio::test]
async fn test_agent_runner_e2e_with_config() {
    let wasm = create_minimal_wasm();
    let config = AgentRunnerConfig {
        max_iterations: 10,
        iteration_timeout_ms: 5000,
    };

    let runner = AgentRunner::with_config(&wasm, config).unwrap();

    let input = AgentInput {
        session_id: "e2e-config-test".into(),
        mode: AgentMode::Browser,
        user_message: "Navigate to a page".into(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert!(!output.text.is_empty());
    // Verify the config was applied
    assert_eq!(runner.config().max_iterations, 10);
}

/// Test agent runner across all modes.
#[tokio::test]
async fn test_agent_runner_e2e_all_modes() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    let modes = [
        (AgentMode::Chat, "Chat message"),
        (AgentMode::Browser, "Browser automation task"),
        (AgentMode::Code, "Code generation request"),
        (AgentMode::Plan, "Planning task"),
    ];

    for (mode, message) in modes {
        let input = AgentInput {
            session_id: format!("e2e-mode-{:?}", mode),
            mode,
            user_message: message.to_string(),
            history: vec![],
        };

        let result = runner.run(input).await;
        assert!(
            result.is_ok(),
            "Failed for mode {:?}: {:?}",
            mode,
            result.err()
        );
    }
}

// ============================================================================
// Streaming Bridge Integration Tests
// ============================================================================

/// Test streaming accumulator with bridge.
#[tokio::test]
async fn test_streaming_bridge_integration() {
    let mut accumulator = StreamAccumulator::new();
    let (tx, mut rx) = mpsc::channel(10);

    accumulator.start_stream("stream-e2e".into(), "session-e2e".into(), tx);

    // Send chunks
    for i in 0..3 {
        let chunk = StreamChunk {
            session_id: "session-e2e".into(),
            stream_id: "stream-e2e".into(),
            delta: format!("Part {} ", i),
            format: StreamFormat::Markdown,
            event: None,
            thinking_event: None,
        };
        accumulator.process_chunk(chunk).await.unwrap();
    }

    // Verify forwarding
    let mut received = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        received.push(chunk);
    }
    assert_eq!(received.len(), 3);

    // End and verify accumulation
    let text = accumulator.end_stream("stream-e2e").unwrap();
    assert_eq!(text, "Part 0 Part 1 Part 2 ");
}

/// Test full streaming flow from daemon to bridge.
#[tokio::test]
async fn test_e2e_streaming_daemon_to_bridge() {
    // Create channel from daemon to bridge
    let (daemon_tx, mut daemon_rx) = create_stream_channel(DEFAULT_STREAM_BUFFER_SIZE);

    // Daemon side: Create stream handle and send chunks
    let stream_handle = StreamHandle::new("sess-e2e-001".to_string(), daemon_tx);
    let stream_id = stream_handle.stream_id().to_string();

    // Bridge side: Setup accumulator
    let mut accumulator = StreamAccumulator::new();
    let (bridge_tx, mut bridge_rx) = mpsc::channel::<StreamChunk>(32);
    accumulator.start_stream(stream_id.clone(), "sess-e2e-001".into(), bridge_tx);

    // Daemon: Send streaming response parts
    let response_parts = ["This ", "is ", "an ", "E2E ", "test!"];
    for part in &response_parts {
        stream_handle
            .send_chunk(part.to_string(), StreamFormat::Markdown)
            .await
            .unwrap();
    }

    // Send end marker with metadata
    let metadata = StreamMetadata {
        total_tokens: Some(25),
        duration_ms: Some(250),
        model: Some("test-model-e2e".to_string()),
    };
    stream_handle.end(Some(metadata)).await.unwrap();

    // Bridge: Process events from daemon
    let mut chunk_count = 0;
    while let Some(event) = daemon_rx.recv().await {
        match event {
            StreamEvent::Chunk(chunk) => {
                accumulator.process_chunk(chunk).await.unwrap();
                chunk_count += 1;
            }
            StreamEvent::End(end) => {
                assert!(end.metadata.is_some());
                let meta = end.metadata.unwrap();
                assert_eq!(meta.total_tokens, Some(25));
                assert_eq!(meta.model, Some("test-model-e2e".to_string()));
                break;
            }
        }
    }

    assert_eq!(chunk_count, 5);

    // Verify bridge received all chunks
    let mut received_text = String::new();
    while let Ok(chunk) = bridge_rx.try_recv() {
        received_text.push_str(&chunk.delta);
    }
    assert_eq!(received_text, "This is an E2E test!");

    // Verify accumulator has correct content
    let accumulated = accumulator.end_stream(&stream_id).unwrap();
    assert_eq!(accumulated, "This is an E2E test!");
}

/// Test multiple concurrent streams.
#[tokio::test]
async fn test_e2e_concurrent_streams() {
    let mut accumulator = StreamAccumulator::new();

    // Create 3 concurrent streams
    let mut receivers = Vec::new();
    for i in 0..3 {
        let (tx, rx) = mpsc::channel::<StreamChunk>(16);
        let stream_id = format!("stream-{}", i);
        let session_id = format!("session-{}", i % 2); // Two sessions
        accumulator.start_stream(stream_id, session_id, tx);
        receivers.push(rx);
    }

    assert_eq!(accumulator.active_count(), 3);

    // Send chunks to each stream
    for i in 0..3 {
        for j in 0..2 {
            let chunk = StreamChunk {
                session_id: format!("session-{}", i % 2),
                stream_id: format!("stream-{}", i),
                delta: format!("S{}C{}", i, j),
                format: StreamFormat::Plain,
                event: None,
                thinking_event: None,
            };
            accumulator.process_chunk(chunk).await.unwrap();
        }
    }

    // Verify each stream received its chunks
    for (i, mut rx) in receivers.into_iter().enumerate() {
        let mut received = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            received.push(chunk.delta);
        }
        assert_eq!(received.len(), 2);
        assert_eq!(received[0], format!("S{}C0", i));
        assert_eq!(received[1], format!("S{}C1", i));
    }

    // End streams and verify accumulated content
    for i in 0..3 {
        let content = accumulator.end_stream(&format!("stream-{}", i)).unwrap();
        assert_eq!(content, format!("S{}C0S{}C1", i, i));
    }
}

// ============================================================================
// Retry Mechanism E2E Tests
// ============================================================================

/// Test error type for retry tests.
#[derive(Debug)]
struct TestError(bool);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TestError(retryable={})", self.0)
    }
}

impl Retryable for TestError {
    fn is_retryable(&self) -> bool {
        self.0
    }
}

/// Test retry mechanism with simulated failures.
#[tokio::test]
async fn test_retry_integration() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let result = with_retry(&config, || {
        let attempts = attempts_clone.clone();
        async move {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(TestError(true))
            } else {
                Ok("success")
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

/// Test retry exhaustion with persistent failures.
#[tokio::test]
async fn test_retry_exhaustion_e2e() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let result: Result<(), TestError> = with_retry(&config, || {
        let attempts = attempts_clone.clone();
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(TestError(true)) // Always fail
        }
    })
    .await;

    assert!(result.is_err());
    // Initial attempt + 3 retries = 4 total attempts
    assert_eq!(attempts.load(Ordering::SeqCst), 4);
}

/// Test non-retryable error stops immediately.
#[tokio::test]
async fn test_retry_non_retryable_e2e() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(1));

    let result: Result<(), TestError> = with_retry(&config, || {
        let attempts = attempts_clone.clone();
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(TestError(false)) // Not retryable
        }
    })
    .await;

    assert!(result.is_err());
    // Should only try once since error is not retryable
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

// ============================================================================
// Combined E2E Scenarios
// ============================================================================

/// Test a complex scenario combining agent runner with streaming.
#[tokio::test]
async fn test_e2e_agent_with_streaming() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();

    // Create streaming infrastructure
    let (tx, mut rx) = create_stream_channel(32);
    let stream_handle = StreamHandle::new("complex-session".into(), tx);
    let stream_id = stream_handle.stream_id().to_string();

    // Run agent
    let input = AgentInput {
        session_id: "complex-session".into(),
        mode: AgentMode::Chat,
        user_message: "Complex E2E test with streaming".into(),
        history: vec![],
    };

    let output = runner.run(input).await.unwrap();
    assert!(!output.text.is_empty());

    // Simulate streaming the response
    let words: Vec<&str> = output.text.split_whitespace().collect();
    for word in &words {
        stream_handle
            .send_chunk(format!("{} ", word), StreamFormat::Plain)
            .await
            .unwrap();
    }
    stream_handle.end(None).await.unwrap();

    // Collect streamed content
    let mut streamed_content = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Chunk(chunk) => {
                streamed_content.push_str(&chunk.delta);
            }
            StreamEvent::End(end) => {
                assert_eq!(end.stream_id, stream_id);
                break;
            }
        }
    }

    // Verify the streamed content matches
    let reconstructed: String = words.iter().map(|w| format!("{} ", w)).collect();
    assert_eq!(streamed_content, reconstructed);
}

/// Test session management across multiple operations.
#[tokio::test]
async fn test_e2e_session_lifecycle() {
    let wasm = create_minimal_wasm();
    let runner = AgentRunner::new(&wasm).unwrap();
    let session_id = "lifecycle-session-001";

    // First interaction
    let input1 = AgentInput {
        session_id: session_id.into(),
        mode: AgentMode::Chat,
        user_message: "First message".into(),
        history: vec![],
    };
    let output1 = runner.run(input1).await.unwrap();
    assert!(!output1.text.is_empty());

    // Second interaction (simulating history)
    let input2 = AgentInput {
        session_id: session_id.into(),
        mode: AgentMode::Chat,
        user_message: "Second message".into(),
        history: vec![], // In a real scenario, this would contain history
    };
    let output2 = runner.run(input2).await.unwrap();
    assert!(!output2.text.is_empty());

    // Third interaction with mode change
    let input3 = AgentInput {
        session_id: session_id.into(),
        mode: AgentMode::Plan,
        user_message: "Create a plan".into(),
        history: vec![],
    };
    let output3 = runner.run(input3).await.unwrap();
    assert!(!output3.text.is_empty());
}

/// Test error recovery across components.
#[tokio::test]
async fn test_e2e_error_recovery() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();
    let wasm = create_minimal_wasm();

    let config = RetryConfig::new()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    // Simulate an operation that fails initially but succeeds after retry
    let result = with_retry(&config, || {
        let attempts = attempts_clone.clone();
        let wasm = wasm.clone();
        async move {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            if n < 1 {
                Err(TestError(true)) // First attempt fails
            } else {
                // Second attempt: run the agent
                let runner = AgentRunner::new(&wasm).map_err(|_| TestError(false))?;
                let input = AgentInput {
                    session_id: "recovery-test".into(),
                    mode: AgentMode::Chat,
                    user_message: "Recovery test".into(),
                    history: vec![],
                };
                runner.run(input).await.map_err(|_| TestError(false))
            }
        }
    })
    .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(!output.text.is_empty());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}
