//! Integration tests for streaming functionality.
//!
//! These tests verify the streaming infrastructure across the bridge and daemon crates,
//! including StreamAccumulator from bridge and StreamHandle from daemon.

use nevoflux_bridge::{ActiveStream, StreamAccumulator, StreamError, StreamMessageType};
use nevoflux_daemon::{
    create_stream_channel, StreamEvent, StreamHandle, StreamSendError, DEFAULT_STREAM_BUFFER_SIZE,
};
use nevoflux_protocol::{StreamChunk, StreamFormat, StreamMetadata};
use tokio::sync::mpsc;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a StreamChunk with the given parameters.
fn make_chunk(stream_id: &str, session_id: &str, delta: &str) -> StreamChunk {
    StreamChunk {
        session_id: session_id.to_string(),
        stream_id: stream_id.to_string(),
        delta: delta.to_string(),
        format: StreamFormat::Markdown,
        event: None,
        thinking_event: None,
    }
}

// ============================================================================
// StreamAccumulator Integration Tests
// ============================================================================

#[tokio::test]
async fn test_stream_accumulator_full_lifecycle() {
    // Create accumulator and channel
    let mut accumulator = StreamAccumulator::new();
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(32);

    // Verify initially empty
    assert_eq!(accumulator.active_count(), 0);

    // Start a new stream
    let stream_id = "stream-001".to_string();
    let session_id = "session-001".to_string();
    accumulator.start_stream(stream_id.clone(), session_id.clone(), tx);

    // Verify stream is active
    assert!(accumulator.is_active(&stream_id));
    assert_eq!(accumulator.active_count(), 1);

    // Process multiple chunks
    let chunks = ["Hello, ", "this is ", "a streaming ", "response!"];
    for delta in &chunks {
        let chunk = make_chunk(&stream_id, &session_id, delta);
        accumulator.process_chunk(chunk).await.unwrap();
    }

    // Verify chunks were forwarded
    for expected_delta in &chunks {
        let received = rx.recv().await.unwrap();
        assert_eq!(received.delta, *expected_delta);
        assert_eq!(received.stream_id, stream_id);
        assert_eq!(received.session_id, session_id);
    }

    // End stream and verify accumulated content
    let accumulated = accumulator.end_stream(&stream_id).unwrap();
    assert_eq!(accumulated, "Hello, this is a streaming response!");

    // Verify stream is no longer active
    assert!(!accumulator.is_active(&stream_id));
    assert_eq!(accumulator.active_count(), 0);
}

#[tokio::test]
async fn test_stream_accumulator_multiple_streams() {
    let mut accumulator = StreamAccumulator::new();

    // Create multiple streams
    let streams = vec![
        ("stream-1", "session-1"),
        ("stream-2", "session-1"),
        ("stream-3", "session-2"),
    ];

    let mut receivers = Vec::new();

    for (stream_id, session_id) in &streams {
        let (tx, rx) = mpsc::channel::<StreamChunk>(16);
        accumulator.start_stream(stream_id.to_string(), session_id.to_string(), tx);
        receivers.push(rx);
    }

    assert_eq!(accumulator.active_count(), 3);

    // Send chunks to each stream
    for (i, (stream_id, session_id)) in streams.iter().enumerate() {
        let chunk = make_chunk(stream_id, session_id, &format!("Message {}", i));
        accumulator.process_chunk(chunk).await.unwrap();
    }

    // Verify each receiver got its chunk
    for (i, rx) in receivers.iter_mut().enumerate() {
        let received = rx.recv().await.unwrap();
        assert_eq!(received.delta, format!("Message {}", i));
    }

    // End one stream
    let content = accumulator.end_stream("stream-2").unwrap();
    assert_eq!(content, "Message 1");
    assert_eq!(accumulator.active_count(), 2);

    // End session streams
    let ended = accumulator.end_session_streams("session-1");
    assert_eq!(ended.len(), 1); // Only stream-1 left from session-1
    assert_eq!(accumulator.active_count(), 1);
    assert!(accumulator.is_active("stream-3"));
}

#[tokio::test]
async fn test_stream_accumulator_error_handling() {
    let mut accumulator = StreamAccumulator::new();

    // Error: unknown stream
    let chunk = make_chunk("unknown-stream", "session-1", "test");
    let result = accumulator.process_chunk(chunk).await;
    assert!(matches!(result, Err(StreamError::UnknownStream(_))));

    // Error: channel closed
    let (tx, rx) = mpsc::channel::<StreamChunk>(1);
    accumulator.start_stream("stream-1".into(), "session-1".into(), tx);
    drop(rx); // Close the channel

    let chunk = make_chunk("stream-1", "session-1", "test");
    let result = accumulator.process_chunk(chunk).await;
    assert!(matches!(result, Err(StreamError::ChannelClosed)));
}

#[test]
fn test_active_stream_properties() {
    let (tx, _rx) = mpsc::channel::<StreamChunk>(1);
    let mut stream = ActiveStream::new("stream-001".into(), "session-001".into(), tx);

    assert_eq!(stream.stream_id, "stream-001");
    assert_eq!(stream.session_id, "session-001");
    assert_eq!(stream.accumulated_len(), 0);

    stream.append("Hello ");
    assert_eq!(stream.accumulated, "Hello ");
    assert_eq!(stream.accumulated_len(), 6);

    stream.append("World!");
    assert_eq!(stream.accumulated, "Hello World!");
    assert_eq!(stream.accumulated_len(), 12);
}

// ============================================================================
// StreamHandle Integration Tests
// ============================================================================

#[tokio::test]
async fn test_stream_handle_full_lifecycle() {
    let (tx, mut rx) = create_stream_channel(DEFAULT_STREAM_BUFFER_SIZE);
    let handle = StreamHandle::new("session-001".to_string(), tx);

    let stream_id = handle.stream_id().to_string();
    assert_eq!(handle.session_id(), "session-001");
    assert!(!stream_id.is_empty());

    // Send multiple chunks
    handle
        .send_chunk("Hello, ".to_string(), StreamFormat::Markdown)
        .await
        .unwrap();
    handle
        .send_chunk("World!".to_string(), StreamFormat::Markdown)
        .await
        .unwrap();

    // End the stream with metadata
    let metadata = StreamMetadata {
        total_tokens: Some(50),
        duration_ms: Some(1000),
        model: Some("test-model".to_string()),
    };
    handle.end(Some(metadata)).await.unwrap();

    // Collect and verify events
    let mut chunks = Vec::new();
    let mut end_event = None;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Chunk(chunk) => chunks.push(chunk),
            StreamEvent::End(end) => {
                end_event = Some(end);
                break;
            }
        }
    }

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].delta, "Hello, ");
    assert_eq!(chunks[1].delta, "World!");

    let end = end_event.unwrap();
    assert_eq!(end.stream_id, stream_id);
    assert!(end.metadata.is_some());
    let meta = end.metadata.unwrap();
    assert_eq!(meta.total_tokens, Some(50));
    assert_eq!(meta.model, Some("test-model".to_string()));
}

#[tokio::test]
async fn test_stream_handle_unique_ids() {
    let (tx1, _rx1) = create_stream_channel(16);
    let (tx2, _rx2) = create_stream_channel(16);
    let (tx3, _rx3) = create_stream_channel(16);

    let handle1 = StreamHandle::new("session-001".to_string(), tx1);
    let handle2 = StreamHandle::new("session-001".to_string(), tx2);
    let handle3 = StreamHandle::new("session-002".to_string(), tx3);

    // Each handle should have a unique stream ID
    assert_ne!(handle1.stream_id(), handle2.stream_id());
    assert_ne!(handle2.stream_id(), handle3.stream_id());
    assert_ne!(handle1.stream_id(), handle3.stream_id());

    // But session IDs match as specified
    assert_eq!(handle1.session_id(), handle2.session_id());
    assert_ne!(handle2.session_id(), handle3.session_id());
}

#[tokio::test]
async fn test_stream_handle_channel_closed_error() {
    let (tx, rx) = create_stream_channel(16);
    let handle = StreamHandle::new("session-001".to_string(), tx);

    // Drop receiver to close channel
    drop(rx);

    // send_chunk should fail
    let result = handle
        .send_chunk("test".to_string(), StreamFormat::Plain)
        .await;
    assert!(matches!(result, Err(StreamSendError::ChannelClosed)));
}

#[tokio::test]
async fn test_stream_handle_cloned_shares_channel() {
    let (tx, mut rx) = create_stream_channel(16);
    let handle = StreamHandle::new("session-001".to_string(), tx);
    let handle_clone = handle.clone();

    // Both handles should have the same IDs
    assert_eq!(handle.stream_id(), handle_clone.stream_id());
    assert_eq!(handle.session_id(), handle_clone.session_id());

    // Send from both handles
    handle
        .send_chunk("From original".to_string(), StreamFormat::Plain)
        .await
        .unwrap();
    handle_clone
        .send_chunk("From clone".to_string(), StreamFormat::Plain)
        .await
        .unwrap();

    // Verify both chunks arrive on the same channel
    let event1 = rx.recv().await.unwrap();
    let event2 = rx.recv().await.unwrap();

    if let (StreamEvent::Chunk(c1), StreamEvent::Chunk(c2)) = (event1, event2) {
        assert_eq!(c1.delta, "From original");
        assert_eq!(c2.delta, "From clone");
    } else {
        panic!("Expected two chunk events");
    }
}

// ============================================================================
// StreamEvent Integration Tests
// ============================================================================

#[test]
fn test_stream_event_methods() {
    let chunk = StreamChunk {
        session_id: "sess-001".to_string(),
        stream_id: "stream-001".to_string(),
        delta: "Hello".to_string(),
        format: StreamFormat::Markdown,
        event: None,
        thinking_event: None,
    };
    let chunk_event = StreamEvent::Chunk(chunk);

    assert!(chunk_event.is_chunk());
    assert!(!chunk_event.is_end());
    assert_eq!(chunk_event.session_id(), "sess-001");
    assert_eq!(chunk_event.stream_id(), "stream-001");

    let end = nevoflux_protocol::StreamEnd {
        session_id: "sess-002".to_string(),
        stream_id: "stream-002".to_string(),
        metadata: None,
    };
    let end_event = StreamEvent::End(end);

    assert!(!end_event.is_chunk());
    assert!(end_event.is_end());
    assert_eq!(end_event.session_id(), "sess-002");
    assert_eq!(end_event.stream_id(), "stream-002");
}

// ============================================================================
// End-to-End Streaming Flow Tests
// ============================================================================

#[tokio::test]
async fn test_end_to_end_streaming_flow() {
    // This test simulates a full streaming flow:
    // 1. Daemon creates StreamHandle and sends chunks
    // 2. Bridge's StreamAccumulator receives and accumulates chunks
    // 3. Verify final accumulated content

    // Setup: Create channel from daemon to bridge
    let (daemon_tx, mut daemon_rx) = create_stream_channel(32);

    // Daemon side: Create stream handle and send chunks
    let stream_handle = StreamHandle::new("sess-001".to_string(), daemon_tx);
    let stream_id = stream_handle.stream_id().to_string();

    // Bridge side: Setup accumulator
    let mut accumulator = StreamAccumulator::new();
    let (bridge_tx, mut bridge_rx) = mpsc::channel::<StreamChunk>(32);
    accumulator.start_stream(stream_id.clone(), "sess-001".into(), bridge_tx);

    // Daemon: Send streaming response
    let response_parts = ["I can ", "help you ", "with that ", "task."];
    for part in &response_parts {
        stream_handle
            .send_chunk(part.to_string(), StreamFormat::Markdown)
            .await
            .unwrap();
    }

    // Send end marker
    let metadata = StreamMetadata {
        total_tokens: Some(10),
        duration_ms: Some(500),
        model: None,
    };
    stream_handle.end(Some(metadata)).await.unwrap();

    // Bridge: Process events from daemon
    while let Some(event) = daemon_rx.recv().await {
        match event {
            StreamEvent::Chunk(chunk) => {
                // Forward to accumulator
                accumulator.process_chunk(chunk).await.unwrap();
            }
            StreamEvent::End(_end) => {
                // Stream ended
                break;
            }
        }
    }

    // Verify bridge received all chunks
    let mut received_text = String::new();
    while let Ok(chunk) = bridge_rx.try_recv() {
        received_text.push_str(&chunk.delta);
    }
    assert_eq!(received_text, "I can help you with that task.");

    // Verify accumulator has correct content
    let accumulated = accumulator.end_stream(&stream_id).unwrap();
    assert_eq!(accumulated, "I can help you with that task.");
}

#[tokio::test]
async fn test_streaming_with_different_formats() {
    let (tx, mut rx) = create_stream_channel(16);
    let handle = StreamHandle::new("session-001".to_string(), tx);

    // Send chunks with different formats
    handle
        .send_chunk("# Header\n".to_string(), StreamFormat::Markdown)
        .await
        .unwrap();
    handle
        .send_chunk("Plain text.".to_string(), StreamFormat::Plain)
        .await
        .unwrap();
    handle
        .send_chunk("<b>Bold</b>".to_string(), StreamFormat::Html)
        .await
        .unwrap();
    handle.end(None).await.unwrap();

    // Verify formats
    let formats: Vec<StreamFormat> = {
        let mut formats = Vec::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Chunk(chunk) => formats.push(chunk.format),
                StreamEvent::End(_) => break,
            }
        }
        formats
    };

    assert_eq!(formats.len(), 3);
    assert_eq!(formats[0], StreamFormat::Markdown);
    assert_eq!(formats[1], StreamFormat::Plain);
    assert_eq!(formats[2], StreamFormat::Html);
}

#[test]
fn test_default_stream_buffer_size() {
    assert_eq!(DEFAULT_STREAM_BUFFER_SIZE, 32);
}

#[test]
fn test_stream_message_type_extraction() {
    use nevoflux_bridge::extract_stream_message;
    use nevoflux_protocol::AgentMessage;

    // Test chunk extraction
    let chunk = StreamChunk {
        session_id: "sess-001".to_string(),
        stream_id: "stream-001".to_string(),
        delta: "Hello".to_string(),
        format: StreamFormat::Markdown,
        event: None,
        thinking_event: None,
    };
    let msg = AgentMessage::StreamChunk(chunk);

    let extracted = extract_stream_message(&msg);
    assert!(matches!(extracted, Some(StreamMessageType::Chunk(_))));

    // Test end extraction
    let end = nevoflux_protocol::StreamEnd {
        session_id: "sess-001".to_string(),
        stream_id: "stream-001".to_string(),
        metadata: None,
    };
    let msg = AgentMessage::StreamEnd(end);

    let extracted = extract_stream_message(&msg);
    assert!(matches!(extracted, Some(StreamMessageType::End(_))));

    // Test non-stream message
    let msg = AgentMessage::AgentState(nevoflux_protocol::AgentStateMessage {
        session_id: "sess-001".into(),
        state: nevoflux_protocol::AgentState::Idle,
        step: None,
        tool: None,
        progress: None,
    });

    let extracted = extract_stream_message(&msg);
    assert!(extracted.is_none());
}
