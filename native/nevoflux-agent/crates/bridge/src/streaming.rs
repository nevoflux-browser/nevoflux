//! Streaming response accumulator for bridges.
//!
//! This module provides utilities for managing multiple concurrent streaming responses
//! from the agent. The [`StreamAccumulator`] maintains state for active streams,
//! accumulates content from chunks, and provides lifecycle management.
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_bridge::streaming::{StreamAccumulator, StreamError};
//! use nevoflux_protocol::StreamChunk;
//! use tokio::sync::mpsc;
//!
//! let mut accumulator = StreamAccumulator::new();
//! let (tx, rx) = mpsc::channel(32);
//!
//! // Start a new stream
//! accumulator.start_stream("stream-1".into(), "session-1".into(), tx);
//!
//! // Process chunks
//! let chunk = StreamChunk {
//!     session_id: "session-1".into(),
//!     stream_id: "stream-1".into(),
//!     delta: "Hello ".into(),
//!     format: nevoflux_protocol::StreamFormat::Markdown,
//! };
//! accumulator.process_chunk(chunk).await?;
//!
//! // End stream and get accumulated content
//! let content = accumulator.end_stream("stream-1");
//! ```

use std::collections::HashMap;

use nevoflux_protocol::{AgentMessage, StreamChunk, StreamEnd};
use tokio::sync::mpsc;

/// Active stream state.
///
/// Tracks an in-progress streaming response, including the accumulated content
/// and a channel for forwarding chunks to consumers.
#[derive(Debug)]
pub struct ActiveStream {
    /// Unique stream identifier.
    pub stream_id: String,
    /// Session this stream belongs to.
    pub session_id: String,
    /// Accumulated content from all chunks.
    pub accumulated: String,
    /// Channel for sending chunks to consumers.
    pub chunk_tx: mpsc::Sender<StreamChunk>,
}

impl ActiveStream {
    /// Creates a new active stream.
    pub fn new(stream_id: String, session_id: String, chunk_tx: mpsc::Sender<StreamChunk>) -> Self {
        Self {
            stream_id,
            session_id,
            accumulated: String::new(),
            chunk_tx,
        }
    }

    /// Appends content from a chunk to the accumulated buffer.
    pub fn append(&mut self, delta: &str) {
        self.accumulated.push_str(delta);
    }

    /// Returns the current accumulated content length.
    pub fn accumulated_len(&self) -> usize {
        self.accumulated.len()
    }
}

/// Stream accumulator for managing multiple concurrent streams.
///
/// The accumulator maintains state for all active streams, allowing the bridge
/// to track and manage multiple streaming responses simultaneously.
#[derive(Default)]
pub struct StreamAccumulator {
    /// Active streams indexed by stream ID.
    streams: HashMap<String, ActiveStream>,
}

impl StreamAccumulator {
    /// Creates a new empty stream accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts tracking a new stream.
    ///
    /// # Arguments
    ///
    /// * `stream_id` - Unique identifier for the stream
    /// * `session_id` - Session this stream belongs to
    /// * `chunk_tx` - Channel for forwarding chunks to consumers
    pub fn start_stream(
        &mut self,
        stream_id: String,
        session_id: String,
        chunk_tx: mpsc::Sender<StreamChunk>,
    ) {
        let stream = ActiveStream::new(stream_id.clone(), session_id, chunk_tx);
        self.streams.insert(stream_id, stream);
    }

    /// Processes a stream chunk by accumulating content and forwarding it.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::UnknownStream`] if the stream ID is not registered.
    /// Returns [`StreamError::ChannelClosed`] if the consumer channel is closed.
    pub async fn process_chunk(&mut self, chunk: StreamChunk) -> Result<(), StreamError> {
        let stream = self
            .streams
            .get_mut(&chunk.stream_id)
            .ok_or_else(|| StreamError::UnknownStream(chunk.stream_id.clone()))?;

        // Accumulate the content
        stream.append(&chunk.delta);

        // Forward to consumer
        stream
            .chunk_tx
            .send(chunk)
            .await
            .map_err(|_| StreamError::ChannelClosed)?;

        Ok(())
    }

    /// Ends a stream and returns the accumulated content.
    ///
    /// Returns `None` if the stream ID is not registered.
    pub fn end_stream(&mut self, stream_id: &str) -> Option<String> {
        self.streams.remove(stream_id).map(|s| s.accumulated)
    }

    /// Checks if a stream is currently active.
    pub fn is_active(&self, stream_id: &str) -> bool {
        self.streams.contains_key(stream_id)
    }

    /// Returns the number of currently active streams.
    pub fn active_count(&self) -> usize {
        self.streams.len()
    }

    /// Returns an iterator over all active stream IDs.
    pub fn active_stream_ids(&self) -> impl Iterator<Item = &str> {
        self.streams.keys().map(|s| s.as_str())
    }

    /// Gets a reference to an active stream by ID.
    pub fn get_stream(&self, stream_id: &str) -> Option<&ActiveStream> {
        self.streams.get(stream_id)
    }

    /// Gets a mutable reference to an active stream by ID.
    pub fn get_stream_mut(&mut self, stream_id: &str) -> Option<&mut ActiveStream> {
        self.streams.get_mut(stream_id)
    }

    /// Removes all streams for a given session.
    ///
    /// Returns the accumulated content for each removed stream.
    pub fn end_session_streams(&mut self, session_id: &str) -> Vec<(String, String)> {
        let stream_ids: Vec<String> = self
            .streams
            .iter()
            .filter(|(_, s)| s.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();

        stream_ids
            .into_iter()
            .filter_map(|id| {
                self.streams
                    .remove(&id)
                    .map(|s| (s.stream_id, s.accumulated))
            })
            .collect()
    }
}

/// Stream error types.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// The specified stream ID is not registered.
    #[error("Unknown stream: {0}")]
    UnknownStream(String),

    /// The consumer channel has been closed.
    #[error("Channel closed")]
    ChannelClosed,
}

/// Stream message type for routing.
///
/// Used to categorize stream-related messages extracted from [`AgentMessage`].
#[derive(Debug, Clone)]
pub enum StreamMessageType {
    /// A streaming content chunk.
    Chunk(StreamChunk),
    /// End of stream marker.
    End(StreamEnd),
}

/// Extracts stream-related messages from an agent message.
///
/// Returns `Some(StreamMessageType)` if the message is a stream chunk or stream end,
/// otherwise returns `None`.
pub fn extract_stream_message(msg: &AgentMessage) -> Option<StreamMessageType> {
    match msg {
        AgentMessage::StreamChunk(chunk) => Some(StreamMessageType::Chunk(chunk.clone())),
        AgentMessage::StreamEnd(end) => Some(StreamMessageType::End(end.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_protocol::StreamFormat;

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

    fn make_end(stream_id: &str, session_id: &str) -> StreamEnd {
        StreamEnd {
            session_id: session_id.to_string(),
            stream_id: stream_id.to_string(),
            metadata: None,
        }
    }

    #[test]
    fn test_stream_accumulator_lifecycle() {
        let mut accumulator = StreamAccumulator::new();
        let (tx, _rx) = mpsc::channel(32);

        // Initially empty
        assert_eq!(accumulator.active_count(), 0);
        assert!(!accumulator.is_active("stream-1"));

        // Start a stream
        accumulator.start_stream("stream-1".into(), "session-1".into(), tx);
        assert_eq!(accumulator.active_count(), 1);
        assert!(accumulator.is_active("stream-1"));

        // End the stream
        let content = accumulator.end_stream("stream-1");
        assert_eq!(content, Some(String::new()));
        assert_eq!(accumulator.active_count(), 0);
        assert!(!accumulator.is_active("stream-1"));
    }

    #[tokio::test]
    async fn test_stream_accumulator_process_chunks() {
        let mut accumulator = StreamAccumulator::new();
        let (tx, mut rx) = mpsc::channel(32);

        accumulator.start_stream("stream-1".into(), "session-1".into(), tx);

        // Process multiple chunks
        let chunk1 = make_chunk("stream-1", "session-1", "Hello ");
        let chunk2 = make_chunk("stream-1", "session-1", "World!");

        accumulator.process_chunk(chunk1).await.unwrap();
        accumulator.process_chunk(chunk2).await.unwrap();

        // Verify chunks were forwarded
        let received1 = rx.recv().await.unwrap();
        assert_eq!(received1.delta, "Hello ");
        let received2 = rx.recv().await.unwrap();
        assert_eq!(received2.delta, "World!");

        // End stream and verify accumulated content
        let content = accumulator.end_stream("stream-1");
        assert_eq!(content, Some("Hello World!".to_string()));
    }

    #[tokio::test]
    async fn test_stream_accumulator_unknown_stream_error() {
        let mut accumulator = StreamAccumulator::new();

        let chunk = make_chunk("unknown-stream", "session-1", "Hello");
        let result = accumulator.process_chunk(chunk).await;

        assert!(matches!(
            result,
            Err(StreamError::UnknownStream(id)) if id == "unknown-stream"
        ));
    }

    #[tokio::test]
    async fn test_stream_accumulator_channel_closed_error() {
        let mut accumulator = StreamAccumulator::new();
        let (tx, rx) = mpsc::channel(1);

        accumulator.start_stream("stream-1".into(), "session-1".into(), tx);

        // Drop the receiver to close the channel
        drop(rx);

        let chunk = make_chunk("stream-1", "session-1", "Hello");
        let result = accumulator.process_chunk(chunk).await;

        assert!(matches!(result, Err(StreamError::ChannelClosed)));
    }

    #[test]
    fn test_stream_accumulator_active_count_tracking() {
        let mut accumulator = StreamAccumulator::new();
        let (tx1, _rx1) = mpsc::channel(1);
        let (tx2, _rx2) = mpsc::channel(1);
        let (tx3, _rx3) = mpsc::channel(1);

        assert_eq!(accumulator.active_count(), 0);

        accumulator.start_stream("stream-1".into(), "session-1".into(), tx1);
        assert_eq!(accumulator.active_count(), 1);

        accumulator.start_stream("stream-2".into(), "session-1".into(), tx2);
        assert_eq!(accumulator.active_count(), 2);

        accumulator.start_stream("stream-3".into(), "session-2".into(), tx3);
        assert_eq!(accumulator.active_count(), 3);

        accumulator.end_stream("stream-2");
        assert_eq!(accumulator.active_count(), 2);

        accumulator.end_stream("stream-1");
        assert_eq!(accumulator.active_count(), 1);

        accumulator.end_stream("stream-3");
        assert_eq!(accumulator.active_count(), 0);
    }

    #[test]
    fn test_stream_accumulator_end_session_streams() {
        let mut accumulator = StreamAccumulator::new();
        let (tx1, _rx1) = mpsc::channel(1);
        let (tx2, _rx2) = mpsc::channel(1);
        let (tx3, _rx3) = mpsc::channel(1);

        accumulator.start_stream("stream-1".into(), "session-1".into(), tx1);
        accumulator.start_stream("stream-2".into(), "session-1".into(), tx2);
        accumulator.start_stream("stream-3".into(), "session-2".into(), tx3);

        // End all streams for session-1
        let ended = accumulator.end_session_streams("session-1");
        assert_eq!(ended.len(), 2);
        assert_eq!(accumulator.active_count(), 1);
        assert!(accumulator.is_active("stream-3"));
    }

    #[test]
    fn test_active_stream_append() {
        let (tx, _rx) = mpsc::channel(1);
        let mut stream = ActiveStream::new("stream-1".into(), "session-1".into(), tx);

        assert_eq!(stream.accumulated_len(), 0);

        stream.append("Hello ");
        assert_eq!(stream.accumulated, "Hello ");
        assert_eq!(stream.accumulated_len(), 6);

        stream.append("World!");
        assert_eq!(stream.accumulated, "Hello World!");
        assert_eq!(stream.accumulated_len(), 12);
    }

    #[test]
    fn test_extract_stream_message_chunk() {
        let chunk = make_chunk("stream-1", "session-1", "Hello");
        let msg = AgentMessage::StreamChunk(chunk.clone());

        let extracted = extract_stream_message(&msg);
        assert!(matches!(extracted, Some(StreamMessageType::Chunk(c)) if c.delta == "Hello"));
    }

    #[test]
    fn test_extract_stream_message_end() {
        let end = make_end("stream-1", "session-1");
        let msg = AgentMessage::StreamEnd(end);

        let extracted = extract_stream_message(&msg);
        assert!(matches!(extracted, Some(StreamMessageType::End(_))));
    }

    #[test]
    fn test_extract_stream_message_non_stream() {
        let msg = AgentMessage::AgentState(nevoflux_protocol::AgentStateMessage {
            session_id: "session-1".into(),
            state: nevoflux_protocol::AgentState::Idle,
            step: None,
            tool: None,
            progress: None,
        });

        let extracted = extract_stream_message(&msg);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_stream_accumulator_get_stream() {
        let mut accumulator = StreamAccumulator::new();
        let (tx, _rx) = mpsc::channel(1);

        accumulator.start_stream("stream-1".into(), "session-1".into(), tx);

        let stream = accumulator.get_stream("stream-1");
        assert!(stream.is_some());
        assert_eq!(stream.unwrap().session_id, "session-1");

        let missing = accumulator.get_stream("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_stream_accumulator_active_stream_ids() {
        let mut accumulator = StreamAccumulator::new();
        let (tx1, _rx1) = mpsc::channel(1);
        let (tx2, _rx2) = mpsc::channel(1);

        accumulator.start_stream("stream-a".into(), "session-1".into(), tx1);
        accumulator.start_stream("stream-b".into(), "session-1".into(), tx2);

        let ids: Vec<&str> = accumulator.active_stream_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"stream-a"));
        assert!(ids.contains(&"stream-b"));
    }
}
