//! Storage facade providing unified access to all repositories.

use std::path::Path;

use crate::connection::Database;
use crate::error::Result;
use crate::repositories::traces::TraceRepository;
use crate::repositories::{
    ArtifactRepository, ConfigRepository, KnowledgeRepository, LearningMetricsRepository,
    LoopRepository, MessageRepository, PermissionRepository, SessionRepository,
    SiteAdaptationRepository, ToolStatsRepository,
};

/// Main storage facade providing access to all repositories.
///
/// This struct serves as the primary entry point for database operations,
/// providing convenient access to session, message, permission, and config
/// repositories through a single interface.
pub struct Storage {
    db: Database,
}

impl Storage {
    /// Open storage at the given path.
    ///
    /// Creates the database file if it doesn't exist and runs migrations.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::open(path)?;
        Ok(Self { db })
    }

    /// Create an in-memory storage for testing.
    ///
    /// The database is created fresh and all migrations are applied.
    pub fn open_in_memory() -> Result<Self> {
        let db = Database::open_in_memory()?;
        Ok(Self { db })
    }

    /// Get a session repository.
    ///
    /// Use this to create, read, update, and delete sessions.
    pub fn sessions(&self) -> SessionRepository<'_> {
        SessionRepository::new(&self.db)
    }

    /// Get a message repository.
    ///
    /// Use this to create, read, and delete messages within sessions.
    pub fn messages(&self) -> MessageRepository<'_> {
        MessageRepository::new(&self.db)
    }

    /// Get a permission repository.
    ///
    /// Use this to manage permissions for resources and actions.
    pub fn permissions(&self) -> PermissionRepository<'_> {
        PermissionRepository::new(&self.db)
    }

    /// Get a config repository.
    ///
    /// Use this to store and retrieve configuration key-value pairs.
    pub fn config(&self) -> ConfigRepository<'_> {
        ConfigRepository::new(&self.db)
    }

    /// Get a trace repository.
    ///
    /// Use this to create, read, and delete trace span records.
    pub fn traces(&self) -> TraceRepository<'_> {
        TraceRepository::new(&self.db)
    }

    /// Get an artifact repository.
    ///
    /// Use this to create, read, and delete artifacts.
    pub fn artifacts(&self) -> ArtifactRepository<'_> {
        ArtifactRepository::new(&self.db)
    }

    /// Get a knowledge repository.
    ///
    /// Use this to create, read, update, and delete knowledge entries.
    pub fn knowledge(&self) -> KnowledgeRepository<'_> {
        KnowledgeRepository::new(&self.db)
    }

    /// Get a site adaptation repository.
    ///
    /// Use this to create, read, update, and delete site adaptation records.
    pub fn site_adaptations(&self) -> SiteAdaptationRepository<'_> {
        SiteAdaptationRepository::new(&self.db)
    }

    /// Get a tool stats repository.
    ///
    /// Use this to create, read, and update tool effectiveness statistics.
    pub fn tool_stats(&self) -> ToolStatsRepository<'_> {
        ToolStatsRepository::new(&self.db)
    }

    /// Get a learning metrics repository.
    ///
    /// Use this to create, query, and delete learning system metrics.
    pub fn learning_metrics(&self) -> LearningMetricsRepository<'_> {
        LearningMetricsRepository::new(&self.db)
    }

    /// Get a loop repository.
    ///
    /// Use this to create, read, and update /loop skill records and
    /// their per-iteration history.
    pub fn loops(&self) -> LoopRepository<'_> {
        LoopRepository::new(&self.db)
    }

    /// Get the underlying database (for advanced operations).
    ///
    /// This provides direct access to the database connection for cases
    /// where repository operations are insufficient.
    pub fn database(&self) -> &Database {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CheckPermissionParams, CreateMessageParams, CreatePermissionParams, CreateSessionParams,
        ListMessagesParams, MessageRole, PermissionScope,
    };

    #[test]
    fn test_storage_open_in_memory() {
        let storage = Storage::open_in_memory();
        assert!(storage.is_ok());
    }

    #[test]
    fn test_storage_session_repository() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a session
        let session = storage
            .sessions()
            .create(
                CreateSessionParams::new()
                    .with_id("test-session")
                    .with_title("Test"),
            )
            .unwrap();

        assert_eq!(session.id, "test-session");
        assert_eq!(session.title, Some("Test".to_string()));

        // Retrieve the session
        let fetched = storage.sessions().get("test-session").unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, "test-session");

        // Count sessions
        let count = storage.sessions().count(false).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_storage_message_repository() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a session first
        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("msg-test-session"))
            .unwrap();

        // Create a message
        let message = storage
            .messages()
            .create(CreateMessageParams::new(
                "msg-test-session",
                MessageRole::User,
                "Hello, world!",
            ))
            .unwrap();

        assert!(!message.id.is_empty());
        assert_eq!(message.session_id, "msg-test-session");
        assert_eq!(message.content, "Hello, world!");

        // Count messages
        let count = storage.messages().count("msg-test-session").unwrap();
        assert_eq!(count, 1);

        // List messages
        let messages = storage
            .messages()
            .list(ListMessagesParams::new("msg-test-session"))
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_storage_permission_repository() {
        let storage = Storage::open_in_memory().unwrap();

        // Create permissions - one with exact match, one with wildcard
        let permission = storage
            .permissions()
            .create(
                CreatePermissionParams::new("file", "read", "/home/user/docs")
                    .with_scope(PermissionScope::Global)
                    .with_granted(true),
            )
            .unwrap();

        assert!(!permission.id.is_empty());
        assert_eq!(permission.resource_type, "file");
        assert_eq!(permission.action, "read");
        assert!(permission.granted);

        // Check exact match permission
        let result = storage
            .permissions()
            .check(CheckPermissionParams::new(
                "file",
                "read",
                "/home/user/docs",
            ))
            .unwrap();
        assert_eq!(result, Some(true));

        // Create a wildcard permission for tool execution
        storage
            .permissions()
            .create(
                CreatePermissionParams::new("tool", "execute", "*")
                    .with_scope(PermissionScope::Global)
                    .with_granted(true),
            )
            .unwrap();

        // Check wildcard permission
        let result = storage
            .permissions()
            .check(CheckPermissionParams::new("tool", "execute", "bash"))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_storage_config_repository() {
        let storage = Storage::open_in_memory().unwrap();

        // Set a config value
        storage
            .config()
            .set("app.name", serde_json::json!("NevoFlux"))
            .unwrap();

        // Get the config value
        let value = storage.config().get("app.name").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap(), serde_json::json!("NevoFlux"));

        // Check if exists
        assert!(storage.config().exists("app.name").unwrap());

        // List config entries
        let entries = storage.config().list().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_storage_cross_repository_operations() {
        let storage = Storage::open_in_memory().unwrap();

        // Create a session
        let session = storage
            .sessions()
            .create(
                CreateSessionParams::new()
                    .with_id("cross-repo-session")
                    .with_title("Cross Repository Test"),
            )
            .unwrap();

        // Add messages to the session
        storage
            .messages()
            .create(
                CreateMessageParams::new(&session.id, MessageRole::User, "First message")
                    .with_id("msg-1"),
            )
            .unwrap();

        storage
            .messages()
            .create(
                CreateMessageParams::new(&session.id, MessageRole::Assistant, "Second message")
                    .with_id("msg-2"),
            )
            .unwrap();

        storage
            .messages()
            .create(
                CreateMessageParams::new(&session.id, MessageRole::User, "Third message")
                    .with_id("msg-3"),
            )
            .unwrap();

        // Add a permission for this session
        storage
            .permissions()
            .create(
                CreatePermissionParams::new("tool", "execute", "bash")
                    .with_scope(PermissionScope::Session)
                    .with_session_id(&session.id)
                    .with_granted(true),
            )
            .unwrap();

        // Store some session-specific config
        storage
            .config()
            .set(
                &format!("session.{}.preferences", session.id),
                serde_json::json!({"theme": "dark"}),
            )
            .unwrap();

        // Verify counts
        assert_eq!(storage.sessions().count(false).unwrap(), 1);
        assert_eq!(storage.messages().count(&session.id).unwrap(), 3);

        // Verify permission works for the session
        let perm_result = storage
            .permissions()
            .check(
                CheckPermissionParams::new("tool", "execute", "bash").with_session_id(&session.id),
            )
            .unwrap();
        assert_eq!(perm_result, Some(true));

        // Verify config exists
        assert!(storage
            .config()
            .exists(&format!("session.{}.preferences", session.id))
            .unwrap());

        // Cleanup: delete session, messages, permissions
        storage.messages().delete_by_session(&session.id).unwrap();
        storage
            .permissions()
            .delete_by_session(&session.id)
            .unwrap();
        storage.sessions().delete(&session.id).unwrap();

        // Verify cleanup
        assert_eq!(storage.sessions().count(false).unwrap(), 0);
        assert_eq!(storage.messages().count(&session.id).unwrap(), 0);
    }

    #[test]
    fn test_storage_database_access() {
        let storage = Storage::open_in_memory().unwrap();

        // Access the underlying database
        let db = storage.database();

        // Use it directly to verify tables exist
        let tables = db
            .with_connection(|conn| {
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
                let names: Vec<String> = stmt
                    .query_map([], |row| row.get(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(names)
            })
            .unwrap();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"permissions".to_string()));
        assert!(tables.contains(&"config".to_string()));
    }

    #[test]
    fn test_storage_multiple_sessions_with_messages() {
        let storage = Storage::open_in_memory().unwrap();

        // Create multiple sessions
        for i in 1..=3 {
            let session_id = format!("session-{}", i);
            storage
                .sessions()
                .create(
                    CreateSessionParams::new()
                        .with_id(&session_id)
                        .with_title(format!("Session {}", i)),
                )
                .unwrap();

            // Add messages to each session
            for j in 1..=i {
                storage
                    .messages()
                    .create(CreateMessageParams::new(
                        &session_id,
                        MessageRole::User,
                        format!("Message {} in session {}", j, i),
                    ))
                    .unwrap();
            }
        }

        // Verify counts
        assert_eq!(storage.sessions().count(false).unwrap(), 3);
        assert_eq!(storage.messages().count("session-1").unwrap(), 1);
        assert_eq!(storage.messages().count("session-2").unwrap(), 2);
        assert_eq!(storage.messages().count("session-3").unwrap(), 3);
    }
}
