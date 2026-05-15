//! Storage test helpers.

use nevoflux_storage::Storage;

/// A test storage wrapper that provides an in-memory SQLite database.
///
/// This is a thin wrapper around `Storage::open_in_memory()` with
/// additional convenience methods for testing.
pub struct TestStorage {
    storage: Storage,
}

impl TestStorage {
    /// Create a new in-memory test storage.
    pub fn new() -> Self {
        Self {
            storage: Storage::open_in_memory().expect("Failed to create in-memory storage"),
        }
    }

    /// Check if the storage is empty (no sessions).
    pub fn is_empty(&self) -> bool {
        self.storage.sessions().count(true).unwrap_or(0) == 0
    }

    /// Get the underlying storage instance.
    pub fn inner(&self) -> &Storage {
        &self.storage
    }

    /// Get a mutable reference to the underlying storage.
    pub fn inner_mut(&mut self) -> &mut Storage {
        &mut self.storage
    }

    /// Get the session count (including archived).
    pub fn session_count(&self) -> u32 {
        self.storage.sessions().count(true).unwrap_or(0)
    }

    /// Get the message count for a session.
    pub fn message_count(&self, session_id: &str) -> u32 {
        self.storage.messages().count(session_id).unwrap_or(0)
    }
}

impl Default for TestStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for TestStorage {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl std::ops::DerefMut for TestStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::{CreateMessageParams, CreateSessionParams, MessageRole};

    #[test]
    fn test_test_storage_new() {
        let storage = TestStorage::new();
        assert!(storage.is_empty());
    }

    #[test]
    fn test_test_storage_session_count() {
        let storage = TestStorage::new();
        assert_eq!(storage.session_count(), 0);

        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("test-1"))
            .unwrap();
        assert_eq!(storage.session_count(), 1);
    }

    #[test]
    fn test_test_storage_message_count() {
        let storage = TestStorage::new();

        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("test-session"))
            .unwrap();

        assert_eq!(storage.message_count("test-session"), 0);

        storage
            .messages()
            .create(CreateMessageParams::new(
                "test-session",
                MessageRole::User,
                "Hello",
            ))
            .unwrap();

        assert_eq!(storage.message_count("test-session"), 1);
    }

    #[test]
    fn test_test_storage_deref() {
        let storage = TestStorage::new();

        // Should be able to call Storage methods directly
        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("deref-test"))
            .unwrap();

        let session = storage.sessions().get("deref-test").unwrap();
        assert!(session.is_some());
    }
}
