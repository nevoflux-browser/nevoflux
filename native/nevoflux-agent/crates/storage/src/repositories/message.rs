//! Message repository for database operations.

use rusqlite::{params, OptionalExtension, Row};
use std::collections::HashMap;

use crate::connection::Database;
use crate::error::{Result, StorageError};
use crate::models::{CreateMessageParams, ListMessagesParams, Message};

/// Generate a simple UUID v4-like identifier for messages.
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Format: msg-{timestamp_hex}-{random_hex}
    let random_part: u64 = (timestamp as u64).wrapping_mul(6364136223846793005);
    format!("msg-{:016x}-{:08x}", timestamp as u64, random_part as u32)
}

/// Get the current Unix timestamp.
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Repository for message CRUD operations.
pub struct MessageRepository<'a> {
    db: &'a Database,
}

impl<'a> MessageRepository<'a> {
    /// Create a new message repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new message.
    pub fn create(&self, params: CreateMessageParams) -> Result<Message> {
        let id = params.id.unwrap_or_else(uuid_v4);
        let now = current_timestamp();
        let content_type = params.content_type.unwrap_or_default();
        let metadata_json = params
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, content_type, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    params.session_id,
                    params.role.as_str(),
                    params.content,
                    content_type.as_str(),
                    now,
                    metadata_json,
                ],
            )?;

            Ok(Message {
                id,
                session_id: params.session_id,
                role: params.role,
                content: params.content,
                content_type,
                created_at: now,
                metadata: params.metadata,
            })
        })
    }

    /// Get a message by ID.
    pub fn get(&self, id: &str) -> Result<Option<Message>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, session_id, role, content, content_type, created_at, metadata
                     FROM messages WHERE id = ?1",
                    params![id],
                    row_to_message,
                )
                .optional()?;

            match result {
                Some(message_result) => Ok(Some(message_result?)),
                None => Ok(None),
            }
        })
    }

    /// Delete a message by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        })
    }

    /// List messages with filtering and pagination.
    /// Messages are ordered by created_at ASC (oldest first).
    pub fn list(&self, params: ListMessagesParams) -> Result<Vec<Message>> {
        self.db.with_connection(|conn| {
            let mut conditions = vec!["session_id = ?".to_string()];
            let mut values: Vec<Box<dyn rusqlite::ToSql>> =
                vec![Box::new(params.session_id.clone())];

            // Handle before_id filter - get messages created before the referenced message
            if let Some(ref before_id) = params.before_id {
                conditions.push(
                    "created_at < (SELECT created_at FROM messages WHERE id = ?)".to_string(),
                );
                values.push(Box::new(before_id.clone()));
            }

            // Handle after_id filter - get messages created after the referenced message
            if let Some(ref after_id) = params.after_id {
                conditions.push(
                    "created_at > (SELECT created_at FROM messages WHERE id = ?)".to_string(),
                );
                values.push(Box::new(after_id.clone()));
            }

            let where_clause = format!("WHERE {}", conditions.join(" AND "));

            // In SQLite, OFFSET requires LIMIT, so we use a high limit when only offset is specified
            let (limit_clause, offset_clause) = match (params.limit, params.offset) {
                (Some(limit), Some(offset)) => {
                    (format!(" LIMIT {}", limit), format!(" OFFSET {}", offset))
                }
                (Some(limit), None) => (format!(" LIMIT {}", limit), String::new()),
                (None, Some(offset)) => (" LIMIT -1".to_string(), format!(" OFFSET {}", offset)),
                (None, None) => (String::new(), String::new()),
            };

            let sql = format!(
                "SELECT id, session_id, role, content, content_type, created_at, metadata
                 FROM messages {} ORDER BY created_at ASC, rowid ASC{}{}",
                where_clause, limit_clause, offset_clause
            );

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v.as_ref()).collect();
            let mut stmt = conn.prepare(&sql)?;

            let messages = stmt
                .query_map(params_refs.as_slice(), row_to_message)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(messages)
        })
    }

    /// Count messages in a session.
    pub fn count(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?;
            Ok(count as u32)
        })
    }

    /// Delete all messages in a session.
    /// Returns the number of deleted messages.
    pub fn delete_by_session(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "DELETE FROM messages WHERE session_id = ?1",
                params![session_id],
            )?;
            Ok(rows_affected as u32)
        })
    }

    /// List the most recent N messages for a session, returned in chronological order.
    ///
    /// Uses `ORDER BY created_at DESC LIMIT N` with the composite index
    /// `(session_id, created_at DESC)` to avoid loading all messages, then
    /// reverses the result to return oldest-first order.
    pub fn list_recent(&self, session_id: &str, limit: u32) -> Result<Vec<Message>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, role, content, content_type, created_at, metadata
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY created_at DESC, rowid DESC
                 LIMIT ?2",
            )?;

            let mut messages = stmt
                .query_map(params![session_id, limit], row_to_message)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            // Reverse to chronological order (oldest first)
            messages.reverse();
            Ok(messages)
        })
    }

    /// Get the last (most recent) message in a session.
    pub fn get_last(&self, session_id: &str) -> Result<Option<Message>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, session_id, role, content, content_type, created_at, metadata
                     FROM messages WHERE session_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                    params![session_id],
                    row_to_message,
                )
                .optional()?;

            match result {
                Some(message_result) => Ok(Some(message_result?)),
                None => Ok(None),
            }
        })
    }
}

/// Convert a database row to a Message.
fn row_to_message(row: &Row<'_>) -> rusqlite::Result<Result<Message>> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let role_str: String = row.get(2)?;
    let content: String = row.get(3)?;
    let content_type_str: String = row.get(4)?;
    let created_at: i64 = row.get(5)?;
    let metadata_json: Option<String> = row.get(6)?;

    let role = role_str.parse().unwrap_or_default();
    let content_type = content_type_str.parse().unwrap_or_default();

    let metadata: Option<HashMap<String, serde_json::Value>> = match metadata_json {
        Some(json) => match serde_json::from_str(&json) {
            Ok(m) => Some(m),
            Err(e) => return Ok(Err(StorageError::Json(e))),
        },
        None => None,
    };

    Ok(Ok(Message {
        id,
        session_id,
        role,
        content,
        content_type,
        created_at,
        metadata,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ContentType, CreateSessionParams, MessageRole};
    use crate::repositories::SessionRepository;

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn create_test_session(db: &Database, id: &str) {
        let repo = SessionRepository::new(db);
        repo.create(CreateSessionParams::new().with_id(id)).unwrap();
    }

    #[test]
    fn test_create_and_get_message() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let params = CreateMessageParams::new("test-session", MessageRole::User, "Hello, world!");
        let created = repo.create(params).unwrap();

        assert!(!created.id.is_empty());
        assert_eq!(created.session_id, "test-session");
        assert_eq!(created.role, MessageRole::User);
        assert_eq!(created.content, "Hello, world!");
        assert_eq!(created.content_type, ContentType::Text);
        assert!(created.created_at > 0);
        assert!(created.metadata.is_none());

        let fetched = repo.get(&created.id).unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.content, created.content);
    }

    #[test]
    fn test_create_message_with_custom_id() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let params = CreateMessageParams::new("test-session", MessageRole::Assistant, "Response")
            .with_id("custom-msg-id");
        let created = repo.create(params).unwrap();

        assert_eq!(created.id, "custom-msg-id");
    }

    #[test]
    fn test_create_message_with_content_type() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let params = CreateMessageParams::new(
            "test-session",
            MessageRole::Assistant,
            "{\"tool\": \"test\"}",
        )
        .with_content_type(ContentType::ToolUse);
        let created = repo.create(params).unwrap();

        assert_eq!(created.content_type, ContentType::ToolUse);

        let fetched = repo.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched.content_type, ContentType::ToolUse);
    }

    #[test]
    fn test_create_message_with_metadata() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));
        metadata.insert("number".to_string(), serde_json::json!(42));

        let params =
            CreateMessageParams::new("test-session", MessageRole::System, "System message")
                .with_metadata(metadata.clone());
        let created = repo.create(params).unwrap();

        assert_eq!(created.metadata, Some(metadata.clone()));

        let fetched = repo.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched.metadata, Some(metadata));
    }

    #[test]
    fn test_get_message_not_found() {
        let db = setup_db();
        let repo = MessageRepository::new(&db);

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_message() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let params = CreateMessageParams::new("test-session", MessageRole::User, "Delete me");
        let created = repo.create(params).unwrap();

        let deleted = repo.delete(&created.id).unwrap();
        assert!(deleted);

        let fetched = repo.get(&created.id).unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn test_delete_message_not_found() {
        let db = setup_db();
        let repo = MessageRepository::new(&db);

        let deleted = repo.delete("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_list_messages_default() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        // Create messages with small delays to ensure ordering
        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::User,
            "First",
        ))
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::Assistant,
            "Second",
        ))
        .unwrap();

        let messages = repo.list(ListMessagesParams::new("test-session")).unwrap();
        assert_eq!(messages.len(), 2);
        // Should be ordered by created_at ASC (oldest first)
        assert_eq!(messages[0].content, "First");
        assert_eq!(messages[1].content, "Second");
    }

    #[test]
    fn test_list_messages_with_limit() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        for i in 0..5 {
            repo.create(CreateMessageParams::new(
                "test-session",
                MessageRole::User,
                format!("Message {}", i),
            ))
            .unwrap();
        }

        let messages = repo
            .list(ListMessagesParams::new("test-session").with_limit(3))
            .unwrap();
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn test_list_messages_with_offset() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        for i in 0..5 {
            repo.create(CreateMessageParams::new(
                "test-session",
                MessageRole::User,
                format!("Message {}", i),
            ))
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let messages = repo
            .list(ListMessagesParams::new("test-session").with_offset(3))
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Message 3");
        assert_eq!(messages[1].content, "Message 4");
    }

    #[test]
    fn test_list_messages_pagination() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        for i in 0..5 {
            repo.create(CreateMessageParams::new(
                "test-session",
                MessageRole::User,
                format!("Message {}", i),
            ))
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let first_page = repo
            .list(ListMessagesParams::new("test-session").with_limit(2))
            .unwrap();
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].content, "Message 0");
        assert_eq!(first_page[1].content, "Message 1");

        let second_page = repo
            .list(
                ListMessagesParams::new("test-session")
                    .with_limit(2)
                    .with_offset(2),
            )
            .unwrap();
        assert_eq!(second_page.len(), 2);
        assert_eq!(second_page[0].content, "Message 2");
        assert_eq!(second_page[1].content, "Message 3");
    }

    #[test]
    fn test_list_messages_before_id() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let msg1 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "First")
                    .with_id("msg-1"),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let _msg2 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "Second")
                    .with_id("msg-2"),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let msg3 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "Third")
                    .with_id("msg-3"),
            )
            .unwrap();

        // Get messages before msg3
        let messages = repo
            .list(ListMessagesParams::new("test-session").before(&msg3.id))
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, msg1.id);
    }

    #[test]
    fn test_list_messages_after_id() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let msg1 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "First")
                    .with_id("msg-1"),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let _msg2 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "Second")
                    .with_id("msg-2"),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let msg3 = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "Third")
                    .with_id("msg-3"),
            )
            .unwrap();

        // Get messages after msg1
        let messages = repo
            .list(ListMessagesParams::new("test-session").after(&msg1.id))
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].id, msg3.id);
    }

    #[test]
    fn test_list_messages_only_for_session() {
        let db = setup_db();
        create_test_session(&db, "session-1");
        create_test_session(&db, "session-2");
        let repo = MessageRepository::new(&db);

        repo.create(CreateMessageParams::new(
            "session-1",
            MessageRole::User,
            "Session 1 message",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "session-2",
            MessageRole::User,
            "Session 2 message",
        ))
        .unwrap();

        let messages = repo.list(ListMessagesParams::new("session-1")).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "session-1");
    }

    #[test]
    fn test_count_messages() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        assert_eq!(repo.count("test-session").unwrap(), 0);

        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::User,
            "One",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::Assistant,
            "Two",
        ))
        .unwrap();

        assert_eq!(repo.count("test-session").unwrap(), 2);
    }

    #[test]
    fn test_count_messages_only_for_session() {
        let db = setup_db();
        create_test_session(&db, "session-1");
        create_test_session(&db, "session-2");
        let repo = MessageRepository::new(&db);

        repo.create(CreateMessageParams::new(
            "session-1",
            MessageRole::User,
            "S1 M1",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "session-1",
            MessageRole::User,
            "S1 M2",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "session-2",
            MessageRole::User,
            "S2 M1",
        ))
        .unwrap();

        assert_eq!(repo.count("session-1").unwrap(), 2);
        assert_eq!(repo.count("session-2").unwrap(), 1);
    }

    #[test]
    fn test_delete_by_session() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::User,
            "One",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::User,
            "Two",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "test-session",
            MessageRole::User,
            "Three",
        ))
        .unwrap();

        let deleted_count = repo.delete_by_session("test-session").unwrap();
        assert_eq!(deleted_count, 3);

        assert_eq!(repo.count("test-session").unwrap(), 0);
    }

    #[test]
    fn test_delete_by_session_only_affects_target() {
        let db = setup_db();
        create_test_session(&db, "session-1");
        create_test_session(&db, "session-2");
        let repo = MessageRepository::new(&db);

        repo.create(CreateMessageParams::new(
            "session-1",
            MessageRole::User,
            "S1 M1",
        ))
        .unwrap();
        repo.create(CreateMessageParams::new(
            "session-2",
            MessageRole::User,
            "S2 M1",
        ))
        .unwrap();

        let deleted_count = repo.delete_by_session("session-1").unwrap();
        assert_eq!(deleted_count, 1);

        assert_eq!(repo.count("session-1").unwrap(), 0);
        assert_eq!(repo.count("session-2").unwrap(), 1);
    }

    #[test]
    fn test_delete_by_session_empty() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let deleted_count = repo.delete_by_session("test-session").unwrap();
        assert_eq!(deleted_count, 0);
    }

    #[test]
    fn test_get_last_message() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        repo.create(
            CreateMessageParams::new("test-session", MessageRole::User, "First").with_id("msg-1"),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let last = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::Assistant, "Last")
                    .with_id("msg-2"),
            )
            .unwrap();

        let fetched = repo.get_last("test-session").unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, last.id);
    }

    #[test]
    fn test_get_last_message_empty_session() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let fetched = repo.get_last("test-session").unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn test_get_last_message_only_for_session() {
        let db = setup_db();
        create_test_session(&db, "session-1");
        create_test_session(&db, "session-2");
        let repo = MessageRepository::new(&db);

        let s1_msg = repo
            .create(
                CreateMessageParams::new("session-1", MessageRole::User, "Session 1 last")
                    .with_id("s1-msg"),
            )
            .unwrap();
        repo.create(
            CreateMessageParams::new("session-2", MessageRole::User, "Session 2 last")
                .with_id("s2-msg"),
        )
        .unwrap();

        let fetched = repo.get_last("session-1").unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, s1_msg.id);
    }

    #[test]
    fn test_all_message_roles() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let user_msg = repo
            .create(CreateMessageParams::new(
                "test-session",
                MessageRole::User,
                "User",
            ))
            .unwrap();
        let assistant_msg = repo
            .create(CreateMessageParams::new(
                "test-session",
                MessageRole::Assistant,
                "Assistant",
            ))
            .unwrap();
        let system_msg = repo
            .create(CreateMessageParams::new(
                "test-session",
                MessageRole::System,
                "System",
            ))
            .unwrap();

        let fetched_user = repo.get(&user_msg.id).unwrap().unwrap();
        assert_eq!(fetched_user.role, MessageRole::User);

        let fetched_assistant = repo.get(&assistant_msg.id).unwrap().unwrap();
        assert_eq!(fetched_assistant.role, MessageRole::Assistant);

        let fetched_system = repo.get(&system_msg.id).unwrap().unwrap();
        assert_eq!(fetched_system.role, MessageRole::System);
    }

    #[test]
    fn test_all_content_types() {
        let db = setup_db();
        create_test_session(&db, "test-session");
        let repo = MessageRepository::new(&db);

        let text_msg = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "text")
                    .with_content_type(ContentType::Text),
            )
            .unwrap();
        let image_msg = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "image data")
                    .with_content_type(ContentType::Image),
            )
            .unwrap();
        let tool_use_msg = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::Assistant, "tool call")
                    .with_content_type(ContentType::ToolUse),
            )
            .unwrap();
        let tool_result_msg = repo
            .create(
                CreateMessageParams::new("test-session", MessageRole::User, "tool result")
                    .with_content_type(ContentType::ToolResult),
            )
            .unwrap();

        assert_eq!(
            repo.get(&text_msg.id).unwrap().unwrap().content_type,
            ContentType::Text
        );
        assert_eq!(
            repo.get(&image_msg.id).unwrap().unwrap().content_type,
            ContentType::Image
        );
        assert_eq!(
            repo.get(&tool_use_msg.id).unwrap().unwrap().content_type,
            ContentType::ToolUse
        );
        assert_eq!(
            repo.get(&tool_result_msg.id).unwrap().unwrap().content_type,
            ContentType::ToolResult
        );
    }
}
