//! Config model and related types.

use serde::{Deserialize, Serialize};

/// A configuration entry with a key-value pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// The unique key for this configuration entry.
    pub key: String,
    /// The JSON value stored for this key.
    pub value: serde_json::Value,
    /// Unix timestamp when the entry was last updated.
    pub updated_at: i64,
}

impl ConfigEntry {
    /// Create a new config entry.
    pub fn new(key: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            key: key.into(),
            value,
            updated_at: current_timestamp(),
        }
    }
}

/// Get the current Unix timestamp.
fn current_timestamp() -> i64 {
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
    fn test_config_entry_new() {
        let entry = ConfigEntry::new("test.key", serde_json::json!("test_value"));

        assert_eq!(entry.key, "test.key");
        assert_eq!(entry.value, serde_json::json!("test_value"));
        assert!(entry.updated_at > 0);
    }

    #[test]
    fn test_config_entry_with_string_value() {
        let entry = ConfigEntry::new("app.name", serde_json::json!("NevoFlux"));

        assert_eq!(entry.key, "app.name");
        assert_eq!(entry.value, serde_json::json!("NevoFlux"));
    }

    #[test]
    fn test_config_entry_with_number_value() {
        let entry = ConfigEntry::new("app.port", serde_json::json!(8080));

        assert_eq!(entry.key, "app.port");
        assert_eq!(entry.value, serde_json::json!(8080));
    }

    #[test]
    fn test_config_entry_with_boolean_value() {
        let entry = ConfigEntry::new("app.debug", serde_json::json!(true));

        assert_eq!(entry.key, "app.debug");
        assert_eq!(entry.value, serde_json::json!(true));
    }

    #[test]
    fn test_config_entry_with_array_value() {
        let entry = ConfigEntry::new("app.features", serde_json::json!(["feature1", "feature2"]));

        assert_eq!(entry.key, "app.features");
        assert_eq!(entry.value, serde_json::json!(["feature1", "feature2"]));
    }

    #[test]
    fn test_config_entry_with_object_value() {
        let entry = ConfigEntry::new(
            "app.settings",
            serde_json::json!({
                "theme": "dark",
                "language": "en"
            }),
        );

        assert_eq!(entry.key, "app.settings");
        assert!(entry.value.is_object());
        assert_eq!(entry.value["theme"], "dark");
        assert_eq!(entry.value["language"], "en");
    }

    #[test]
    fn test_config_entry_with_null_value() {
        let entry = ConfigEntry::new("app.optional", serde_json::Value::Null);

        assert_eq!(entry.key, "app.optional");
        assert!(entry.value.is_null());
    }

    #[test]
    fn test_config_entry_serialization() {
        let entry = ConfigEntry {
            key: "test.key".to_string(),
            value: serde_json::json!("test_value"),
            updated_at: 1234567890,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ConfigEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, entry.key);
        assert_eq!(deserialized.value, entry.value);
        assert_eq!(deserialized.updated_at, entry.updated_at);
    }

    #[test]
    fn test_config_entry_serialization_complex_value() {
        let complex_value = serde_json::json!({
            "nested": {
                "array": [1, 2, 3],
                "string": "hello",
                "number": 42.5,
                "boolean": true,
                "null_field": null
            }
        });

        let entry = ConfigEntry {
            key: "complex.config".to_string(),
            value: complex_value.clone(),
            updated_at: 1234567890,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ConfigEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.value, complex_value);
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable Unix timestamp (after year 2020)
        assert!(ts > 1577836800);
    }
}
