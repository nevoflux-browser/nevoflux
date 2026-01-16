/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Stream Manager
//!
//! Manages streaming output for text and UI updates.
//! Uses the shared-protocol crate for ExtensionMessage format.

use anyhow::Result;
use shared_protocol::{ExtensionMessage, StreamChunkPayload, StreamFormat};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error};

/// Stream type
#[derive(Debug, Clone)]
pub enum StreamType {
    /// Text streaming
    Text,
    /// UI streaming
    Ui,
}

/// Active stream
#[derive(Debug)]
struct ActiveStream {
    session_id: String,
    #[allow(dead_code)]
    stream_type: StreamType,
    sender: mpsc::UnboundedSender<InternalStreamChunk>,
}

/// Internal stream chunk data (different from shared-protocol StreamChunk)
#[derive(Debug, Clone)]
pub enum InternalStreamChunk {
    /// Text delta
    Text(String),
    /// Stream finished
    Finish,
}

/// Stream manager for handling streaming output
#[derive(Clone)]
pub struct StreamManager {
    streams: Arc<RwLock<HashMap<String, ActiveStream>>>,
    message_sender: mpsc::UnboundedSender<String>,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new(message_sender: mpsc::UnboundedSender<String>) -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            message_sender,
        }
    }

    /// Get a clone of the message sender for tool executor
    pub fn get_message_sender(&self) -> mpsc::UnboundedSender<String> {
        self.message_sender.clone()
    }

    /// Create a new text stream
    pub async fn create_text_stream(&self, session_id: String, stream_id: String) -> Result<()> {
        let (tx, rx) = mpsc::unbounded_channel();

        let stream = ActiveStream {
            session_id: session_id.clone(),
            stream_type: StreamType::Text,
            sender: tx,
        };

        self.streams.write().await.insert(stream_id.clone(), stream);

        debug!(
            "Created text stream: {} for session: {}",
            stream_id, session_id
        );

        // Start stream processor
        self.process_text_stream(session_id, stream_id, rx);

        Ok(())
    }

    /// Send a chunk to a stream
    async fn send_chunk(&self, stream_id: &str, chunk: InternalStreamChunk) -> Result<()> {
        let streams = self.streams.read().await;
        let stream = streams
            .get(stream_id)
            .ok_or_else(|| anyhow::anyhow!("Stream not found: {}", stream_id))?;

        stream
            .sender
            .send(chunk)
            .map_err(|_| anyhow::anyhow!("Stream channel closed"))
    }

    /// Send text delta to stream
    pub async fn send_text(&self, stream_id: &str, delta: String) -> Result<()> {
        self.send_chunk(stream_id, InternalStreamChunk::Text(delta)).await
    }

    /// Finish stream
    pub async fn finish_stream(&self, stream_id: &str) -> Result<()> {
        self.send_chunk(stream_id, InternalStreamChunk::Finish).await?;
        debug!("Stream finished: {}", stream_id);
        Ok(())
    }

    /// Send pong response
    pub async fn send_pong(&self, timestamp: u64) -> Result<()> {
        let msg = ExtensionMessage::Pong { timestamp };
        let json = serde_json::to_string(&msg)?;

        // Debug log to file
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/nevoflux-agent-debug.log")
        {
            use std::io::Write;
            let _ = writeln!(f, "Sending pong JSON: {}", json);
        }

        self.message_sender
            .send(json)
            .map_err(|_| anyhow::anyhow!("Failed to send pong message"))?;

        // Debug log success
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/nevoflux-agent-debug.log")
        {
            use std::io::Write;
            let _ = writeln!(f, "Pong sent to message_sender channel");
        }

        Ok(())
    }

    /// Close and remove stream
    pub async fn close_stream(&self, stream_id: &str) -> Result<()> {
        let mut streams = self.streams.write().await;

        if streams.remove(stream_id).is_some() {
            debug!("Stream closed: {}", stream_id);
            Ok(())
        } else {
            anyhow::bail!("Stream not found: {}", stream_id)
        }
    }

    /// Process text stream and send messages using ExtensionMessage format
    fn process_text_stream(
        &self,
        session_id: String,
        stream_id: String,
        mut rx: mpsc::UnboundedReceiver<InternalStreamChunk>,
    ) {
        let message_sender = self.message_sender.clone();
        let streams = Arc::clone(&self.streams);

        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let msg = match chunk {
                    InternalStreamChunk::Text(delta) => {
                        // Send StreamChunk message
                        ExtensionMessage::StreamChunk(StreamChunkPayload {
                            session_id: session_id.clone(),
                            stream_id: stream_id.clone(),
                            delta,
                            format: StreamFormat::Markdown,
                        })
                    }
                    InternalStreamChunk::Finish => {
                        // Send StreamEnd message
                        ExtensionMessage::StreamEnd {
                            stream_id: stream_id.clone(),
                            session_id: session_id.clone(),
                        }
                    }
                };

                let is_finish = matches!(msg, ExtensionMessage::StreamEnd { .. });

                if let Ok(json) = serde_json::to_string(&msg) {
                    if message_sender.send(json).is_err() {
                        error!("Failed to send stream message");
                        break;
                    }
                }

                if is_finish {
                    streams.write().await.remove(&stream_id);
                    break;
                }
            }

            debug!("Stream processor stopped for: {}", stream_id);
        });
    }

    /// Get active stream count
    pub async fn stream_count(&self) -> usize {
        let streams = self.streams.read().await;
        streams.len()
    }

    /// List all active streams
    pub async fn list_streams(&self) -> Vec<String> {
        let streams = self.streams.read().await;
        streams.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_stream_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = StreamManager::new(tx);

        manager
            .create_text_stream("session-1".to_string(), "stream-1".to_string())
            .await
            .unwrap();

        assert_eq!(manager.stream_count().await, 1);
    }

    #[tokio::test]
    async fn test_send_text() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let manager = StreamManager::new(tx);

        manager
            .create_text_stream("session-1".to_string(), "stream-1".to_string())
            .await
            .unwrap();

        manager
            .send_text("stream-1", "Hello world".to_string())
            .await
            .unwrap();

        // Should receive ExtensionMessage::StreamChunk
        let message = rx.recv().await.unwrap();
        assert!(message.contains("stream_chunk"));
        assert!(message.contains("Hello world"));
    }

    #[tokio::test]
    async fn test_finish_stream() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let manager = StreamManager::new(tx);

        manager
            .create_text_stream("session-1".to_string(), "stream-1".to_string())
            .await
            .unwrap();

        manager.finish_stream("stream-1").await.unwrap();

        // Should receive ExtensionMessage::StreamEnd
        let message = rx.recv().await.unwrap();
        assert!(message.contains("stream_end"));

        // Wait a bit for cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Stream should be removed
        assert_eq!(manager.stream_count().await, 0);
    }
}
