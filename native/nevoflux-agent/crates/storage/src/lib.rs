//! NevoFlux Storage - SQLite-based persistence layer
//!
//! Provides repository pattern access to sessions, messages, permissions, and config.
//!
//! # Example
//!
//! ```rust,no_run
//! use nevoflux_storage::{
//!     Storage, CreateSessionParams, CreateMessageParams, MessageRole, Result,
//! };
//!
//! fn main() -> Result<()> {
//!     // Open storage (or use Storage::open_in_memory() for testing)
//!     let storage = Storage::open("./data.db")?;
//!
//!     // Create a new session
//!     let session = storage.sessions().create(
//!         CreateSessionParams::new()
//!             .with_title("My First Session")
//!     )?;
//!
//!     // Add messages to the session
//!     storage.messages().create(
//!         CreateMessageParams::new(&session.id, MessageRole::User, "Hello!")
//!     )?;
//!
//!     storage.messages().create(
//!         CreateMessageParams::new(&session.id, MessageRole::Assistant, "Hi there!")
//!     )?;
//!
//!     // Store configuration
//!     storage.config().set("app.theme", serde_json::json!("dark"))?;
//!
//!     Ok(())
//! }
//! ```

pub mod connection;
pub mod error;
mod migrations;
pub mod models;
pub mod repositories;
mod storage;
pub mod vector;

pub use connection::Database;
pub use error::{Result, StorageError};
pub use storage::Storage;

// Re-export model types for convenience
pub use models::{
    ArtifactRecord, CheckPermissionParams, CleanupPolicy, CleanupResult, ConfigEntry, ContentType,
    CreateArtifactParams, CreateKnowledgeParams, CreateLearningMetricParams, CreateMessageParams,
    CreatePermissionParams, CreateSessionParams, CreateSiteAdaptationParams, CreateToolStatParams,
    Knowledge, LearningMetric, ListMessagesParams, ListSessionsParams, MemoryChunk, Message,
    MessageRole, Permission, PermissionScope, Session, SessionMode, SiteAdaptation, ToolStat,
    UpdateSessionParams,
};

// Re-export repository types for convenience
pub use repositories::{
    ArtifactRepository, ConfigRepository, KnowledgeRepository, LearningMetricsRepository,
    MemoryRepository, MessageRepository, PermissionRepository, SessionRepository,
    SiteAdaptationRepository, ToolStatsRepository, TraceRepository,
};

pub use repositories::traces::{CreateTraceSpanParams, TraceSpanRecord};

// Re-export vector types for convenience
pub use vector::{cosine_similarity, euclidean_distance, SimpleVectorIndex, VectorSearchResult};
