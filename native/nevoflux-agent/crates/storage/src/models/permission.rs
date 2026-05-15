//! Permission model and related types.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Error returned when parsing a permission scope from string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePermissionScopeError;

impl std::fmt::Display for ParsePermissionScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid permission scope")
    }
}

impl std::error::Error for ParsePermissionScopeError {}

/// The scope of a permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    /// Permission applies to a specific session only.
    #[default]
    Session,
    /// Permission applies globally across all sessions.
    Global,
    /// Permission applies only once and should be used immediately.
    Once,
}

impl PermissionScope {
    /// Convert the scope to a string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionScope::Session => "session",
            PermissionScope::Global => "global",
            PermissionScope::Once => "once",
        }
    }
}

impl FromStr for PermissionScope {
    type Err = ParsePermissionScopeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "session" => Ok(PermissionScope::Session),
            "global" => Ok(PermissionScope::Global),
            "once" => Ok(PermissionScope::Once),
            _ => Err(ParsePermissionScopeError),
        }
    }
}

impl std::fmt::Display for PermissionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A permission record for controlling access to resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Unique identifier for the permission.
    pub id: String,
    /// The type of resource (e.g., "file", "tool", "api").
    pub resource_type: String,
    /// The action being permitted (e.g., "read", "write", "execute").
    pub action: String,
    /// Pattern matching the resource (e.g., "/path/*", specific path, or "*" for wildcard).
    pub resource_pattern: String,
    /// The scope of the permission.
    pub scope: PermissionScope,
    /// Whether the permission grants or denies access.
    pub granted: bool,
    /// The session ID this permission is scoped to (for session-scoped permissions).
    pub session_id: Option<String>,
    /// Unix timestamp when the permission was created.
    pub created_at: i64,
    /// Unix timestamp when the permission expires (None = never expires).
    pub expires_at: Option<i64>,
}

/// Parameters for creating a new permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePermissionParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// The type of resource.
    pub resource_type: String,
    /// The action being permitted.
    pub action: String,
    /// Pattern matching the resource.
    pub resource_pattern: String,
    /// The scope of the permission.
    pub scope: PermissionScope,
    /// Whether the permission grants or denies access.
    pub granted: bool,
    /// The session ID this permission is scoped to.
    pub session_id: Option<String>,
    /// Unix timestamp when the permission expires.
    pub expires_at: Option<i64>,
}

impl CreatePermissionParams {
    /// Create new params with required fields.
    pub fn new(
        resource_type: impl Into<String>,
        action: impl Into<String>,
        resource_pattern: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            resource_type: resource_type.into(),
            action: action.into(),
            resource_pattern: resource_pattern.into(),
            scope: PermissionScope::default(),
            granted: true,
            session_id: None,
            expires_at: None,
        }
    }

    /// Set the permission ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the scope.
    pub fn with_scope(mut self, scope: PermissionScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set whether the permission is granted.
    pub fn with_granted(mut self, granted: bool) -> Self {
        self.granted = granted;
        self
    }

    /// Set the session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the expiration time.
    pub fn with_expires_at(mut self, expires_at: i64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

/// Parameters for checking a permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckPermissionParams {
    /// The type of resource being checked.
    pub resource_type: String,
    /// The action being checked.
    pub action: String,
    /// The specific resource being accessed.
    pub resource: String,
    /// The session ID to check permissions for.
    pub session_id: Option<String>,
}

impl CheckPermissionParams {
    /// Create new check params with required fields.
    pub fn new(
        resource_type: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            resource_type: resource_type.into(),
            action: action.into(),
            resource: resource.into(),
            session_id: None,
        }
    }

    /// Set the session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// Generate a simple UUID v4-like identifier for permissions.
#[cfg(test)]
fn perm_uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Format: perm-{timestamp_hex}-{random_hex}
    let random_part: u64 = (timestamp as u64).wrapping_mul(6364136223846793005);
    format!("perm-{:016x}-{:08x}", timestamp as u64, random_part as u32)
}

/// Get the current Unix timestamp.
#[cfg(test)]
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
    fn test_permission_scope_as_str() {
        assert_eq!(PermissionScope::Session.as_str(), "session");
        assert_eq!(PermissionScope::Global.as_str(), "global");
        assert_eq!(PermissionScope::Once.as_str(), "once");
    }

    #[test]
    fn test_permission_scope_from_str() {
        assert_eq!(
            "session".parse::<PermissionScope>(),
            Ok(PermissionScope::Session)
        );
        assert_eq!(
            "SESSION".parse::<PermissionScope>(),
            Ok(PermissionScope::Session)
        );
        assert_eq!(
            "global".parse::<PermissionScope>(),
            Ok(PermissionScope::Global)
        );
        assert_eq!(
            "Global".parse::<PermissionScope>(),
            Ok(PermissionScope::Global)
        );
        assert_eq!("once".parse::<PermissionScope>(), Ok(PermissionScope::Once));
        assert_eq!("ONCE".parse::<PermissionScope>(), Ok(PermissionScope::Once));
        assert!("invalid".parse::<PermissionScope>().is_err());
    }

    #[test]
    fn test_permission_scope_default() {
        assert_eq!(PermissionScope::default(), PermissionScope::Session);
    }

    #[test]
    fn test_permission_scope_display() {
        assert_eq!(format!("{}", PermissionScope::Session), "session");
        assert_eq!(format!("{}", PermissionScope::Global), "global");
        assert_eq!(format!("{}", PermissionScope::Once), "once");
    }

    #[test]
    fn test_permission_scope_serialization() {
        let session = PermissionScope::Session;
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(json, "\"session\"");

        let global = PermissionScope::Global;
        let json = serde_json::to_string(&global).unwrap();
        assert_eq!(json, "\"global\"");

        let once = PermissionScope::Once;
        let json = serde_json::to_string(&once).unwrap();
        assert_eq!(json, "\"once\"");
    }

    #[test]
    fn test_permission_scope_deserialization() {
        let session: PermissionScope = serde_json::from_str("\"session\"").unwrap();
        assert_eq!(session, PermissionScope::Session);

        let global: PermissionScope = serde_json::from_str("\"global\"").unwrap();
        assert_eq!(global, PermissionScope::Global);

        let once: PermissionScope = serde_json::from_str("\"once\"").unwrap();
        assert_eq!(once, PermissionScope::Once);
    }

    #[test]
    fn test_create_permission_params_new() {
        let params = CreatePermissionParams::new("file", "read", "/home/user/*");

        assert!(params.id.is_none());
        assert_eq!(params.resource_type, "file");
        assert_eq!(params.action, "read");
        assert_eq!(params.resource_pattern, "/home/user/*");
        assert_eq!(params.scope, PermissionScope::Session);
        assert!(params.granted);
        assert!(params.session_id.is_none());
        assert!(params.expires_at.is_none());
    }

    #[test]
    fn test_create_permission_params_builder() {
        let params = CreatePermissionParams::new("tool", "execute", "bash")
            .with_id("perm-123")
            .with_scope(PermissionScope::Global)
            .with_granted(false)
            .with_session_id("sess-456")
            .with_expires_at(1700000000);

        assert_eq!(params.id, Some("perm-123".to_string()));
        assert_eq!(params.resource_type, "tool");
        assert_eq!(params.action, "execute");
        assert_eq!(params.resource_pattern, "bash");
        assert_eq!(params.scope, PermissionScope::Global);
        assert!(!params.granted);
        assert_eq!(params.session_id, Some("sess-456".to_string()));
        assert_eq!(params.expires_at, Some(1700000000));
    }

    #[test]
    fn test_check_permission_params_new() {
        let params = CheckPermissionParams::new("file", "write", "/tmp/test.txt");

        assert_eq!(params.resource_type, "file");
        assert_eq!(params.action, "write");
        assert_eq!(params.resource, "/tmp/test.txt");
        assert!(params.session_id.is_none());
    }

    #[test]
    fn test_check_permission_params_with_session() {
        let params = CheckPermissionParams::new("api", "call", "https://api.example.com/endpoint")
            .with_session_id("sess-789");

        assert_eq!(params.resource_type, "api");
        assert_eq!(params.action, "call");
        assert_eq!(params.resource, "https://api.example.com/endpoint");
        assert_eq!(params.session_id, Some("sess-789".to_string()));
    }

    #[test]
    fn test_perm_uuid_v4_uniqueness() {
        let id1 = perm_uuid_v4();
        let id2 = perm_uuid_v4();
        // IDs should be different (with high probability)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_perm_uuid_v4_format() {
        let id = perm_uuid_v4();
        assert!(id.starts_with("perm-"));
        assert!(id.len() > 20);
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable Unix timestamp (after year 2020)
        assert!(ts > 1577836800);
    }

    #[test]
    fn test_permission_serialization() {
        let permission = Permission {
            id: "perm-123".to_string(),
            resource_type: "file".to_string(),
            action: "read".to_string(),
            resource_pattern: "/home/*".to_string(),
            scope: PermissionScope::Session,
            granted: true,
            session_id: Some("sess-456".to_string()),
            created_at: 1234567890,
            expires_at: Some(1234567900),
        };

        let json = serde_json::to_string(&permission).unwrap();
        let deserialized: Permission = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, permission.id);
        assert_eq!(deserialized.resource_type, permission.resource_type);
        assert_eq!(deserialized.action, permission.action);
        assert_eq!(deserialized.resource_pattern, permission.resource_pattern);
        assert_eq!(deserialized.scope, permission.scope);
        assert_eq!(deserialized.granted, permission.granted);
        assert_eq!(deserialized.session_id, permission.session_id);
        assert_eq!(deserialized.created_at, permission.created_at);
        assert_eq!(deserialized.expires_at, permission.expires_at);
    }

    #[test]
    fn test_permission_without_optional_fields() {
        let permission = Permission {
            id: "perm-123".to_string(),
            resource_type: "tool".to_string(),
            action: "execute".to_string(),
            resource_pattern: "*".to_string(),
            scope: PermissionScope::Global,
            granted: true,
            session_id: None,
            created_at: 1234567890,
            expires_at: None,
        };

        let json = serde_json::to_string(&permission).unwrap();
        let deserialized: Permission = serde_json::from_str(&json).unwrap();

        assert!(deserialized.session_id.is_none());
        assert!(deserialized.expires_at.is_none());
    }

    #[test]
    fn test_parse_permission_scope_error_display() {
        let err = ParsePermissionScopeError;
        assert_eq!(format!("{}", err), "invalid permission scope");
    }
}
