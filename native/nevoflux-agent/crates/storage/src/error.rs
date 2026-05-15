//! Error types for the storage layer.

use thiserror::Error;

/// Errors that can occur in the storage layer.
#[derive(Error, Debug)]
pub enum StorageError {
    /// SQLite database error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Entity not found in storage.
    #[error("Not found: {entity} with id '{id}'")]
    NotFound {
        /// The type of entity that was not found.
        entity: String,
        /// The ID that was searched for.
        id: String,
    },

    /// Entity already exists in storage.
    #[error("Already exists: {entity} with id '{id}'")]
    AlreadyExists {
        /// The type of entity that already exists.
        entity: String,
        /// The ID that was attempted to be created.
        id: String,
    },

    /// Database migration failed.
    #[error("Database migration failed: {0}")]
    Migration(String),

    /// Connection pool error.
    #[error("Connection pool error: {0}")]
    Pool(String),
}

/// A specialized Result type for storage operations.
pub type Result<T> = std::result::Result<T, StorageError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error_message() {
        let err = StorageError::NotFound {
            entity: "session".to_string(),
            id: "sess-001".to_string(),
        };
        assert_eq!(err.to_string(), "Not found: session with id 'sess-001'");
    }

    #[test]
    fn test_already_exists_error_message() {
        let err = StorageError::AlreadyExists {
            entity: "session".to_string(),
            id: "sess-001".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Already exists: session with id 'sess-001'"
        );
    }

    #[test]
    fn test_sqlite_error_conversion() {
        let sqlite_err = rusqlite::Error::InvalidQuery;
        let storage_err: StorageError = sqlite_err.into();
        assert!(matches!(storage_err, StorageError::Sqlite(_)));
    }
}
