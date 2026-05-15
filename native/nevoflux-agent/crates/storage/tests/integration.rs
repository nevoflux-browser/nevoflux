//! Integration tests for the storage crate

use nevoflux_storage::{
    CheckPermissionParams, ContentType, CreateMessageParams, CreatePermissionParams,
    CreateSessionParams, ListMessagesParams, ListSessionsParams, MessageRole, PermissionScope,
    SessionMode, Storage, UpdateSessionParams,
};

#[test]
fn test_full_session_lifecycle() {
    let storage = Storage::open_in_memory().unwrap();

    // 1. Create session
    let session = storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("sess-lifecycle")
                .with_title("Test Session")
                .with_mode(SessionMode::Agent),
        )
        .unwrap();
    assert_eq!(session.mode, SessionMode::Agent);

    // 2. Add messages with delay to ensure ordering
    storage
        .messages()
        .create(
            CreateMessageParams::new("sess-lifecycle", MessageRole::User, "Hello!")
                .with_id("msg-1"),
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    storage
        .messages()
        .create(
            CreateMessageParams::new("sess-lifecycle", MessageRole::Assistant, "Hi there!")
                .with_id("msg-2"),
        )
        .unwrap();

    // 3. Verify messages
    let messages = storage
        .messages()
        .list(ListMessagesParams::new("sess-lifecycle"))
        .unwrap();
    assert_eq!(messages.len(), 2);

    // 4. Update session
    let updated = storage
        .sessions()
        .update(
            "sess-lifecycle",
            UpdateSessionParams::new().with_pinned(true),
        )
        .unwrap();
    assert!(updated.pinned);

    // 5. Get last message
    let last = storage
        .messages()
        .get_last("sess-lifecycle")
        .unwrap()
        .unwrap();
    assert_eq!(last.id, "msg-2");

    // 6. Delete messages and session
    storage
        .messages()
        .delete_by_session("sess-lifecycle")
        .unwrap();
    storage.sessions().delete("sess-lifecycle").unwrap();
    assert!(storage.sessions().get("sess-lifecycle").unwrap().is_none());
}

#[test]
fn test_permission_workflow() {
    let storage = Storage::open_in_memory().unwrap();

    // Grant global read
    storage
        .permissions()
        .create(
            CreatePermissionParams::new("file", "read", "*")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

    // Deny write to /etc
    storage
        .permissions()
        .create(
            CreatePermissionParams::new("file", "write", "/etc/*")
                .with_scope(PermissionScope::Global)
                .with_granted(false),
        )
        .unwrap();

    // Check permissions - exact pattern match for wildcard
    assert_eq!(
        storage
            .permissions()
            .check(CheckPermissionParams::new("file", "read", "*"))
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        storage
            .permissions()
            .check(CheckPermissionParams::new("file", "write", "/etc/*"))
            .unwrap(),
        Some(false)
    );
    // No permission set for shell
    assert_eq!(
        storage
            .permissions()
            .check(CheckPermissionParams::new("shell", "execute", "ls"))
            .unwrap(),
        None
    );
}

#[test]
fn test_config_persistence() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .config()
        .set("llm.provider", serde_json::json!("anthropic"))
        .unwrap();
    storage
        .config()
        .set("llm.model", serde_json::json!("claude"))
        .unwrap();
    storage
        .config()
        .set("session.timeout", serde_json::json!(30))
        .unwrap();

    let llm_configs = storage.config().list_by_prefix("llm.").unwrap();
    assert_eq!(llm_configs.len(), 2);

    let timeout: i32 = storage
        .config()
        .get_typed("session.timeout")
        .unwrap()
        .unwrap();
    assert_eq!(timeout, 30);
}

#[test]
fn test_session_listing_filters() {
    let storage = Storage::open_in_memory().unwrap();

    // Create 10 sessions, some pinned, some archived
    for i in 0..10 {
        let session = storage
            .sessions()
            .create(CreateSessionParams::new().with_id(format!("sess-{:02}", i)))
            .unwrap();

        if i % 3 == 0 {
            storage
                .sessions()
                .update(&session.id, UpdateSessionParams::new().with_pinned(true))
                .unwrap();
        }
        if i % 4 == 0 {
            storage
                .sessions()
                .update(&session.id, UpdateSessionParams::new().with_archived(true))
                .unwrap();
        }
    }

    // Test filters
    // Default excludes archived: 0,4,8 are archived, so 7 remain
    let default_list = storage.sessions().list(ListSessionsParams::new()).unwrap();
    assert_eq!(default_list.len(), 7);

    let with_archived = storage
        .sessions()
        .list(ListSessionsParams::new().include_archived(true))
        .unwrap();
    assert_eq!(with_archived.len(), 10);

    // Pinned: 0,3,6,9 but 0 is archived, so 3 remain (3,6,9)
    let pinned_only = storage
        .sessions()
        .list(ListSessionsParams::new().with_pinned(true))
        .unwrap();
    assert_eq!(pinned_only.len(), 3);
}

#[test]
fn test_message_ordering() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("sess-order"))
        .unwrap();

    for i in 0..5 {
        storage
            .messages()
            .create(
                CreateMessageParams::new("sess-order", MessageRole::User, format!("Message {}", i))
                    .with_id(format!("msg-{}", i)),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let messages = storage
        .messages()
        .list(ListMessagesParams::new("sess-order"))
        .unwrap();
    assert_eq!(messages.len(), 5);
    for (i, message) in messages.iter().enumerate() {
        assert_eq!(message.content, format!("Message {}", i));
    }
}

#[test]
fn test_session_with_different_modes() {
    let storage = Storage::open_in_memory().unwrap();

    // Create chat session
    let chat = storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("chat-session")
                .with_mode(SessionMode::Chat),
        )
        .unwrap();
    assert_eq!(chat.mode, SessionMode::Chat);

    // Create agent session
    let agent = storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("agent-session")
                .with_mode(SessionMode::Agent),
        )
        .unwrap();
    assert_eq!(agent.mode, SessionMode::Agent);

    // Filter by mode
    let chat_sessions = storage
        .sessions()
        .list(ListSessionsParams::new().with_mode(SessionMode::Chat))
        .unwrap();
    assert_eq!(chat_sessions.len(), 1);
    assert_eq!(chat_sessions[0].id, "chat-session");

    let agent_sessions = storage
        .sessions()
        .list(ListSessionsParams::new().with_mode(SessionMode::Agent))
        .unwrap();
    assert_eq!(agent_sessions.len(), 1);
    assert_eq!(agent_sessions[0].id, "agent-session");
}

#[test]
fn test_message_content_types() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("content-types"))
        .unwrap();

    // Text message
    let text = storage
        .messages()
        .create(
            CreateMessageParams::new("content-types", MessageRole::User, "Hello")
                .with_content_type(ContentType::Text),
        )
        .unwrap();
    assert_eq!(text.content_type, ContentType::Text);

    // Tool use message
    let tool_use = storage
        .messages()
        .create(
            CreateMessageParams::new(
                "content-types",
                MessageRole::Assistant,
                r#"{"tool": "bash", "input": "ls"}"#,
            )
            .with_content_type(ContentType::ToolUse),
        )
        .unwrap();
    assert_eq!(tool_use.content_type, ContentType::ToolUse);

    // Tool result message
    let tool_result = storage
        .messages()
        .create(
            CreateMessageParams::new("content-types", MessageRole::User, "file1.txt\nfile2.txt")
                .with_content_type(ContentType::ToolResult),
        )
        .unwrap();
    assert_eq!(tool_result.content_type, ContentType::ToolResult);

    // Image message
    let image = storage
        .messages()
        .create(
            CreateMessageParams::new("content-types", MessageRole::User, "base64_image_data")
                .with_content_type(ContentType::Image),
        )
        .unwrap();
    assert_eq!(image.content_type, ContentType::Image);
}

#[test]
fn test_permission_session_scope() {
    let storage = Storage::open_in_memory().unwrap();

    // Create a session
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("perm-session"))
        .unwrap();

    // Create session-scoped permission
    storage
        .permissions()
        .create(
            CreatePermissionParams::new("tool", "execute", "bash")
                .with_scope(PermissionScope::Session)
                .with_session_id("perm-session")
                .with_granted(true),
        )
        .unwrap();

    // Check permission with correct session
    let result = storage
        .permissions()
        .check(
            CheckPermissionParams::new("tool", "execute", "bash").with_session_id("perm-session"),
        )
        .unwrap();
    assert_eq!(result, Some(true));
}

#[test]
fn test_config_typed_values() {
    let storage = Storage::open_in_memory().unwrap();

    // Store different types
    storage
        .config()
        .set("string", serde_json::json!("hello"))
        .unwrap();
    storage
        .config()
        .set("number", serde_json::json!(42))
        .unwrap();
    storage
        .config()
        .set("float", serde_json::json!(1.5))
        .unwrap();
    storage
        .config()
        .set("bool", serde_json::json!(true))
        .unwrap();
    storage
        .config()
        .set("array", serde_json::json!([1, 2, 3]))
        .unwrap();
    storage
        .config()
        .set("object", serde_json::json!({"key": "value"}))
        .unwrap();

    // Retrieve typed values
    let s: String = storage.config().get_typed("string").unwrap().unwrap();
    assert_eq!(s, "hello");

    let n: i32 = storage.config().get_typed("number").unwrap().unwrap();
    assert_eq!(n, 42);

    let f: f64 = storage.config().get_typed("float").unwrap().unwrap();
    assert!((f - 1.5).abs() < 0.001);

    let b: bool = storage.config().get_typed("bool").unwrap().unwrap();
    assert!(b);

    let arr: Vec<i32> = storage.config().get_typed("array").unwrap().unwrap();
    assert_eq!(arr, vec![1, 2, 3]);

    let obj: std::collections::HashMap<String, String> =
        storage.config().get_typed("object").unwrap().unwrap();
    assert_eq!(obj.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_session_metadata() {
    let storage = Storage::open_in_memory().unwrap();

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("project".to_string(), serde_json::json!("nevoflux"));
    metadata.insert("version".to_string(), serde_json::json!(1));

    let session = storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("meta-session")
                .with_metadata(metadata.clone()),
        )
        .unwrap();

    assert_eq!(session.metadata, Some(metadata.clone()));

    // Verify persistence
    let fetched = storage.sessions().get("meta-session").unwrap().unwrap();
    assert_eq!(fetched.metadata, Some(metadata));
}

#[test]
fn test_message_metadata() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("msg-meta"))
        .unwrap();

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("tokens".to_string(), serde_json::json!(150));
    metadata.insert("model".to_string(), serde_json::json!("claude-3"));

    let message = storage
        .messages()
        .create(
            CreateMessageParams::new("msg-meta", MessageRole::Assistant, "Response")
                .with_metadata(metadata.clone()),
        )
        .unwrap();

    assert_eq!(message.metadata, Some(metadata.clone()));

    // Verify persistence
    let fetched = storage.messages().get(&message.id).unwrap().unwrap();
    assert_eq!(fetched.metadata, Some(metadata));
}

#[test]
fn test_multiple_sessions_isolation() {
    let storage = Storage::open_in_memory().unwrap();

    // Create two sessions
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("session-a"))
        .unwrap();
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("session-b"))
        .unwrap();

    // Add messages to each
    for i in 0..3 {
        storage
            .messages()
            .create(CreateMessageParams::new(
                "session-a",
                MessageRole::User,
                format!("A-{}", i),
            ))
            .unwrap();
    }

    for i in 0..5 {
        storage
            .messages()
            .create(CreateMessageParams::new(
                "session-b",
                MessageRole::User,
                format!("B-{}", i),
            ))
            .unwrap();
    }

    // Verify isolation
    assert_eq!(storage.messages().count("session-a").unwrap(), 3);
    assert_eq!(storage.messages().count("session-b").unwrap(), 5);

    let a_messages = storage
        .messages()
        .list(ListMessagesParams::new("session-a"))
        .unwrap();
    assert!(a_messages.iter().all(|m| m.session_id == "session-a"));

    let b_messages = storage
        .messages()
        .list(ListMessagesParams::new("session-b"))
        .unwrap();
    assert!(b_messages.iter().all(|m| m.session_id == "session-b"));
}

#[test]
fn test_session_update_multiple_fields() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("multi-update"))
        .unwrap();

    // Update multiple fields at once
    let updated = storage
        .sessions()
        .update(
            "multi-update",
            UpdateSessionParams::new()
                .with_title("New Title")
                .with_mode(SessionMode::Agent)
                .with_pinned(true),
        )
        .unwrap();

    assert_eq!(updated.title, Some("New Title".to_string()));
    assert_eq!(updated.mode, SessionMode::Agent);
    assert!(updated.pinned);
}

#[test]
fn test_config_delete_and_overwrite() {
    let storage = Storage::open_in_memory().unwrap();

    // Set initial value
    storage
        .config()
        .set("key", serde_json::json!("initial"))
        .unwrap();
    assert_eq!(
        storage.config().get("key").unwrap(),
        Some(serde_json::json!("initial"))
    );

    // Overwrite
    storage
        .config()
        .set("key", serde_json::json!("updated"))
        .unwrap();
    assert_eq!(
        storage.config().get("key").unwrap(),
        Some(serde_json::json!("updated"))
    );

    // Delete
    storage.config().delete("key").unwrap();
    assert_eq!(storage.config().get("key").unwrap(), None);
    assert!(!storage.config().exists("key").unwrap());
}

#[test]
fn test_permission_by_session() {
    let storage = Storage::open_in_memory().unwrap();

    // Create session
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("perm-list-session"))
        .unwrap();

    // Create global permission
    storage
        .permissions()
        .create(
            CreatePermissionParams::new("file", "read", "/home/*")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

    // Create session-scoped permissions
    storage
        .permissions()
        .create(
            CreatePermissionParams::new("file", "write", "/home/*")
                .with_scope(PermissionScope::Session)
                .with_session_id("perm-list-session")
                .with_granted(true),
        )
        .unwrap();

    storage
        .permissions()
        .create(
            CreatePermissionParams::new("tool", "execute", "bash")
                .with_scope(PermissionScope::Session)
                .with_session_id("perm-list-session")
                .with_granted(false),
        )
        .unwrap();

    // List permissions for session (includes global + session-scoped)
    let perms = storage
        .permissions()
        .list_by_session("perm-list-session")
        .unwrap();
    assert_eq!(perms.len(), 3);

    // Verify content
    let file_perms: Vec<_> = perms.iter().filter(|p| p.resource_type == "file").collect();
    assert_eq!(file_perms.len(), 2);

    let tool_perms: Vec<_> = perms.iter().filter(|p| p.resource_type == "tool").collect();
    assert_eq!(tool_perms.len(), 1);
    assert!(!tool_perms[0].granted);
}

#[test]
fn test_session_search() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("s1")
                .with_title("Project Alpha Development"),
        )
        .unwrap();
    storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("s2")
                .with_title("Project Beta Testing"),
        )
        .unwrap();
    storage
        .sessions()
        .create(
            CreateSessionParams::new()
                .with_id("s3")
                .with_title("Random Chat"),
        )
        .unwrap();

    // Search for "Project"
    let results = storage
        .sessions()
        .list(ListSessionsParams::new().with_search("Project"))
        .unwrap();
    assert_eq!(results.len(), 2);

    // Search for "Alpha"
    let results = storage
        .sessions()
        .list(ListSessionsParams::new().with_search("Alpha"))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "s1");
}

#[test]
fn test_message_pagination() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("paginated"))
        .unwrap();

    // Create 20 messages
    for i in 0..20 {
        storage
            .messages()
            .create(CreateMessageParams::new(
                "paginated",
                MessageRole::User,
                format!("Message {}", i),
            ))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // First page
    let page1 = storage
        .messages()
        .list(ListMessagesParams::new("paginated").with_limit(5))
        .unwrap();
    assert_eq!(page1.len(), 5);
    assert_eq!(page1[0].content, "Message 0");

    // Second page
    let page2 = storage
        .messages()
        .list(
            ListMessagesParams::new("paginated")
                .with_limit(5)
                .with_offset(5),
        )
        .unwrap();
    assert_eq!(page2.len(), 5);
    assert_eq!(page2[0].content, "Message 5");

    // Last page
    let page_last = storage
        .messages()
        .list(
            ListMessagesParams::new("paginated")
                .with_limit(5)
                .with_offset(15),
        )
        .unwrap();
    assert_eq!(page_last.len(), 5);
    assert_eq!(page_last[0].content, "Message 15");
}

#[test]
fn test_concurrent_operations() {
    let storage = Storage::open_in_memory().unwrap();

    // Create session
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("concurrent"))
        .unwrap();

    // Perform multiple operations
    for i in 0..10 {
        storage
            .messages()
            .create(CreateMessageParams::new(
                "concurrent",
                MessageRole::User,
                format!("Msg {}", i),
            ))
            .unwrap();
        storage
            .config()
            .set(&format!("key-{}", i), serde_json::json!(i))
            .unwrap();
    }

    // Verify all operations succeeded
    assert_eq!(storage.messages().count("concurrent").unwrap(), 10);
    let configs = storage.config().list().unwrap();
    assert_eq!(configs.len(), 10);
}

#[test]
fn test_session_count() {
    let storage = Storage::open_in_memory().unwrap();

    // Initially empty
    assert_eq!(storage.sessions().count(false).unwrap(), 0);

    // Create sessions
    for i in 0..5 {
        storage
            .sessions()
            .create(CreateSessionParams::new().with_id(format!("count-{}", i)))
            .unwrap();
    }

    // Archive some
    storage
        .sessions()
        .update("count-0", UpdateSessionParams::new().with_archived(true))
        .unwrap();
    storage
        .sessions()
        .update("count-1", UpdateSessionParams::new().with_archived(true))
        .unwrap();

    // Count excluding archived
    assert_eq!(storage.sessions().count(false).unwrap(), 3);

    // Count including archived
    assert_eq!(storage.sessions().count(true).unwrap(), 5);
}

#[test]
fn test_permission_expiration() {
    let storage = Storage::open_in_memory().unwrap();

    // Create session for permission listing
    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("expiry-session"))
        .unwrap();

    // Create permission with expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    storage
        .permissions()
        .create(
            CreatePermissionParams::new("temp", "access", "resource")
                .with_scope(PermissionScope::Session)
                .with_session_id("expiry-session")
                .with_granted(true)
                .with_expires_at(now + 3600), // Expires in 1 hour
        )
        .unwrap();

    let perms = storage
        .permissions()
        .list_by_session("expiry-session")
        .unwrap();
    assert_eq!(perms.len(), 1);
    assert!(perms[0].expires_at.is_some());
    assert_eq!(perms[0].expires_at.unwrap(), now + 3600);
}

#[test]
fn test_session_touch() {
    let storage = Storage::open_in_memory().unwrap();

    let session = storage
        .sessions()
        .create(CreateSessionParams::new().with_id("touch-test"))
        .unwrap();
    let original_updated = session.updated_at;

    // Wait and touch
    std::thread::sleep(std::time::Duration::from_secs(1));
    storage.sessions().touch("touch-test").unwrap();

    let updated = storage.sessions().get("touch-test").unwrap().unwrap();
    assert!(updated.updated_at > original_updated);
}

#[test]
fn test_empty_session_messages() {
    let storage = Storage::open_in_memory().unwrap();

    storage
        .sessions()
        .create(CreateSessionParams::new().with_id("empty"))
        .unwrap();

    // List empty session
    let messages = storage
        .messages()
        .list(ListMessagesParams::new("empty"))
        .unwrap();
    assert!(messages.is_empty());

    // Get last from empty session
    let last = storage.messages().get_last("empty").unwrap();
    assert!(last.is_none());

    // Count empty session
    assert_eq!(storage.messages().count("empty").unwrap(), 0);
}

#[test]
fn test_delete_nonexistent_entities() {
    let storage = Storage::open_in_memory().unwrap();

    // Delete nonexistent session returns false
    assert!(!storage.sessions().delete("nonexistent").unwrap());

    // Delete nonexistent message returns false
    assert!(!storage.messages().delete("nonexistent").unwrap());

    // Delete nonexistent permission returns false
    assert!(!storage.permissions().delete("nonexistent").unwrap());

    // Delete nonexistent config returns false
    assert!(!storage.config().delete("nonexistent").unwrap());
}
