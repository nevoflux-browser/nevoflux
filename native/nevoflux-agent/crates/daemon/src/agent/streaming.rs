//! Streaming support for agent responses.
//!
//! This module provides types and utilities for streaming agent responses
//! back to clients in real-time as they are generated.

use nevoflux_protocol::{StreamChunk, StreamEnd, StreamFormat, StreamMetadata};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Stream handle for sending chunks.
///
/// This handle is used by the agent runner to send streaming response chunks
/// back to the client. It manages the stream lifecycle including chunk
/// transmission and proper stream termination.
#[derive(Clone)]
pub struct StreamHandle {
    stream_id: String,
    session_id: String,
    tx: mpsc::Sender<StreamEvent>,
}

impl StreamHandle {
    /// Create a new stream handle.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID this stream belongs to
    /// * `tx` - The sender channel for stream events
    pub fn new(session_id: String, tx: mpsc::Sender<StreamEvent>) -> Self {
        Self {
            stream_id: Uuid::new_v4().to_string(),
            session_id,
            tx,
        }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Send a chunk of streaming content.
    ///
    /// # Arguments
    ///
    /// * `delta` - The incremental content to send
    /// * `format` - The format of the content (markdown, plain, html)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the chunk was sent successfully, or `StreamSendError`
    /// if the channel has been closed.
    pub async fn send_chunk(
        &self,
        delta: String,
        format: StreamFormat,
    ) -> Result<(), StreamSendError> {
        let chunk = StreamChunk {
            session_id: self.session_id.clone(),
            stream_id: self.stream_id.clone(),
            delta,
            format,
            event: None,
            thinking_event: None,
        };

        self.tx
            .send(StreamEvent::Chunk(chunk))
            .await
            .map_err(|_| StreamSendError::ChannelClosed)
    }

    /// End the stream with optional metadata.
    ///
    /// This method consumes the handle, ensuring the stream cannot be used
    /// after it has been ended.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Optional metadata about the stream (tokens, duration, etc.)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the end marker was sent successfully, or `StreamSendError`
    /// if the channel has been closed.
    pub async fn end(self, metadata: Option<StreamMetadata>) -> Result<(), StreamSendError> {
        let end = StreamEnd {
            session_id: self.session_id.clone(),
            stream_id: self.stream_id.clone(),
            metadata,
        };

        self.tx
            .send(StreamEvent::End(end))
            .await
            .map_err(|_| StreamSendError::ChannelClosed)
    }
}

/// Events that can be sent through a stream channel.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of streaming content.
    Chunk(StreamChunk),
    /// End of stream marker.
    End(StreamEnd),
}

impl StreamEvent {
    /// Check if this is an end event.
    pub fn is_end(&self) -> bool {
        matches!(self, StreamEvent::End(_))
    }

    /// Check if this is a chunk event.
    pub fn is_chunk(&self) -> bool {
        matches!(self, StreamEvent::Chunk(_))
    }

    /// Get the session ID from this event.
    pub fn session_id(&self) -> &str {
        match self {
            StreamEvent::Chunk(chunk) => &chunk.session_id,
            StreamEvent::End(end) => &end.session_id,
        }
    }

    /// Get the stream ID from this event.
    pub fn stream_id(&self) -> &str {
        match self {
            StreamEvent::Chunk(chunk) => &chunk.stream_id,
            StreamEvent::End(end) => &end.stream_id,
        }
    }
}

/// Error type for stream send operations.
#[derive(Debug, thiserror::Error)]
pub enum StreamSendError {
    /// The channel has been closed.
    #[error("Channel closed")]
    ChannelClosed,
}

/// Create a stream channel with the specified buffer size.
///
/// # Arguments
///
/// * `buffer_size` - The number of events that can be buffered
///
/// # Returns
///
/// A tuple of (sender, receiver) for the stream channel.
pub fn create_stream_channel(
    buffer_size: usize,
) -> (mpsc::Sender<StreamEvent>, mpsc::Receiver<StreamEvent>) {
    mpsc::channel(buffer_size)
}

/// Default buffer size for stream channels.
pub const DEFAULT_STREAM_BUFFER_SIZE: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_handle_creation() {
        let (tx, _rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);

        assert_eq!(handle.session_id(), "sess-001");
        assert!(!handle.stream_id().is_empty());
    }

    #[tokio::test]
    async fn test_stream_handle_unique_ids() {
        let (tx1, _rx1) = create_stream_channel(16);
        let (tx2, _rx2) = create_stream_channel(16);

        let handle1 = StreamHandle::new("sess-001".to_string(), tx1);
        let handle2 = StreamHandle::new("sess-001".to_string(), tx2);

        // Each handle should have a unique stream ID
        assert_ne!(handle1.stream_id(), handle2.stream_id());
    }

    #[tokio::test]
    async fn test_send_chunk() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);
        let stream_id = handle.stream_id().to_string();

        let result = handle
            .send_chunk("Hello".to_string(), StreamFormat::Markdown)
            .await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        assert!(event.is_chunk());

        if let StreamEvent::Chunk(chunk) = event {
            assert_eq!(chunk.session_id, "sess-001");
            assert_eq!(chunk.stream_id, stream_id);
            assert_eq!(chunk.delta, "Hello");
            assert_eq!(chunk.format, StreamFormat::Markdown);
        }
    }

    #[tokio::test]
    async fn test_send_multiple_chunks() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);

        handle
            .send_chunk("Hello ".to_string(), StreamFormat::Plain)
            .await
            .unwrap();
        handle
            .send_chunk("World".to_string(), StreamFormat::Plain)
            .await
            .unwrap();

        let event1 = rx.recv().await.unwrap();
        let event2 = rx.recv().await.unwrap();

        if let StreamEvent::Chunk(chunk1) = event1 {
            assert_eq!(chunk1.delta, "Hello ");
        }

        if let StreamEvent::Chunk(chunk2) = event2 {
            assert_eq!(chunk2.delta, "World");
        }
    }

    #[tokio::test]
    async fn test_end_stream() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);
        let stream_id = handle.stream_id().to_string();

        let metadata = StreamMetadata {
            total_tokens: Some(100),
            duration_ms: Some(1500),
            model: Some("test-model".to_string()),
        };

        let result = handle.end(Some(metadata.clone())).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        assert!(event.is_end());

        if let StreamEvent::End(end) = event {
            assert_eq!(end.session_id, "sess-001");
            assert_eq!(end.stream_id, stream_id);
            assert!(end.metadata.is_some());

            let meta = end.metadata.unwrap();
            assert_eq!(meta.total_tokens, Some(100));
            assert_eq!(meta.duration_ms, Some(1500));
            assert_eq!(meta.model, Some("test-model".to_string()));
        }
    }

    #[tokio::test]
    async fn test_end_stream_no_metadata() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);

        let result = handle.end(None).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        if let StreamEvent::End(end) = event {
            assert!(end.metadata.is_none());
        }
    }

    #[tokio::test]
    async fn test_send_chunk_closed_channel() {
        let (tx, rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);

        // Drop the receiver to close the channel
        drop(rx);

        let result = handle
            .send_chunk("Hello".to_string(), StreamFormat::Plain)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StreamSendError::ChannelClosed
        ));
    }

    #[tokio::test]
    async fn test_end_stream_closed_channel() {
        let (tx, rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);

        // Drop the receiver to close the channel
        drop(rx);

        let result = handle.end(None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StreamSendError::ChannelClosed
        ));
    }

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

        let end = StreamEnd {
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

    #[test]
    fn test_create_stream_channel() {
        let (tx, _rx) = create_stream_channel(32);
        assert!(!tx.is_closed());
    }

    #[test]
    fn test_default_buffer_size() {
        assert_eq!(DEFAULT_STREAM_BUFFER_SIZE, 32);
    }

    #[tokio::test]
    async fn test_full_streaming_flow() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);
        let stream_id = handle.stream_id().to_string();

        // Send multiple chunks
        handle
            .send_chunk("Hello, ".to_string(), StreamFormat::Markdown)
            .await
            .unwrap();
        handle
            .send_chunk("this is ".to_string(), StreamFormat::Markdown)
            .await
            .unwrap();
        handle
            .send_chunk("a streaming response.".to_string(), StreamFormat::Markdown)
            .await
            .unwrap();

        // End the stream with metadata
        let metadata = StreamMetadata {
            total_tokens: Some(10),
            duration_ms: Some(500),
            model: None,
        };
        handle.end(Some(metadata)).await.unwrap();

        // Collect all events
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

        // Verify chunks
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].delta, "Hello, ");
        assert_eq!(chunks[1].delta, "this is ");
        assert_eq!(chunks[2].delta, "a streaming response.");

        // Verify all chunks have the same stream ID
        for chunk in &chunks {
            assert_eq!(chunk.stream_id, stream_id);
        }

        // Verify end event
        assert!(end_event.is_some());
        let end = end_event.unwrap();
        assert_eq!(end.stream_id, stream_id);
        assert!(end.metadata.is_some());
    }

    #[tokio::test]
    async fn test_cloned_handle_shares_channel() {
        let (tx, mut rx) = create_stream_channel(16);
        let handle = StreamHandle::new("sess-001".to_string(), tx);
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

        let event1 = rx.recv().await.unwrap();
        let event2 = rx.recv().await.unwrap();

        if let (StreamEvent::Chunk(c1), StreamEvent::Chunk(c2)) = (event1, event2) {
            assert_eq!(c1.delta, "From original");
            assert_eq!(c2.delta, "From clone");
        }
    }
}
