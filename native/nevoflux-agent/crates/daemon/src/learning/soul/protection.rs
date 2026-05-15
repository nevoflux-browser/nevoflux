/// Permission level for modifying a section of a soul document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangePermission {
    /// Cannot be modified by the learning system (hardcoded safety boundaries).
    Forbidden,
    /// Requires user to confirm twice before applying.
    RequireDoubleConfirm,
    /// Requires user to confirm once before applying.
    RequireConfirm,
    /// Applied automatically, user is notified.
    AutoWithNotify,
}

/// Check the protection level for a given document file and section.
///
/// Protection levels (from design doc):
/// - L0 Immutable: SOUL.md > Safety Boundaries — Forbidden
/// - L1 Strong: IDENTITY.md all, SOUL.md Core Values — RequireDoubleConfirm
/// - L2 Semi: SOUL.md behavior rules, USER.md basics, AGENTS.md flows — RequireConfirm
/// - L3 Auto: USER.md preferences, TOOLS.md all, AGENTS.md strategies — AutoWithNotify
pub fn check_permission(target: &str, section: &str) -> ChangePermission {
    match (target, section) {
        // IDENTITY.md — entirely strong protection
        ("IDENTITY.md", _) => ChangePermission::RequireDoubleConfirm,

        // SOUL.md — per section
        ("SOUL.md", "Safety Boundaries") => ChangePermission::Forbidden,
        ("SOUL.md", "Core Values") => ChangePermission::RequireDoubleConfirm,
        ("SOUL.md", _) => ChangePermission::RequireConfirm,

        // USER.md — per section
        ("USER.md", "Basic Information") => ChangePermission::RequireConfirm,
        ("USER.md", "Sensitive Domain Blacklist") => ChangePermission::RequireConfirm,
        ("USER.md", _) => ChangePermission::AutoWithNotify,

        // TOOLS.md — entirely auto
        ("TOOLS.md", _) => ChangePermission::AutoWithNotify,

        // AGENTS.md — per section
        ("AGENTS.md", "Task Execution Flow") => ChangePermission::RequireConfirm,
        ("AGENTS.md", "Failure Fallback Strategy") => ChangePermission::RequireConfirm,
        ("AGENTS.md", _) => ChangePermission::AutoWithNotify,

        // Default: require confirmation
        _ => ChangePermission::RequireConfirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soul_safety_boundaries_are_forbidden() {
        assert_eq!(
            check_permission("SOUL.md", "Safety Boundaries"),
            ChangePermission::Forbidden
        );
    }

    #[test]
    fn identity_requires_double_confirm() {
        assert_eq!(
            check_permission("IDENTITY.md", "Name"),
            ChangePermission::RequireDoubleConfirm
        );
        assert_eq!(
            check_permission("IDENTITY.md", "Core Positioning"),
            ChangePermission::RequireDoubleConfirm
        );
    }

    #[test]
    fn soul_core_values_requires_double_confirm() {
        assert_eq!(
            check_permission("SOUL.md", "Core Values"),
            ChangePermission::RequireDoubleConfirm
        );
    }

    #[test]
    fn soul_other_sections_require_confirm() {
        assert_eq!(
            check_permission("SOUL.md", "Default Communication Style"),
            ChangePermission::RequireConfirm
        );
        assert_eq!(
            check_permission("SOUL.md", "Error Handling Guidelines"),
            ChangePermission::RequireConfirm
        );
    }

    #[test]
    fn tools_md_is_auto_with_notify() {
        assert_eq!(
            check_permission("TOOLS.md", "Site Adaptation Graph"),
            ChangePermission::AutoWithNotify
        );
        assert_eq!(
            check_permission("TOOLS.md", "Runtime Parameters"),
            ChangePermission::AutoWithNotify
        );
        assert_eq!(
            check_permission("TOOLS.md", "MCP Tool Inventory"),
            ChangePermission::AutoWithNotify
        );
    }

    #[test]
    fn user_md_mixed_protection() {
        assert_eq!(
            check_permission("USER.md", "Basic Information"),
            ChangePermission::RequireConfirm
        );
        assert_eq!(
            check_permission("USER.md", "Sensitive Domain Blacklist"),
            ChangePermission::RequireConfirm
        );
        assert_eq!(
            check_permission("USER.md", "Communication Overrides"),
            ChangePermission::AutoWithNotify
        );
        assert_eq!(
            check_permission("USER.md", "Professional Domains"),
            ChangePermission::AutoWithNotify
        );
    }

    #[test]
    fn agents_md_mixed_protection() {
        assert_eq!(
            check_permission("AGENTS.md", "Task Execution Flow"),
            ChangePermission::RequireConfirm
        );
        assert_eq!(
            check_permission("AGENTS.md", "Failure Fallback Strategy"),
            ChangePermission::RequireConfirm
        );
        assert_eq!(
            check_permission("AGENTS.md", "Multi-Task Orchestration"),
            ChangePermission::AutoWithNotify
        );
        assert_eq!(
            check_permission("AGENTS.md", "Session Collaboration"),
            ChangePermission::AutoWithNotify
        );
    }

    #[test]
    fn unknown_file_defaults_to_require_confirm() {
        assert_eq!(
            check_permission("UNKNOWN.md", "Something"),
            ChangePermission::RequireConfirm
        );
    }
}
