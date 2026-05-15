//! Mock permission checker for testing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A mock permission checker for testing.
///
/// Allows pre-configuring permission decisions for testing different scenarios.
#[derive(Debug, Clone)]
pub struct MockPermissionChecker {
    decisions: Arc<Mutex<HashMap<String, bool>>>,
    default_decision: bool,
    check_count: Arc<Mutex<u32>>,
}

impl Default for MockPermissionChecker {
    fn default() -> Self {
        Self::allow_all()
    }
}

impl MockPermissionChecker {
    /// Create a permission checker that allows all requests.
    pub fn allow_all() -> Self {
        Self {
            decisions: Arc::new(Mutex::new(HashMap::new())),
            default_decision: true,
            check_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a permission checker that denies all requests.
    pub fn deny_all() -> Self {
        Self {
            decisions: Arc::new(Mutex::new(HashMap::new())),
            default_decision: false,
            check_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Configure a specific permission decision.
    ///
    /// The key format is "{resource_type}:{action}:{resource}".
    pub fn with_decision(
        self,
        resource_type: &str,
        action: &str,
        resource: &str,
        granted: bool,
    ) -> Self {
        let key = format!("{}:{}:{}", resource_type, action, resource);
        self.decisions.lock().unwrap().insert(key, granted);
        self
    }

    /// Check if an action is permitted.
    pub fn check(&self, resource_type: &str, action: &str, resource: &str) -> bool {
        *self.check_count.lock().unwrap() += 1;

        let key = format!("{}:{}:{}", resource_type, action, resource);
        self.decisions
            .lock()
            .unwrap()
            .get(&key)
            .copied()
            .unwrap_or(self.default_decision)
    }

    /// Get the number of times check was called.
    pub fn check_count(&self) -> u32 {
        *self.check_count.lock().unwrap()
    }

    /// Reset the check count.
    pub fn reset_count(&self) {
        *self.check_count.lock().unwrap() = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all() {
        let checker = MockPermissionChecker::allow_all();

        assert!(checker.check("file", "read", "/any/path"));
        assert!(checker.check("shell", "execute", "rm -rf"));
    }

    #[test]
    fn test_deny_all() {
        let checker = MockPermissionChecker::deny_all();

        assert!(!checker.check("file", "read", "/any/path"));
        assert!(!checker.check("shell", "execute", "ls"));
    }

    #[test]
    fn test_specific_decisions() {
        let checker = MockPermissionChecker::deny_all()
            .with_decision("file", "read", "/safe/path", true)
            .with_decision("file", "write", "/safe/path", false);

        assert!(checker.check("file", "read", "/safe/path"));
        assert!(!checker.check("file", "write", "/safe/path"));
        assert!(!checker.check("file", "read", "/other/path"));
    }

    #[test]
    fn test_check_count() {
        let checker = MockPermissionChecker::allow_all();

        assert_eq!(checker.check_count(), 0);
        checker.check("file", "read", "/path1");
        checker.check("file", "read", "/path2");
        assert_eq!(checker.check_count(), 2);

        checker.reset_count();
        assert_eq!(checker.check_count(), 0);
    }
}
