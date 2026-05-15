//! Tool availability checking for skills.
//!
//! This module provides functions to check if the tools required by a skill
//! are available in the current environment before injecting the skill.
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_skills::{SkillMetadata, check_tool_availability, ToolCheckResult};
//!
//! let metadata = SkillMetadata::new("stitch-skill")
//!     .with_allowed_tool("stitch*:*");
//!
//! let available_tools = vec!["Read".to_string(), "Write".to_string()];
//! let result = check_tool_availability(&metadata, &available_tools);
//!
//! match result {
//!     ToolCheckResult::Satisfied => println!("All tools available"),
//!     ToolCheckResult::Missing(tools) => println!("Missing: {:?}", tools),
//! }
//! ```

use crate::types::SkillMetadata;

/// Result of checking tool availability for a skill.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCheckResult {
    /// All required tools are available.
    Satisfied,
    /// Some required tools are missing. Contains the list of missing patterns.
    Missing(Vec<String>),
}

impl ToolCheckResult {
    /// Returns true if all tools are satisfied.
    pub fn is_satisfied(&self) -> bool {
        matches!(self, ToolCheckResult::Satisfied)
    }

    /// Returns the missing tools if any.
    pub fn missing_tools(&self) -> Option<&[String]> {
        match self {
            ToolCheckResult::Missing(tools) => Some(tools),
            ToolCheckResult::Satisfied => None,
        }
    }
}

/// Check if the tools required by a skill are available.
///
/// # Arguments
///
/// * `metadata` - The skill metadata containing `allowed_tools` patterns.
/// * `available_tools` - List of available tool names in the format "name" or "server:name".
///
/// # Returns
///
/// Returns `ToolCheckResult::Satisfied` if all required tools are available,
/// or `ToolCheckResult::Missing` with the list of missing patterns.
///
/// # Pattern Matching
///
/// The `allowed_tools` field supports glob-like patterns:
/// - `Read` - Exact match for a tool named "Read"
/// - `stitch*` - Matches any tool starting with "stitch"
/// - `stitch:*` - Matches any tool from the "stitch" server
/// - `stitch*:*` - Matches any tool from any server starting with "stitch"
/// - `*` - Matches any tool (wildcard)
pub fn check_tool_availability(
    metadata: &SkillMetadata,
    available_tools: &[String],
) -> ToolCheckResult {
    if metadata.allowed_tools.is_empty() {
        return ToolCheckResult::Satisfied;
    }

    let missing: Vec<String> = metadata
        .allowed_tools
        .iter()
        .filter(|pattern| !tool_pattern_matches(pattern, available_tools))
        .cloned()
        .collect();

    if missing.is_empty() {
        ToolCheckResult::Satisfied
    } else {
        ToolCheckResult::Missing(missing)
    }
}

/// Check if a tool pattern matches any of the available tools.
///
/// # Pattern Syntax
///
/// - `*` in a pattern matches any sequence of characters
/// - Patterns without `:` match tool names directly
/// - Patterns with `:` match in the format `server:tool`
fn tool_pattern_matches(pattern: &str, available_tools: &[String]) -> bool {
    available_tools
        .iter()
        .any(|tool| matches_glob_pattern(pattern, tool))
}

/// Match a glob-like pattern against a string.
///
/// Supports `*` as a wildcard that matches any sequence of characters.
fn matches_glob_pattern(pattern: &str, text: &str) -> bool {
    // Handle simple cases first
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }

    // Split pattern by '*' and match segments
    let segments: Vec<&str> = pattern.split('*').collect();

    // Single '*' at the end: prefix match
    if segments.len() == 2 && segments[1].is_empty() {
        return text.starts_with(segments[0]);
    }

    // Single '*' at the start: suffix match
    if segments.len() == 2 && segments[0].is_empty() {
        return text.ends_with(segments[1]);
    }

    // General case: match all segments in order
    let mut pos = 0;
    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        if let Some(found_pos) = text[pos..].find(segment) {
            // First segment must be at the start if pattern doesn't start with '*'
            if i == 0 && found_pos != 0 {
                return false;
            }
            pos += found_pos + segment.len();
        } else {
            return false;
        }
    }

    // If pattern doesn't end with '*', text must end with the last segment
    if !pattern.ends_with('*') {
        if let Some(last_segment) = segments.last() {
            if !last_segment.is_empty() {
                return text.ends_with(last_segment);
            }
        }
    }

    true
}

/// Format a user-friendly error message for missing tools.
///
/// # Arguments
///
/// * `skill_name` - Name of the skill that requires the tools.
/// * `missing_tools` - List of missing tool patterns.
///
/// # Returns
///
/// A formatted error message explaining which tools are missing and how to resolve.
pub fn format_missing_tools_message(skill_name: &str, missing_tools: &[String]) -> String {
    let tools_list = missing_tools.join(", ");

    format!(
        "Skill '{}' requires tools that are not available: [{}]. \
         Please ensure the required MCP servers are configured and connected.",
        skill_name, tools_list
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_allowed_tools_always_satisfied() {
        let metadata = SkillMetadata::new("test-skill");
        let available = vec!["Read".to_string(), "Write".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_exact_match_satisfied() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("Read");
        let available = vec!["Read".to_string(), "Write".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_exact_match_missing() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("NotAvailable");
        let available = vec!["Read".to_string(), "Write".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(!result.is_satisfied());
        assert_eq!(
            result.missing_tools(),
            Some(&["NotAvailable".to_string()][..])
        );
    }

    #[test]
    fn test_prefix_glob_pattern() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("stitch*");
        let available = vec!["Read".to_string(), "stitch-server:create_page".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_prefix_glob_pattern_no_match() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("stitch*");
        let available = vec!["Read".to_string(), "Write".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(!result.is_satisfied());
    }

    #[test]
    fn test_server_colon_tool_pattern() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("notion:*");
        let available = vec![
            "Read".to_string(),
            "notion:search".to_string(),
            "notion:create_page".to_string(),
        ];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_server_wildcard_pattern() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("stitch*:*");
        let available = vec!["Read".to_string(), "stitch-prod:create_page".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_wildcard_matches_anything() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("*");
        let available = vec!["anything".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_multiple_patterns_all_satisfied() {
        let metadata = SkillMetadata::new("test-skill")
            .with_allowed_tool("Read")
            .with_allowed_tool("Write");
        let available = vec!["Read".to_string(), "Write".to_string(), "Bash".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(result.is_satisfied());
    }

    #[test]
    fn test_multiple_patterns_some_missing() {
        let metadata = SkillMetadata::new("test-skill")
            .with_allowed_tool("Read")
            .with_allowed_tool("stitch:*");
        let available = vec!["Read".to_string(), "Write".to_string()];

        let result = check_tool_availability(&metadata, &available);
        assert!(!result.is_satisfied());
        assert_eq!(result.missing_tools(), Some(&["stitch:*".to_string()][..]));
    }

    #[test]
    fn test_empty_available_tools() {
        let metadata = SkillMetadata::new("test-skill").with_allowed_tool("Read");
        let available: Vec<String> = vec![];

        let result = check_tool_availability(&metadata, &available);
        assert!(!result.is_satisfied());
    }

    #[test]
    fn test_format_missing_tools_message() {
        let message = format_missing_tools_message("my-skill", &["stitch:*".to_string()]);

        assert!(message.contains("my-skill"));
        assert!(message.contains("stitch:*"));
        assert!(message.contains("MCP servers"));
    }

    #[test]
    fn test_format_missing_tools_message_multiple() {
        let message = format_missing_tools_message(
            "my-skill",
            &["stitch:*".to_string(), "notion:*".to_string()],
        );

        assert!(message.contains("stitch:*"));
        assert!(message.contains("notion:*"));
    }

    // Glob pattern matching tests
    #[test]
    fn test_glob_exact_match() {
        assert!(matches_glob_pattern("Read", "Read"));
        assert!(!matches_glob_pattern("Read", "Write"));
    }

    #[test]
    fn test_glob_prefix_match() {
        assert!(matches_glob_pattern("stitch*", "stitch-server"));
        assert!(matches_glob_pattern("stitch*", "stitch"));
        assert!(!matches_glob_pattern("stitch*", "notion"));
    }

    #[test]
    fn test_glob_suffix_match() {
        assert!(matches_glob_pattern("*:create", "notion:create"));
        assert!(matches_glob_pattern("*:create", "stitch:create"));
        assert!(!matches_glob_pattern("*:create", "notion:search"));
    }

    #[test]
    fn test_glob_middle_wildcard() {
        assert!(matches_glob_pattern("stitch*:create", "stitch-prod:create"));
        assert!(matches_glob_pattern("stitch*:create", "stitch:create"));
        assert!(!matches_glob_pattern(
            "stitch*:create",
            "stitch-prod:search"
        ));
    }

    #[test]
    fn test_glob_full_wildcard() {
        assert!(matches_glob_pattern("*", "anything"));
        assert!(matches_glob_pattern("*", ""));
    }

    #[test]
    fn test_tool_check_result_methods() {
        let satisfied = ToolCheckResult::Satisfied;
        assert!(satisfied.is_satisfied());
        assert!(satisfied.missing_tools().is_none());

        let missing = ToolCheckResult::Missing(vec!["tool1".to_string()]);
        assert!(!missing.is_satisfied());
        assert_eq!(missing.missing_tools(), Some(&["tool1".to_string()][..]));
    }
}
