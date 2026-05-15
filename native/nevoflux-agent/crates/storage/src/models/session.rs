//! Session model and related types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Error returned when parsing a session mode from string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSessionModeError;

impl std::fmt::Display for ParseSessionModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid session mode")
    }
}

impl std::error::Error for ParseSessionModeError {}

/// The mode of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    /// Standard chat mode.
    #[default]
    Chat,
    /// Agent mode with tool execution capabilities.
    Agent,
}

impl SessionMode {
    /// Convert the mode to a string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Chat => "chat",
            SessionMode::Agent => "agent",
        }
    }
}

impl FromStr for SessionMode {
    type Err = ParseSessionModeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "chat" => Ok(SessionMode::Chat),
            "agent" => Ok(SessionMode::Agent),
            _ => Err(ParseSessionModeError),
        }
    }
}

impl std::fmt::Display for SessionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A session representing a conversation or agent interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for the session.
    pub id: String,
    /// Optional title for the session.
    pub title: Option<String>,
    /// The mode of the session.
    pub mode: SessionMode,
    /// Unix timestamp when the session was created.
    pub created_at: i64,
    /// Unix timestamp when the session was last updated.
    pub updated_at: i64,
    /// Whether the session is pinned.
    pub pinned: bool,
    /// Whether the session is archived.
    pub archived: bool,
    /// Additional metadata as key-value pairs.
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Session {
    /// Create a new session with default values.
    pub fn new() -> Self {
        let now = current_timestamp();
        Self {
            id: uuid_v4(),
            title: None,
            mode: SessionMode::default(),
            created_at: now,
            updated_at: now,
            pinned: false,
            archived: false,
            metadata: None,
        }
    }

    /// Set the session mode.
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the session title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the session ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters for creating a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateSessionParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// Optional title for the session.
    pub title: Option<String>,
    /// The mode of the session (defaults to Chat).
    pub mode: Option<SessionMode>,
    /// Whether the session is pinned (defaults to false).
    pub pinned: Option<bool>,
    /// Additional metadata.
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl CreateSessionParams {
    /// Create new params with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the session ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the session title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the session mode.
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set whether the session is pinned.
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = Some(pinned);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Parameters for updating an existing session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSessionParams {
    /// New title (None = don't change, Some(None) = clear).
    pub title: Option<Option<String>>,
    /// New mode.
    pub mode: Option<SessionMode>,
    /// New pinned state.
    pub pinned: Option<bool>,
    /// New archived state.
    pub archived: Option<bool>,
    /// New metadata (replaces existing).
    pub metadata: Option<Option<HashMap<String, serde_json::Value>>>,
}

impl UpdateSessionParams {
    /// Create new update params.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a new title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(Some(title.into()));
        self
    }

    /// Clear the title.
    pub fn clear_title(mut self) -> Self {
        self.title = Some(None);
        self
    }

    /// Set the mode.
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set the pinned state.
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = Some(pinned);
        self
    }

    /// Set the archived state.
    pub fn with_archived(mut self, archived: bool) -> Self {
        self.archived = Some(archived);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(Some(metadata));
        self
    }

    /// Clear the metadata.
    pub fn clear_metadata(mut self) -> Self {
        self.metadata = Some(None);
        self
    }

    /// Check if any fields are set to be updated.
    pub fn has_changes(&self) -> bool {
        self.title.is_some()
            || self.mode.is_some()
            || self.pinned.is_some()
            || self.archived.is_some()
            || self.metadata.is_some()
    }
}

/// Session cleanup policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPolicy {
    /// Delete sessions inactive for more than this many days.
    /// `None` means no time-based cleanup.
    pub inactive_days: Option<u32>,
    /// Maximum number of sessions to keep.
    /// `None` means no limit on session count.
    pub max_sessions: Option<u32>,
    /// Maximum total storage size in MB.
    /// `None` means no limit on storage size.
    pub max_storage_mb: Option<u32>,
    /// Whether to skip pinned sessions during cleanup.
    pub preserve_pinned: bool,
    /// Whether to skip archived sessions during cleanup.
    pub preserve_archived: bool,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            inactive_days: None,
            max_sessions: None,
            max_storage_mb: None,
            preserve_pinned: true,
            preserve_archived: false,
        }
    }
}

impl CleanupPolicy {
    /// Create a new cleanup policy with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the inactive days threshold.
    pub fn with_inactive_days(mut self, days: u32) -> Self {
        self.inactive_days = Some(days);
        self
    }

    /// Set the maximum number of sessions.
    pub fn with_max_sessions(mut self, max: u32) -> Self {
        self.max_sessions = Some(max);
        self
    }

    /// Set the maximum storage size in MB.
    pub fn with_max_storage_mb(mut self, mb: u32) -> Self {
        self.max_storage_mb = Some(mb);
        self
    }

    /// Set whether to preserve pinned sessions.
    pub fn preserve_pinned(mut self, preserve: bool) -> Self {
        self.preserve_pinned = preserve;
        self
    }

    /// Set whether to preserve archived sessions.
    pub fn preserve_archived(mut self, preserve: bool) -> Self {
        self.preserve_archived = preserve;
        self
    }

    /// Check if any cleanup rules are configured.
    pub fn has_rules(&self) -> bool {
        self.inactive_days.is_some() || self.max_sessions.is_some() || self.max_storage_mb.is_some()
    }
}

/// Result of a cleanup operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupResult {
    /// Number of sessions deleted due to inactivity.
    pub inactive_deleted: u32,
    /// Number of sessions deleted due to session count limit.
    pub count_deleted: u32,
    /// Number of sessions deleted due to storage limit.
    pub storage_deleted: u32,
    /// Total bytes freed.
    pub bytes_freed: u64,
}

impl CleanupResult {
    /// Get the total number of sessions deleted.
    pub fn total_deleted(&self) -> u32 {
        self.inactive_deleted + self.count_deleted + self.storage_deleted
    }

    /// Check if any sessions were deleted.
    pub fn has_deletions(&self) -> bool {
        self.total_deleted() > 0
    }
}

/// Parameters for listing sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSessionsParams {
    /// Include archived sessions (default: false).
    pub include_archived: Option<bool>,
    /// Filter by mode.
    pub mode: Option<SessionMode>,
    /// Filter by pinned state.
    pub pinned: Option<bool>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Number of results to skip.
    pub offset: Option<u32>,
    /// Search query for title.
    pub search: Option<String>,
    /// Exclude sessions with no messages (default: false).
    pub exclude_empty: Option<bool>,
}

impl ListSessionsParams {
    /// Create new list params.
    pub fn new() -> Self {
        Self::default()
    }

    /// Include archived sessions.
    pub fn include_archived(mut self, include: bool) -> Self {
        self.include_archived = Some(include);
        self
    }

    /// Filter by mode.
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Filter by pinned state.
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = Some(pinned);
        self
    }

    /// Set the limit.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the offset.
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set search query.
    pub fn with_search(mut self, search: impl Into<String>) -> Self {
        self.search = Some(search.into());
        self
    }

    /// Exclude sessions that have no messages.
    pub fn exclude_empty(mut self, exclude: bool) -> Self {
        self.exclude_empty = Some(exclude);
        self
    }
}

/// Generate a simple UUID v4-like identifier.
pub fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Use timestamp and a simple counter for uniqueness
    // Format: sess-{timestamp_hex}-{random_hex}
    let random_part: u64 = (timestamp as u64).wrapping_mul(6364136223846793005);
    format!("sess-{:016x}-{:08x}", timestamp as u64, random_part as u32)
}

/// Get the current Unix timestamp.
pub fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_mode_as_str() {
        assert_eq!(SessionMode::Chat.as_str(), "chat");
        assert_eq!(SessionMode::Agent.as_str(), "agent");
    }

    #[test]
    fn test_session_mode_from_str() {
        assert_eq!("chat".parse::<SessionMode>(), Ok(SessionMode::Chat));
        assert_eq!("CHAT".parse::<SessionMode>(), Ok(SessionMode::Chat));
        assert_eq!("agent".parse::<SessionMode>(), Ok(SessionMode::Agent));
        assert_eq!("Agent".parse::<SessionMode>(), Ok(SessionMode::Agent));
        assert!("invalid".parse::<SessionMode>().is_err());
    }

    #[test]
    fn test_session_mode_default() {
        assert_eq!(SessionMode::default(), SessionMode::Chat);
    }

    #[test]
    fn test_session_mode_display() {
        assert_eq!(format!("{}", SessionMode::Chat), "chat");
        assert_eq!(format!("{}", SessionMode::Agent), "agent");
    }

    #[test]
    fn test_session_mode_serialization() {
        let chat = SessionMode::Chat;
        let json = serde_json::to_string(&chat).unwrap();
        assert_eq!(json, "\"chat\"");

        let agent = SessionMode::Agent;
        let json = serde_json::to_string(&agent).unwrap();
        assert_eq!(json, "\"agent\"");
    }

    #[test]
    fn test_session_mode_deserialization() {
        let chat: SessionMode = serde_json::from_str("\"chat\"").unwrap();
        assert_eq!(chat, SessionMode::Chat);

        let agent: SessionMode = serde_json::from_str("\"agent\"").unwrap();
        assert_eq!(agent, SessionMode::Agent);
    }

    #[test]
    fn test_session_new() {
        let session = Session::new();
        assert!(!session.id.is_empty());
        assert!(session.title.is_none());
        assert_eq!(session.mode, SessionMode::Chat);
        assert!(!session.pinned);
        assert!(!session.archived);
        assert!(session.metadata.is_none());
        assert!(session.created_at > 0);
        assert_eq!(session.created_at, session.updated_at);
    }

    #[test]
    fn test_session_builder() {
        let session = Session::new()
            .with_id("test-id")
            .with_title("Test Session")
            .with_mode(SessionMode::Agent);

        assert_eq!(session.id, "test-id");
        assert_eq!(session.title, Some("Test Session".to_string()));
        assert_eq!(session.mode, SessionMode::Agent);
    }

    #[test]
    fn test_session_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));

        let session = Session::new().with_metadata(metadata.clone());
        assert_eq!(session.metadata, Some(metadata));
    }

    #[test]
    fn test_create_session_params_builder() {
        let params = CreateSessionParams::new()
            .with_id("custom-id")
            .with_title("My Session")
            .with_mode(SessionMode::Agent)
            .with_pinned(true);

        assert_eq!(params.id, Some("custom-id".to_string()));
        assert_eq!(params.title, Some("My Session".to_string()));
        assert_eq!(params.mode, Some(SessionMode::Agent));
        assert_eq!(params.pinned, Some(true));
    }

    #[test]
    fn test_update_session_params_has_changes() {
        let params = UpdateSessionParams::new();
        assert!(!params.has_changes());

        let params = UpdateSessionParams::new().with_title("New Title");
        assert!(params.has_changes());

        let params = UpdateSessionParams::new().with_pinned(true);
        assert!(params.has_changes());
    }

    #[test]
    fn test_update_session_params_clear_title() {
        let params = UpdateSessionParams::new().clear_title();
        assert_eq!(params.title, Some(None));
        assert!(params.has_changes());
    }

    #[test]
    fn test_list_sessions_params_builder() {
        let params = ListSessionsParams::new()
            .include_archived(true)
            .with_mode(SessionMode::Agent)
            .with_pinned(true)
            .with_limit(10)
            .with_offset(5)
            .with_search("test");

        assert_eq!(params.include_archived, Some(true));
        assert_eq!(params.mode, Some(SessionMode::Agent));
        assert_eq!(params.pinned, Some(true));
        assert_eq!(params.limit, Some(10));
        assert_eq!(params.offset, Some(5));
        assert_eq!(params.search, Some("test".to_string()));
    }

    #[test]
    fn test_uuid_v4_uniqueness() {
        let id1 = uuid_v4();
        let id2 = uuid_v4();
        // IDs should be different (with high probability)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_uuid_v4_format() {
        let id = uuid_v4();
        assert!(id.starts_with("sess-"));
        assert!(id.len() > 20);
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable Unix timestamp (after year 2020)
        assert!(ts > 1577836800);
    }

    #[test]
    fn test_session_serialization() {
        let session = Session::new()
            .with_id("test-123")
            .with_title("Test")
            .with_mode(SessionMode::Agent);

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, session.id);
        assert_eq!(deserialized.title, session.title);
        assert_eq!(deserialized.mode, session.mode);
    }
}
