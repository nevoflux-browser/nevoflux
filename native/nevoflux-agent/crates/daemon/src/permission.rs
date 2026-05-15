//! Permission enforcement for NevoFlux Agent
//!
//! This module provides permission checking and enforcement for agent operations,
//! including sensitive path detection and session-scoped permissions.

use std::collections::HashSet;
use std::str::FromStr;

/// Sensitive paths that are always denied access
const SENSITIVE_PATHS: &[&str] = &[
    "/.ssh/",
    "/.gnupg/",
    "/.aws/credentials",
    "/.config/gcloud/",
    "_history",
    "/etc/passwd",
    "/etc/shadow",
];

/// Types of resources that can be accessed
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// File system resources
    File,
    /// Script execution
    Script,
    /// Network connections
    Network,
    /// MCP server connections
    Mcp,
    /// Plugin resources
    Plugin,
}

impl ResourceType {
    /// Convert resource type to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Script => "script",
            Self::Network => "network",
            Self::Mcp => "mcp",
            Self::Plugin => "plugin",
        }
    }
}

impl FromStr for ResourceType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "file" => Ok(Self::File),
            "script" => Ok(Self::Script),
            "network" => Ok(Self::Network),
            "mcp" => Ok(Self::Mcp),
            "plugin" => Ok(Self::Plugin),
            _ => Err(()),
        }
    }
}

/// Actions that can be performed on resources
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    /// Read access
    Read,
    /// Write access
    Write,
    /// Execute access
    Execute,
    /// Network connection
    Connect,
}

impl Action {
    /// Convert action to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Execute => "execute",
            Self::Connect => "connect",
        }
    }
}

impl FromStr for Action {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "execute" => Ok(Self::Execute),
            "connect" => Ok(Self::Connect),
            _ => Err(()),
        }
    }
}

/// Result of a permission check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    /// Access is allowed
    Allowed,
    /// Access is denied with a reason
    Denied(&'static str),
    /// Need to check persistent storage for permission
    NeedsCheck,
}

/// Permission enforcer that manages session permissions and checks access
#[derive(Debug, Default)]
pub struct PermissionEnforcer {
    /// Session-scoped permissions: (resource_type, action, resource)
    session_permissions: HashSet<(String, String, String)>,
}

impl PermissionEnforcer {
    /// Create a new permission enforcer
    pub fn new() -> Self {
        Self {
            session_permissions: HashSet::new(),
        }
    }

    /// Check if a path matches any sensitive path pattern
    pub fn is_sensitive_path(&self, path: &str) -> bool {
        SENSITIVE_PATHS
            .iter()
            .any(|sensitive| path.contains(sensitive))
    }

    /// Grant a session-scoped permission
    pub fn grant_session(&mut self, resource_type: &str, action: &str, resource: &str) {
        self.session_permissions.insert((
            resource_type.to_lowercase(),
            action.to_lowercase(),
            resource.to_string(),
        ));
    }

    /// Check if a session permission exists
    pub fn has_session_permission(
        &self,
        resource_type: &str,
        action: &str,
        resource: &str,
    ) -> bool {
        self.session_permissions.contains(&(
            resource_type.to_lowercase(),
            action.to_lowercase(),
            resource.to_string(),
        ))
    }

    /// Check if an action on a resource is allowed
    ///
    /// Returns:
    /// - `Allowed` if the action is permitted by session permissions
    /// - `Denied` if the resource is sensitive (for file resources)
    /// - `NeedsCheck` if persistent storage should be checked
    pub fn check(&self, resource_type: &str, action: &str, resource: &str) -> PermissionResult {
        // Check for sensitive paths (only for file resources)
        if resource_type.to_lowercase() == "file" && self.is_sensitive_path(resource) {
            return PermissionResult::Denied("access to sensitive path is forbidden");
        }

        // Check session permissions
        if self.has_session_permission(resource_type, action, resource) {
            return PermissionResult::Allowed;
        }

        // Need to check persistent storage
        PermissionResult::NeedsCheck
    }

    /// Clear all session permissions
    pub fn clear_session(&mut self) {
        self.session_permissions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_from_str() {
        assert_eq!("file".parse::<ResourceType>(), Ok(ResourceType::File));
        assert_eq!("FILE".parse::<ResourceType>(), Ok(ResourceType::File));
        assert_eq!("script".parse::<ResourceType>(), Ok(ResourceType::Script));
        assert_eq!("network".parse::<ResourceType>(), Ok(ResourceType::Network));
        assert_eq!("mcp".parse::<ResourceType>(), Ok(ResourceType::Mcp));
        assert_eq!("plugin".parse::<ResourceType>(), Ok(ResourceType::Plugin));
        assert!("unknown".parse::<ResourceType>().is_err());
        assert!("".parse::<ResourceType>().is_err());
    }

    #[test]
    fn test_action_from_str() {
        assert_eq!("read".parse::<Action>(), Ok(Action::Read));
        assert_eq!("READ".parse::<Action>(), Ok(Action::Read));
        assert_eq!("write".parse::<Action>(), Ok(Action::Write));
        assert_eq!("execute".parse::<Action>(), Ok(Action::Execute));
        assert_eq!("connect".parse::<Action>(), Ok(Action::Connect));
        assert!("unknown".parse::<Action>().is_err());
        assert!("".parse::<Action>().is_err());
    }

    #[test]
    fn test_sensitive_path_detection() {
        let enforcer = PermissionEnforcer::new();

        // SSH paths
        assert!(enforcer.is_sensitive_path("/home/user/.ssh/id_rsa"));
        assert!(enforcer.is_sensitive_path("/home/user/.ssh/config"));

        // GPG paths
        assert!(enforcer.is_sensitive_path("/home/user/.gnupg/private-keys-v1.d"));

        // AWS credentials
        assert!(enforcer.is_sensitive_path("/home/user/.aws/credentials"));

        // GCloud config
        assert!(enforcer.is_sensitive_path("/home/user/.config/gcloud/credentials.json"));

        // History files
        assert!(enforcer.is_sensitive_path("/home/user/.bash_history"));
        assert!(enforcer.is_sensitive_path("/home/user/.zsh_history"));

        // System files
        assert!(enforcer.is_sensitive_path("/etc/passwd"));
        assert!(enforcer.is_sensitive_path("/etc/shadow"));

        // Non-sensitive paths
        assert!(!enforcer.is_sensitive_path("/home/user/documents/file.txt"));
        assert!(!enforcer.is_sensitive_path("/tmp/test.txt"));
        assert!(!enforcer.is_sensitive_path("/home/user/.config/app/config.json"));
    }

    #[test]
    fn test_sensitive_path_always_denied() {
        let enforcer = PermissionEnforcer::new();

        // Sensitive file paths should always be denied
        let result = enforcer.check("file", "read", "/home/user/.ssh/id_rsa");
        assert_eq!(
            result,
            PermissionResult::Denied("access to sensitive path is forbidden")
        );

        let result = enforcer.check("file", "write", "/home/user/.aws/credentials");
        assert_eq!(
            result,
            PermissionResult::Denied("access to sensitive path is forbidden")
        );

        // Even with session permission, sensitive paths should be denied
        let mut enforcer = PermissionEnforcer::new();
        enforcer.grant_session("file", "read", "/home/user/.ssh/id_rsa");
        let result = enforcer.check("file", "read", "/home/user/.ssh/id_rsa");
        assert_eq!(
            result,
            PermissionResult::Denied("access to sensitive path is forbidden")
        );
    }

    #[test]
    fn test_session_permission() {
        let mut enforcer = PermissionEnforcer::new();

        // No permission initially
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/test.txt"));

        // Grant permission
        enforcer.grant_session("file", "read", "/tmp/test.txt");
        assert!(enforcer.has_session_permission("file", "read", "/tmp/test.txt"));

        // Check returns Allowed
        let result = enforcer.check("file", "read", "/tmp/test.txt");
        assert_eq!(result, PermissionResult::Allowed);

        // Different action is not permitted
        assert!(!enforcer.has_session_permission("file", "write", "/tmp/test.txt"));
        let result = enforcer.check("file", "write", "/tmp/test.txt");
        assert_eq!(result, PermissionResult::NeedsCheck);

        // Clear session
        enforcer.clear_session();
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/test.txt"));
    }

    #[test]
    fn test_session_permission_specificity() {
        let mut enforcer = PermissionEnforcer::new();

        // Grant permission for specific file
        enforcer.grant_session("file", "read", "/tmp/specific.txt");

        // Only that specific file is allowed
        assert!(enforcer.has_session_permission("file", "read", "/tmp/specific.txt"));
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/other.txt"));
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/specific"));
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/specific.txt.bak"));

        // Case insensitive for resource type and action
        enforcer.grant_session("FILE", "READ", "/tmp/case.txt");
        assert!(enforcer.has_session_permission("file", "read", "/tmp/case.txt"));
        assert!(enforcer.has_session_permission("File", "Read", "/tmp/case.txt"));

        // Resource path is case sensitive
        enforcer.grant_session("file", "read", "/tmp/CaseSensitive.txt");
        assert!(enforcer.has_session_permission("file", "read", "/tmp/CaseSensitive.txt"));
        assert!(!enforcer.has_session_permission("file", "read", "/tmp/casesensitive.txt"));
    }

    #[test]
    fn test_non_file_resource_not_checked_for_sensitive_paths() {
        let enforcer = PermissionEnforcer::new();

        // Network resources don't check sensitive paths
        let result = enforcer.check("network", "connect", "/.ssh/");
        assert_eq!(result, PermissionResult::NeedsCheck);

        // Script resources don't check sensitive paths
        let result = enforcer.check("script", "execute", "/.gnupg/script.sh");
        assert_eq!(result, PermissionResult::NeedsCheck);
    }

    #[test]
    fn test_resource_type_as_str() {
        assert_eq!(ResourceType::File.as_str(), "file");
        assert_eq!(ResourceType::Script.as_str(), "script");
        assert_eq!(ResourceType::Network.as_str(), "network");
        assert_eq!(ResourceType::Mcp.as_str(), "mcp");
        assert_eq!(ResourceType::Plugin.as_str(), "plugin");
    }

    #[test]
    fn test_action_as_str() {
        assert_eq!(Action::Read.as_str(), "read");
        assert_eq!(Action::Write.as_str(), "write");
        assert_eq!(Action::Execute.as_str(), "execute");
        assert_eq!(Action::Connect.as_str(), "connect");
    }
}
