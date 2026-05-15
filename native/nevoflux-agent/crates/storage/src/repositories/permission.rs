//! Permission repository for database operations.

use rusqlite::{params, OptionalExtension, Row};

use crate::connection::Database;
use crate::error::Result;
use crate::models::{CheckPermissionParams, CreatePermissionParams, Permission};

/// Generate a simple UUID v4-like identifier for permissions.
fn uuid_v4() -> String {
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
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Repository for permission CRUD operations.
pub struct PermissionRepository<'a> {
    db: &'a Database,
}

impl<'a> PermissionRepository<'a> {
    /// Create a new permission repository.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Create a new permission.
    pub fn create(&self, params: CreatePermissionParams) -> Result<Permission> {
        let id = params.id.unwrap_or_else(uuid_v4);
        let now = current_timestamp();

        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO permissions (id, resource_type, action, resource_pattern, scope, granted, session_id, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    params.resource_type,
                    params.action,
                    params.resource_pattern,
                    params.scope.as_str(),
                    params.granted as i32,
                    params.session_id,
                    now,
                    params.expires_at,
                ],
            )?;

            Ok(Permission {
                id,
                resource_type: params.resource_type,
                action: params.action,
                resource_pattern: params.resource_pattern,
                scope: params.scope,
                granted: params.granted,
                session_id: params.session_id,
                created_at: now,
                expires_at: params.expires_at,
            })
        })
    }

    /// Get a permission by ID.
    pub fn get(&self, id: &str) -> Result<Option<Permission>> {
        self.db.with_connection(|conn| {
            let result = conn
                .query_row(
                    "SELECT id, resource_type, action, resource_pattern, scope, granted, session_id, created_at, expires_at
                     FROM permissions WHERE id = ?1",
                    params![id],
                    row_to_permission,
                )
                .optional()?;

            Ok(result)
        })
    }

    /// Delete a permission by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_connection(|conn| {
            let rows_affected =
                conn.execute("DELETE FROM permissions WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        })
    }

    /// Check if a permission is granted, denied, or not found.
    ///
    /// Returns:
    /// - `Some(true)` if permission is granted
    /// - `Some(false)` if permission is denied
    /// - `None` if no matching permission is found
    ///
    /// Logic:
    /// - Prefers exact resource_pattern match over wildcard '*'
    /// - Prefers session-scoped permissions over global
    /// - Ignores expired permissions
    /// - Session-scoped permissions require matching session_id
    pub fn check(&self, params: CheckPermissionParams) -> Result<Option<bool>> {
        let now = current_timestamp();

        self.db.with_connection(|conn| {
            // Build query to find all matching permissions
            // Order by: exact match first, then session scope before global
            let mut sql = String::from(
                "SELECT id, resource_type, action, resource_pattern, scope, granted, session_id, created_at, expires_at
                 FROM permissions
                 WHERE resource_type = ?1
                   AND action = ?2
                   AND (resource_pattern = ?3 OR resource_pattern = '*')
                   AND (expires_at IS NULL OR expires_at > ?4)",
            );

            let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(params.resource_type.clone()),
                Box::new(params.action.clone()),
                Box::new(params.resource.clone()),
                Box::new(now),
            ];

            // Handle session scope requirements
            if let Some(ref session_id) = params.session_id {
                // Session-scoped permissions require matching session_id
                // Global permissions apply regardless of session_id
                sql.push_str(" AND (scope = 'global' OR (scope IN ('session', 'once') AND session_id = ?5))");
                values.push(Box::new(session_id.clone()));
            } else {
                // Without session_id, only global permissions apply
                sql.push_str(" AND scope = 'global'");
            }

            // Order by: exact match first, session scope first, most recent first
            sql.push_str(
                " ORDER BY
                    CASE WHEN resource_pattern = ?3 THEN 0 ELSE 1 END,
                    CASE WHEN scope = 'session' THEN 0 WHEN scope = 'once' THEN 1 ELSE 2 END,
                    created_at DESC
                 LIMIT 1",
            );

            // Re-add resource for ORDER BY (need to match parameter count)
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let permission = stmt
                .query_row(params_refs.as_slice(), row_to_permission)
                .optional()?;

            match permission {
                Some(perm) => Ok(Some(perm.granted)),
                None => Ok(None),
            }
        })
    }

    /// List all permissions for a session (including global permissions).
    pub fn list_by_session(&self, session_id: &str) -> Result<Vec<Permission>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, resource_type, action, resource_pattern, scope, granted, session_id, created_at, expires_at
                 FROM permissions
                 WHERE scope = 'global' OR session_id = ?1
                 ORDER BY created_at DESC",
            )?;

            let permissions = stmt
                .query_map(params![session_id], row_to_permission)?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(permissions)
        })
    }

    /// Delete all session-scoped permissions for a session.
    /// Returns the number of deleted permissions.
    pub fn delete_by_session(&self, session_id: &str) -> Result<u32> {
        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "DELETE FROM permissions WHERE session_id = ?1 AND scope IN ('session', 'once')",
                params![session_id],
            )?;
            Ok(rows_affected as u32)
        })
    }

    /// Delete all expired permissions.
    /// Returns the number of deleted permissions.
    pub fn delete_expired(&self) -> Result<u32> {
        let now = current_timestamp();

        self.db.with_connection(|conn| {
            let rows_affected = conn.execute(
                "DELETE FROM permissions WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                params![now],
            )?;
            Ok(rows_affected as u32)
        })
    }

    /// Revoke (delete) specific permissions by resource type, action, pattern, and optional session.
    /// Returns the number of revoked permissions.
    pub fn revoke(
        &self,
        resource_type: &str,
        action: &str,
        resource_pattern: &str,
        session_id: Option<&str>,
    ) -> Result<u32> {
        self.db.with_connection(|conn| {
            let rows_affected = if let Some(sid) = session_id {
                conn.execute(
                    "DELETE FROM permissions
                     WHERE resource_type = ?1 AND action = ?2 AND resource_pattern = ?3 AND session_id = ?4",
                    params![resource_type, action, resource_pattern, sid],
                )?
            } else {
                conn.execute(
                    "DELETE FROM permissions
                     WHERE resource_type = ?1 AND action = ?2 AND resource_pattern = ?3 AND session_id IS NULL",
                    params![resource_type, action, resource_pattern],
                )?
            };
            Ok(rows_affected as u32)
        })
    }
}

/// Convert a database row to a Permission.
fn row_to_permission(row: &Row<'_>) -> rusqlite::Result<Permission> {
    let id: String = row.get(0)?;
    let resource_type: String = row.get(1)?;
    let action: String = row.get(2)?;
    let resource_pattern: String = row.get(3)?;
    let scope_str: String = row.get(4)?;
    let granted: i32 = row.get(5)?;
    let session_id: Option<String> = row.get(6)?;
    let created_at: i64 = row.get(7)?;
    let expires_at: Option<i64> = row.get(8)?;

    let scope = scope_str.parse().unwrap_or_default();

    Ok(Permission {
        id,
        resource_type,
        action,
        resource_pattern,
        scope,
        granted: granted != 0,
        session_id,
        created_at,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PermissionScope;

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_create_and_get() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let params = CreatePermissionParams::new("file", "read", "/home/user/*");
        let created = repo.create(params).unwrap();

        assert!(!created.id.is_empty());
        assert_eq!(created.resource_type, "file");
        assert_eq!(created.action, "read");
        assert_eq!(created.resource_pattern, "/home/user/*");
        assert_eq!(created.scope, PermissionScope::Session);
        assert!(created.granted);
        assert!(created.session_id.is_none());
        assert!(created.created_at > 0);
        assert!(created.expires_at.is_none());

        let fetched = repo.get(&created.id).unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.resource_type, created.resource_type);
        assert_eq!(fetched.action, created.action);
    }

    #[test]
    fn test_create_with_all_params() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let params = CreatePermissionParams::new("tool", "execute", "bash")
            .with_id("perm-custom-123")
            .with_scope(PermissionScope::Global)
            .with_granted(false)
            .with_session_id("sess-456")
            .with_expires_at(1800000000);

        let created = repo.create(params).unwrap();

        assert_eq!(created.id, "perm-custom-123");
        assert_eq!(created.resource_type, "tool");
        assert_eq!(created.action, "execute");
        assert_eq!(created.resource_pattern, "bash");
        assert_eq!(created.scope, PermissionScope::Global);
        assert!(!created.granted);
        assert_eq!(created.session_id, Some("sess-456".to_string()));
        assert_eq!(created.expires_at, Some(1800000000));
    }

    #[test]
    fn test_get_not_found() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let params = CreatePermissionParams::new("file", "write", "/tmp/*");
        let created = repo.create(params).unwrap();

        let deleted = repo.delete(&created.id).unwrap();
        assert!(deleted);

        let fetched = repo.get(&created.id).unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn test_delete_not_found() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let deleted = repo.delete("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_check_granted() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a global permission
        repo.create(
            CreatePermissionParams::new("file", "read", "/home/user/docs")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        let result = repo
            .check(CheckPermissionParams::new(
                "file",
                "read",
                "/home/user/docs",
            ))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_check_denied() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a denied global permission
        repo.create(
            CreatePermissionParams::new("file", "write", "/etc/passwd")
                .with_scope(PermissionScope::Global)
                .with_granted(false),
        )
        .unwrap();

        let result = repo
            .check(CheckPermissionParams::new("file", "write", "/etc/passwd"))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_check_not_found() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let result = repo
            .check(CheckPermissionParams::new("file", "read", "/some/path"))
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_check_wildcard() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a wildcard permission
        repo.create(
            CreatePermissionParams::new("tool", "execute", "*")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        let result = repo
            .check(CheckPermissionParams::new("tool", "execute", "any_tool"))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_check_exact_match_preferred_over_wildcard() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a wildcard permission that grants
        repo.create(
            CreatePermissionParams::new("file", "read", "*")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Create an exact match that denies
        repo.create(
            CreatePermissionParams::new("file", "read", "/sensitive/file")
                .with_scope(PermissionScope::Global)
                .with_granted(false),
        )
        .unwrap();

        // Exact match should take precedence
        let result = repo
            .check(CheckPermissionParams::new(
                "file",
                "read",
                "/sensitive/file",
            ))
            .unwrap();
        assert_eq!(result, Some(false));

        // Other files should still be granted via wildcard
        let result = repo
            .check(CheckPermissionParams::new("file", "read", "/other/file"))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_check_session_scoped() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a session-scoped permission
        repo.create(
            CreatePermissionParams::new("api", "call", "endpoint")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-123")
                .with_granted(true),
        )
        .unwrap();

        // Should find permission with matching session
        let result = repo
            .check(
                CheckPermissionParams::new("api", "call", "endpoint").with_session_id("sess-123"),
            )
            .unwrap();
        assert_eq!(result, Some(true));

        // Should not find permission with different session
        let result = repo
            .check(
                CheckPermissionParams::new("api", "call", "endpoint").with_session_id("sess-456"),
            )
            .unwrap();
        assert_eq!(result, None);

        // Should not find permission without session
        let result = repo
            .check(CheckPermissionParams::new("api", "call", "endpoint"))
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_check_session_preferred_over_global() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a global permission that grants
        repo.create(
            CreatePermissionParams::new("file", "read", "/shared")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Create a session-scoped permission that denies
        repo.create(
            CreatePermissionParams::new("file", "read", "/shared")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-123")
                .with_granted(false),
        )
        .unwrap();

        // Session scope should take precedence
        let result = repo
            .check(
                CheckPermissionParams::new("file", "read", "/shared").with_session_id("sess-123"),
            )
            .unwrap();
        assert_eq!(result, Some(false));

        // Different session should get global permission
        let result = repo
            .check(
                CheckPermissionParams::new("file", "read", "/shared").with_session_id("sess-456"),
            )
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_check_expired_permission() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create an expired permission
        repo.create(
            CreatePermissionParams::new("file", "read", "/temp")
                .with_scope(PermissionScope::Global)
                .with_granted(true)
                .with_expires_at(1), // Expired in 1970
        )
        .unwrap();

        // Should not find the expired permission
        let result = repo
            .check(CheckPermissionParams::new("file", "read", "/temp"))
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_check_non_expired_permission() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let future = current_timestamp() + 3600; // 1 hour from now

        repo.create(
            CreatePermissionParams::new("file", "read", "/temp")
                .with_scope(PermissionScope::Global)
                .with_granted(true)
                .with_expires_at(future),
        )
        .unwrap();

        // Should find the non-expired permission
        let result = repo
            .check(CheckPermissionParams::new("file", "read", "/temp"))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_list_by_session() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create global permission
        repo.create(
            CreatePermissionParams::new("tool", "execute", "*")
                .with_id("perm-global")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Create session-scoped permission for sess-1
        repo.create(
            CreatePermissionParams::new("file", "read", "/home")
                .with_id("perm-sess1")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-1")
                .with_granted(true),
        )
        .unwrap();

        // Create session-scoped permission for sess-2
        repo.create(
            CreatePermissionParams::new("file", "write", "/home")
                .with_id("perm-sess2")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-2")
                .with_granted(true),
        )
        .unwrap();

        // List for sess-1 should include global + sess-1 specific
        let perms = repo.list_by_session("sess-1").unwrap();
        assert_eq!(perms.len(), 2);
        let ids: Vec<&str> = perms.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"perm-global"));
        assert!(ids.contains(&"perm-sess1"));
        assert!(!ids.contains(&"perm-sess2"));
    }

    #[test]
    fn test_delete_by_session() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create global permission
        repo.create(
            CreatePermissionParams::new("tool", "execute", "*")
                .with_id("perm-global")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Create session-scoped permissions for sess-1
        repo.create(
            CreatePermissionParams::new("file", "read", "/home")
                .with_id("perm-sess1-a")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-1")
                .with_granted(true),
        )
        .unwrap();

        repo.create(
            CreatePermissionParams::new("file", "write", "/home")
                .with_id("perm-sess1-b")
                .with_scope(PermissionScope::Once)
                .with_session_id("sess-1")
                .with_granted(true),
        )
        .unwrap();

        let deleted = repo.delete_by_session("sess-1").unwrap();
        assert_eq!(deleted, 2);

        // Global permission should still exist
        assert!(repo.get("perm-global").unwrap().is_some());

        // Session permissions should be deleted
        assert!(repo.get("perm-sess1-a").unwrap().is_none());
        assert!(repo.get("perm-sess1-b").unwrap().is_none());
    }

    #[test]
    fn test_delete_expired() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create an expired permission
        repo.create(
            CreatePermissionParams::new("file", "read", "/old")
                .with_id("perm-expired")
                .with_scope(PermissionScope::Global)
                .with_granted(true)
                .with_expires_at(1),
        )
        .unwrap();

        // Create a non-expired permission
        let future = current_timestamp() + 3600;
        repo.create(
            CreatePermissionParams::new("file", "read", "/new")
                .with_id("perm-active")
                .with_scope(PermissionScope::Global)
                .with_granted(true)
                .with_expires_at(future),
        )
        .unwrap();

        // Create a permission without expiration
        repo.create(
            CreatePermissionParams::new("file", "read", "/permanent")
                .with_id("perm-no-expiry")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        let deleted = repo.delete_expired().unwrap();
        assert_eq!(deleted, 1);

        // Expired permission should be deleted
        assert!(repo.get("perm-expired").unwrap().is_none());

        // Active and permanent permissions should still exist
        assert!(repo.get("perm-active").unwrap().is_some());
        assert!(repo.get("perm-no-expiry").unwrap().is_some());
    }

    #[test]
    fn test_revoke() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create permissions
        repo.create(
            CreatePermissionParams::new("file", "read", "/home")
                .with_id("perm-1")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-1")
                .with_granted(true),
        )
        .unwrap();

        repo.create(
            CreatePermissionParams::new("file", "read", "/home")
                .with_id("perm-2")
                .with_scope(PermissionScope::Session)
                .with_session_id("sess-2")
                .with_granted(true),
        )
        .unwrap();

        // Revoke only for sess-1
        let revoked = repo
            .revoke("file", "read", "/home", Some("sess-1"))
            .unwrap();
        assert_eq!(revoked, 1);

        // perm-1 should be deleted
        assert!(repo.get("perm-1").unwrap().is_none());

        // perm-2 should still exist
        assert!(repo.get("perm-2").unwrap().is_some());
    }

    #[test]
    fn test_revoke_global() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a global permission (no session_id)
        repo.create(
            CreatePermissionParams::new("tool", "execute", "*")
                .with_id("perm-global")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Revoke global permission
        let revoked = repo.revoke("tool", "execute", "*", None).unwrap();
        assert_eq!(revoked, 1);

        assert!(repo.get("perm-global").unwrap().is_none());
    }

    #[test]
    fn test_revoke_no_match() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        let revoked = repo.revoke("file", "read", "/nonexistent", None).unwrap();
        assert_eq!(revoked, 0);
    }

    #[test]
    fn test_once_scope() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create a once-scoped permission
        repo.create(
            CreatePermissionParams::new("dangerous", "execute", "rm")
                .with_scope(PermissionScope::Once)
                .with_session_id("sess-123")
                .with_granted(true),
        )
        .unwrap();

        // Should be found with matching session
        let result = repo
            .check(
                CheckPermissionParams::new("dangerous", "execute", "rm")
                    .with_session_id("sess-123"),
            )
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_multiple_permissions_same_resource() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // Create read permission
        repo.create(
            CreatePermissionParams::new("file", "read", "/shared")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Create write permission (denied)
        repo.create(
            CreatePermissionParams::new("file", "write", "/shared")
                .with_scope(PermissionScope::Global)
                .with_granted(false),
        )
        .unwrap();

        // Read should be granted
        let result = repo
            .check(CheckPermissionParams::new("file", "read", "/shared"))
            .unwrap();
        assert_eq!(result, Some(true));

        // Write should be denied
        let result = repo
            .check(CheckPermissionParams::new("file", "write", "/shared"))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_different_resource_types() {
        let db = setup_db();
        let repo = PermissionRepository::new(&db);

        // File read permission
        repo.create(
            CreatePermissionParams::new("file", "read", "/path")
                .with_scope(PermissionScope::Global)
                .with_granted(true),
        )
        .unwrap();

        // Same path but different resource type should not match
        let result = repo
            .check(CheckPermissionParams::new("directory", "read", "/path"))
            .unwrap();
        assert_eq!(result, None);
    }
}
