//! Config repository for database operations.

use rusqlite::{params, types::Value as SqliteValue, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::connection::Database;
use crate::error::{Result, StorageError};
use crate::models::ConfigEntry;

/// Get the current Unix timestamp.
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Convert a SQLite Value to a JSON string representation.
fn sqlite_value_to_json_string(value: SqliteValue) -> String {
    match value {
        SqliteValue::Null => "null".to_string(),
        SqliteValue::Integer(i) => i.to_string(),
        SqliteValue::Real(f) => f.to_string(),
        SqliteValue::Text(s) => s,
        SqliteValue::Blob(b) => String::from_utf8_lossy(&b).to_string(),
    }
}

/// Repository for config CRUD operations.
pub struct ConfigRepository<'a> {
    db: &'a Database,
}

impl<'a> ConfigRepository<'a> {
    /// Create a new config repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Get a config value by key.
    /// Returns the raw JSON value if the key exists, None otherwise.
    pub fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT value FROM config WHERE key = ?1",
                    params![key],
                    |row| {
                        let value: SqliteValue = row.get(0)?;
                        Ok(sqlite_value_to_json_string(value))
                    },
                )
                .optional()?;

            match result {
                Some(value_str) => {
                    let value: serde_json::Value = serde_json::from_str(&value_str)?;
                    Ok(Some(value))
                }
                None => Ok(None),
            }
        })
    }

    /// Get a config value by key, deserializing it to a typed value.
    /// Returns None if the key doesn't exist.
    /// Returns an error if deserialization fails.
    pub fn get_typed<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get(key)? {
            Some(value) => {
                let typed: T = serde_json::from_value(value)?;
                Ok(Some(typed))
            }
            None => Ok(None),
        }
    }

    /// Set a config value by key.
    /// Uses upsert semantics (INSERT ON CONFLICT UPDATE).
    pub fn set(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let now = current_timestamp();
        let value_str = serde_json::to_string(&value)?;

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO config (key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                     value = excluded.value,
                     updated_at = excluded.updated_at",
                params![key, value_str, now],
            )?;
            Ok(())
        })
    }

    /// Set a config value by key, serializing from a typed value.
    /// Uses upsert semantics (INSERT ON CONFLICT UPDATE).
    pub fn set_typed<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json_value = serde_json::to_value(value)?;
        self.set(key, json_value)
    }

    /// Delete a config entry by key.
    /// Returns true if a row was deleted, false if the key didn't exist.
    pub fn delete(&self, key: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute("DELETE FROM config WHERE key = ?1", params![key])?;
            Ok(rows_affected > 0)
        })
    }

    /// List all config entries.
    pub fn list(&self) -> Result<Vec<ConfigEntry>> {
        self.db.with_connection(|conn| {
            let mut stmt =
                conn.prepare("SELECT key, value, updated_at FROM config ORDER BY key ASC")?;

            let entries = stmt
                .query_map([], row_to_config_entry)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(entries)
        })
    }

    /// List config entries with keys matching a prefix.
    /// Uses SQL LIKE with 'prefix%' pattern.
    pub fn list_by_prefix(&self, prefix: &str) -> Result<Vec<ConfigEntry>> {
        self.db.with_connection(|conn| {
            let pattern = format!("{}%", prefix);
            let mut stmt = conn.prepare(
                "SELECT key, value, updated_at FROM config WHERE key LIKE ?1 ORDER BY key ASC",
            )?;

            let entries = stmt
                .query_map(params![pattern], row_to_config_entry)?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>>>()?;

            Ok(entries)
        })
    }

    /// Check if a config key exists.
    pub fn exists(&self, key: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM config WHERE key = ?1)",
                params![key],
                |row| row.get(0),
            )?;
            Ok(exists)
        })
    }
}

/// Convert a database row to a ConfigEntry.
fn row_to_config_entry(row: &Row<'_>) -> rusqlite::Result<Result<ConfigEntry>> {
    let key: String = row.get(0)?;
    let value_raw: SqliteValue = row.get(1)?;
    let updated_at: i64 = row.get(2)?;

    let value_str = sqlite_value_to_json_string(value_raw);
    let value: serde_json::Value = match serde_json::from_str(&value_str) {
        Ok(v) => v,
        Err(e) => return Ok(Err(StorageError::Json(e))),
    };

    Ok(Ok(ConfigEntry {
        key,
        value,
        updated_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_set_and_get() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("test.key", serde_json::json!("test_value"))
            .unwrap();
        let value = repo.get("test.key").unwrap();

        assert!(value.is_some());
        assert_eq!(value.unwrap(), serde_json::json!("test_value"));
    }

    #[test]
    fn test_get_nonexistent() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        let value = repo.get("nonexistent.key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_overwrite() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("test.key", serde_json::json!("original")).unwrap();
        repo.set("test.key", serde_json::json!("updated")).unwrap();

        let value = repo.get("test.key").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap(), serde_json::json!("updated"));
    }

    #[test]
    fn test_get_typed_string() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("app.name", serde_json::json!("NevoFlux")).unwrap();

        let name: Option<String> = repo.get_typed("app.name").unwrap();
        assert_eq!(name, Some("NevoFlux".to_string()));
    }

    #[test]
    fn test_get_typed_number() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("app.port", serde_json::json!(8080)).unwrap();

        let port: Option<i32> = repo.get_typed("app.port").unwrap();
        assert_eq!(port, Some(8080));
    }

    #[test]
    fn test_get_typed_boolean() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("app.debug", serde_json::json!(true)).unwrap();

        let debug: Option<bool> = repo.get_typed("app.debug").unwrap();
        assert_eq!(debug, Some(true));
    }

    #[test]
    fn test_get_typed_struct() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Settings {
            theme: String,
            font_size: i32,
        }

        let settings = Settings {
            theme: "dark".to_string(),
            font_size: 14,
        };

        repo.set("app.settings", serde_json::to_value(&settings).unwrap())
            .unwrap();

        let retrieved: Option<Settings> = repo.get_typed("app.settings").unwrap();
        assert_eq!(retrieved, Some(settings));
    }

    #[test]
    fn test_get_typed_nonexistent() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        let value: Option<String> = repo.get_typed("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_set_typed_string() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set_typed("app.name", &"NevoFlux".to_string()).unwrap();

        let value = repo.get("app.name").unwrap();
        assert_eq!(value, Some(serde_json::json!("NevoFlux")));
    }

    #[test]
    fn test_set_typed_struct() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct DatabaseConfig {
            host: String,
            port: u16,
            name: String,
        }

        let config = DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            name: "nevoflux".to_string(),
        };

        repo.set_typed("db.config", &config).unwrap();

        let retrieved: Option<DatabaseConfig> = repo.get_typed("db.config").unwrap();
        assert_eq!(retrieved, Some(config));
    }

    #[test]
    fn test_delete() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("delete.me", serde_json::json!("value")).unwrap();
        assert!(repo.exists("delete.me").unwrap());

        let deleted = repo.delete("delete.me").unwrap();
        assert!(deleted);
        assert!(!repo.exists("delete.me").unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        let deleted = repo.delete("nonexistent.key").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_list() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("alpha", serde_json::json!(1)).unwrap();
        repo.set("beta", serde_json::json!(2)).unwrap();
        repo.set("gamma", serde_json::json!(3)).unwrap();

        let entries = repo.list().unwrap();
        assert_eq!(entries.len(), 3);

        // Should be ordered by key ASC
        assert_eq!(entries[0].key, "alpha");
        assert_eq!(entries[1].key, "beta");
        assert_eq!(entries[2].key, "gamma");
    }

    #[test]
    fn test_list_empty() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        let entries = repo.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_by_prefix() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("app.name", serde_json::json!("NevoFlux")).unwrap();
        repo.set("app.version", serde_json::json!("1.0.0")).unwrap();
        repo.set("app.debug", serde_json::json!(true)).unwrap();
        repo.set("db.host", serde_json::json!("localhost")).unwrap();
        repo.set("db.port", serde_json::json!(5432)).unwrap();

        let app_entries = repo.list_by_prefix("app.").unwrap();
        assert_eq!(app_entries.len(), 3);
        assert!(app_entries.iter().all(|e| e.key.starts_with("app.")));

        let db_entries = repo.list_by_prefix("db.").unwrap();
        assert_eq!(db_entries.len(), 2);
        assert!(db_entries.iter().all(|e| e.key.starts_with("db.")));
    }

    #[test]
    fn test_list_by_prefix_empty_result() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("app.name", serde_json::json!("NevoFlux")).unwrap();

        let entries = repo.list_by_prefix("nonexistent.").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_by_prefix_empty_prefix() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("alpha", serde_json::json!(1)).unwrap();
        repo.set("beta", serde_json::json!(2)).unwrap();

        // Empty prefix should match everything
        let entries = repo.list_by_prefix("").unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_exists() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        assert!(!repo.exists("test.key").unwrap());

        repo.set("test.key", serde_json::json!("value")).unwrap();
        assert!(repo.exists("test.key").unwrap());

        repo.delete("test.key").unwrap();
        assert!(!repo.exists("test.key").unwrap());
    }

    #[test]
    fn test_complex_json_value() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        let complex_value = serde_json::json!({
            "database": {
                "primary": {
                    "host": "db1.example.com",
                    "port": 5432,
                    "credentials": {
                        "username": "admin",
                        "ssl_enabled": true
                    }
                },
                "replicas": [
                    {"host": "db2.example.com", "port": 5432},
                    {"host": "db3.example.com", "port": 5432}
                ]
            },
            "features": ["feature_a", "feature_b", "feature_c"],
            "limits": {
                "max_connections": 100,
                "timeout_ms": 5000,
                "retry_count": 3
            },
            "optional_field": null
        });

        repo.set("infrastructure.config", complex_value.clone())
            .unwrap();

        let retrieved = repo.get("infrastructure.config").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), complex_value);
    }

    #[test]
    fn test_overwrite_updates_timestamp() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("test.key", serde_json::json!("v1")).unwrap();

        let entries = repo.list().unwrap();
        let first_timestamp = entries[0].updated_at;

        // Wait a bit and update
        std::thread::sleep(std::time::Duration::from_secs(1));

        repo.set("test.key", serde_json::json!("v2")).unwrap();

        let entries = repo.list().unwrap();
        let second_timestamp = entries[0].updated_at;

        assert!(second_timestamp > first_timestamp);
    }

    #[test]
    fn test_set_and_get_various_types() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        // Integer
        repo.set("int", serde_json::json!(42)).unwrap();
        assert_eq!(repo.get("int").unwrap(), Some(serde_json::json!(42)));

        // Float
        repo.set("float", serde_json::json!(1.23456)).unwrap();
        assert_eq!(repo.get("float").unwrap(), Some(serde_json::json!(1.23456)));

        // Negative number
        repo.set("negative", serde_json::json!(-100)).unwrap();
        assert_eq!(repo.get("negative").unwrap(), Some(serde_json::json!(-100)));

        // Empty string
        repo.set("empty_string", serde_json::json!("")).unwrap();
        assert_eq!(
            repo.get("empty_string").unwrap(),
            Some(serde_json::json!(""))
        );

        // Empty array
        repo.set("empty_array", serde_json::json!([])).unwrap();
        assert_eq!(
            repo.get("empty_array").unwrap(),
            Some(serde_json::json!([]))
        );

        // Empty object
        repo.set("empty_object", serde_json::json!({})).unwrap();
        assert_eq!(
            repo.get("empty_object").unwrap(),
            Some(serde_json::json!({}))
        );
    }

    #[test]
    fn test_list_returns_entries_with_values() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        repo.set("key1", serde_json::json!("value1")).unwrap();
        repo.set("key2", serde_json::json!(42)).unwrap();

        let entries = repo.list().unwrap();
        assert_eq!(entries.len(), 2);

        let key1_entry = entries.iter().find(|e| e.key == "key1").unwrap();
        assert_eq!(key1_entry.value, serde_json::json!("value1"));

        let key2_entry = entries.iter().find(|e| e.key == "key2").unwrap();
        assert_eq!(key2_entry.value, serde_json::json!(42));
    }

    #[test]
    fn test_keys_with_special_characters() {
        let db = setup_db();
        let repo = ConfigRepository::new(&db);

        // Key with dots
        repo.set("a.b.c.d", serde_json::json!("dots")).unwrap();
        assert!(repo.exists("a.b.c.d").unwrap());

        // Key with dashes
        repo.set("my-key-name", serde_json::json!("dashes"))
            .unwrap();
        assert!(repo.exists("my-key-name").unwrap());

        // Key with underscores
        repo.set("my_key_name", serde_json::json!("underscores"))
            .unwrap();
        assert!(repo.exists("my_key_name").unwrap());

        // Key with colon
        repo.set("namespace:key", serde_json::json!("colon"))
            .unwrap();
        assert!(repo.exists("namespace:key").unwrap());
    }
}
