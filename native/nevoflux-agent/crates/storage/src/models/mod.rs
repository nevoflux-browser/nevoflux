//! Data models for the storage layer.

pub(crate) mod artifact;
mod config;
pub(crate) mod knowledge;
pub(crate) mod learning_metrics;
pub mod loop_record;
mod memory;
mod message;
mod permission;
mod session;
pub(crate) mod site_adaptation;
pub(crate) mod tool_stat;

pub use artifact::{ArtifactRecord, CreateArtifactParams};
pub use config::ConfigEntry;
pub use knowledge::{CreateKnowledgeParams, Knowledge};
pub use learning_metrics::{CreateLearningMetricParams, LearningMetric};
pub use loop_record::{IterationStatus, LoopIteration, LoopRecord, LoopState};
pub use memory::MemoryChunk;
pub use message::{ContentType, CreateMessageParams, ListMessagesParams, Message, MessageRole};
pub use permission::{CheckPermissionParams, CreatePermissionParams, Permission, PermissionScope};
pub use session::{
    current_timestamp, uuid_v4, CleanupPolicy, CleanupResult, CreateSessionParams,
    ListSessionsParams, Session, SessionMode, UpdateSessionParams,
};
pub use site_adaptation::{CreateSiteAdaptationParams, SiteAdaptation};
pub use tool_stat::{CreateToolStatParams, ToolStat};
