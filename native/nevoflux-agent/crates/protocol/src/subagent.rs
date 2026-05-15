// crates/protocol/src/subagent.rs

//! Subagent role system protocol types.
//!
//! Defines configuration and result types for spawning and managing subagents
//! with specific roles, tool restrictions, and model overrides.

use serde::{Deserialize, Serialize};

/// Tool access configuration for a subagent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsConfig {
    /// Disable all tools
    None,
    /// Allowlist with wildcard support (e.g. `["browser_*", "read_file"]`)
    Allow(Vec<String>),
}

/// Summary of an agent role for listing/discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRoleSummary {
    /// Role name identifier
    pub name: String,
    /// Human-readable description
    pub description: String,
}

/// Configuration for spawning a subagent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SpawnSubagentConfig {
    /// The prompt/task to send to the subagent (required)
    pub prompt: String,

    /// Named role to apply (loads role-specific defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,

    /// Custom system prompt override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// LLM provider name (e.g. "anthropic", "openai")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model name override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Tool access configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsConfig>,

    /// Agent mode (e.g. "chat", "browser", "agent")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Maximum iterations before timeout
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,

    /// Browser tab to operate on
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
}

/// Status of a completed subagent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    /// Subagent completed successfully
    Completed,
    /// Subagent encountered an error
    Failed,
    /// Subagent was killed by the parent
    Killed,
    /// Subagent exceeded its iteration/time limit
    Timeout,
}

/// Result of a subagent execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentResult {
    /// Unique subagent instance id
    pub id: u64,
    /// Final status
    pub status: SubagentStatus,
    /// Output text from the subagent (if completed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message (if failed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock duration in milliseconds
    pub duration_ms: u64,
    /// Total tokens consumed
    pub tokens_used: u64,
}

/// Check whether a tool name matches a pattern.
///
/// Supported patterns:
/// - `"*"` matches everything
/// - `"prefix*"` matches any name starting with `prefix`
/// - Exact string matches the tool name literally
pub fn matches_tool_pattern(pattern: &str, tool_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        tool_name.starts_with(prefix)
    } else {
        pattern == tool_name
    }
}

/// Check whether a tool name is allowed by the given allowlist.
///
/// Returns `true` if any pattern in the allowlist matches.
pub fn is_tool_allowed(allowlist: &[String], tool_name: &str) -> bool {
    allowlist.iter().any(|p| matches_tool_pattern(p, tool_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_tool_pattern_exact() {
        assert!(matches_tool_pattern("read_file", "read_file"));
        assert!(!matches_tool_pattern("read_file", "write_file"));
    }

    #[test]
    fn test_matches_tool_pattern_wildcard() {
        assert!(matches_tool_pattern("browser_*", "browser_navigate"));
        assert!(matches_tool_pattern("browser_*", "browser_click"));
        assert!(!matches_tool_pattern("browser_*", "read_file"));
    }

    #[test]
    fn test_matches_tool_pattern_star() {
        assert!(matches_tool_pattern("*", "anything"));
        assert!(matches_tool_pattern("*", "browser_navigate"));
        assert!(matches_tool_pattern("*", ""));
    }

    #[test]
    fn test_is_tool_allowed() {
        let allowlist = vec!["browser_*".to_string(), "read_file".to_string()];
        assert!(is_tool_allowed(&allowlist, "browser_navigate"));
        assert!(is_tool_allowed(&allowlist, "browser_click"));
        assert!(is_tool_allowed(&allowlist, "read_file"));
        assert!(!is_tool_allowed(&allowlist, "write_file"));
        assert!(!is_tool_allowed(&allowlist, "execute_bash"));
    }

    #[test]
    fn test_is_tool_allowed_empty() {
        let allowlist: Vec<String> = vec![];
        assert!(!is_tool_allowed(&allowlist, "anything"));
        assert!(!is_tool_allowed(&allowlist, "read_file"));
    }

    #[test]
    fn test_tools_config_serde_none() {
        let config = ToolsConfig::None;
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(json, r#""none""#);
        let deserialized: ToolsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_tools_config_serde_allow() {
        let config = ToolsConfig::Allow(vec!["browser_*".to_string(), "read_file".to_string()]);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ToolsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_spawn_config_serde_minimal() {
        let config = SpawnSubagentConfig {
            prompt: "Do something".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Only prompt should be present
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(value["prompt"], "Do something");

        let deserialized: SpawnSubagentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_spawn_config_serde_full() {
        let config = SpawnSubagentConfig {
            prompt: "Navigate to example.com".to_string(),
            role: Some("researcher".to_string()),
            system_prompt: Some("You are a researcher.".to_string()),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-20250514".to_string()),
            tools: Some(ToolsConfig::Allow(vec!["browser_*".to_string()])),
            mode: Some("browser".to_string()),
            max_iterations: Some(20),
            tab_id: Some(42),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SpawnSubagentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_spawn_config_backward_compat() {
        // Old-style payload with only prompt and mode
        let json = r#"{"prompt":"Search for Rust tutorials","mode":"browser"}"#;
        let config: SpawnSubagentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.prompt, "Search for Rust tutorials");
        assert_eq!(config.mode, Some("browser".to_string()));
        assert_eq!(config.role, None);
        assert_eq!(config.tools, None);
        assert_eq!(config.max_iterations, None);
    }

    #[test]
    fn test_subagent_result_serde() {
        let result = SubagentResult {
            id: 42,
            status: SubagentStatus::Completed,
            output: Some("Task done".to_string()),
            error: None,
            duration_ms: 1500,
            tokens_used: 3200,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SubagentResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, result);

        // Verify error field is omitted when None
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(!value.as_object().unwrap().contains_key("error"));
    }

    #[test]
    fn test_agent_role_summary_serde() {
        let summary = AgentRoleSummary {
            name: "researcher".to_string(),
            description: "A role for web research tasks".to_string(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: AgentRoleSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, summary);
    }
}
