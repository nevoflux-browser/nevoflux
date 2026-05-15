//! Memory chunk model for storing text chunks with embeddings.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique ID for memory chunks.
pub fn memory_chunk_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let random_part: u64 = (timestamp as u64).wrapping_mul(6364136223846793005);
    format!("mem-{:016x}-{:08x}", timestamp as u64, random_part as u32)
}

/// Get the current Unix timestamp.
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// A memory chunk representing a piece of text with optional embedding and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    /// Unique identifier for the memory chunk.
    pub id: String,
    /// The text content of the memory chunk.
    pub content: String,
    /// Optional vector embedding for semantic search.
    pub embedding: Option<Vec<f32>>,
    /// Additional metadata as JSON.
    pub metadata: serde_json::Value,
    /// Unix timestamp when the chunk was created.
    pub created_at: i64,
    /// Unix timestamp when the chunk was last updated.
    pub updated_at: i64,
    /// Optional session ID this chunk is associated with.
    pub session_id: Option<String>,
}

impl MemoryChunk {
    /// Create a new memory chunk with the given content.
    pub fn new(content: impl Into<String>) -> Self {
        let now = current_timestamp();
        Self {
            id: memory_chunk_id(),
            content: content.into(),
            embedding: None,
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
            session_id: None,
        }
    }

    /// Set a custom ID for the memory chunk.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set the embedding vector.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

impl Default for MemoryChunk {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_chunk_new() {
        let chunk = MemoryChunk::new("Hello, world!");

        assert!(!chunk.id.is_empty());
        assert!(chunk.id.starts_with("mem-"));
        assert_eq!(chunk.content, "Hello, world!");
        assert!(chunk.embedding.is_none());
        assert_eq!(chunk.metadata, serde_json::Value::Null);
        assert!(chunk.created_at > 0);
        assert_eq!(chunk.created_at, chunk.updated_at);
        assert!(chunk.session_id.is_none());
    }

    #[test]
    fn test_memory_chunk_with_id() {
        let chunk = MemoryChunk::new("content").with_id("custom-id");

        assert_eq!(chunk.id, "custom-id");
    }

    #[test]
    fn test_memory_chunk_with_embedding() {
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let chunk = MemoryChunk::new("content").with_embedding(embedding.clone());

        assert_eq!(chunk.embedding, Some(embedding));
    }

    #[test]
    fn test_memory_chunk_with_metadata() {
        let metadata = serde_json::json!({"key": "value", "number": 42});
        let chunk = MemoryChunk::new("content").with_metadata(metadata.clone());

        assert_eq!(chunk.metadata, metadata);
    }

    #[test]
    fn test_memory_chunk_with_session() {
        let chunk = MemoryChunk::new("content").with_session("session-123");

        assert_eq!(chunk.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_memory_chunk_builder_chain() {
        let embedding = vec![1.0, 2.0, 3.0];
        let metadata = serde_json::json!({"source": "test"});

        let chunk = MemoryChunk::new("test content")
            .with_id("test-id")
            .with_embedding(embedding.clone())
            .with_metadata(metadata.clone())
            .with_session("sess-123");

        assert_eq!(chunk.id, "test-id");
        assert_eq!(chunk.content, "test content");
        assert_eq!(chunk.embedding, Some(embedding));
        assert_eq!(chunk.metadata, metadata);
        assert_eq!(chunk.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_memory_chunk_id_uniqueness() {
        let id1 = memory_chunk_id();
        let id2 = memory_chunk_id();

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_memory_chunk_serialization() {
        let chunk = MemoryChunk::new("test content")
            .with_id("test-123")
            .with_metadata(serde_json::json!({"key": "value"}));

        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: MemoryChunk = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, chunk.id);
        assert_eq!(deserialized.content, chunk.content);
        assert_eq!(deserialized.metadata, chunk.metadata);
    }

    #[test]
    fn test_memory_chunk_default() {
        let chunk = MemoryChunk::default();

        assert!(!chunk.id.is_empty());
        assert_eq!(chunk.content, "");
    }
}
