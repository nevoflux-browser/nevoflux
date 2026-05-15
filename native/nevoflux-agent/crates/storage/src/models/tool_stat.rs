//! Tool statistics data model.

use serde::{Deserialize, Serialize};

/// Statistics for an MCP tool's effectiveness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStat {
    /// Unique identifier (TS-{hash}).
    pub id: String,
    /// Name of the tool.
    pub tool_name: String,
    /// Category of intent this tool serves.
    pub intent_category: Option<String>,
    /// Total number of calls.
    pub call_count: i64,
    /// Number of successful calls.
    pub success_count: i64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: Option<f64>,
    /// Average token cost per call.
    pub avg_token_cost: Option<f64>,
    /// JSON of commonly used parameters.
    pub common_params: Option<String>,
    /// JSON of failure patterns observed.
    pub failure_patterns: Option<String>,
    /// JSON of best tool combinations.
    pub best_combinations: Option<String>,
    /// When the record was last updated (RFC 3339).
    pub updated_at: String,
}

/// Parameters for creating a new tool stat record.
#[derive(Debug, Clone)]
pub struct CreateToolStatParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// Name of the tool.
    pub tool_name: String,
    /// Category of intent this tool serves.
    pub intent_category: Option<String>,
}

impl CreateToolStatParams {
    /// Create new params with the tool name.
    pub fn new(tool_name: &str) -> Self {
        Self {
            id: None,
            tool_name: tool_name.to_string(),
            intent_category: None,
        }
    }

    /// Set a custom ID.
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set the intent category.
    pub fn with_intent_category(mut self, category: &str) -> Self {
        self.intent_category = Some(category.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_tool_stat_params() {
        let params = CreateToolStatParams::new("browser_click");

        assert_eq!(params.tool_name, "browser_click");
        assert!(params.id.is_none());
        assert!(params.intent_category.is_none());
    }

    #[test]
    fn test_create_tool_stat_params_builder() {
        let params = CreateToolStatParams::new("browser_navigate")
            .with_id("TS-abc123")
            .with_intent_category("navigation");

        assert_eq!(params.id, Some("TS-abc123".to_string()));
        assert_eq!(params.tool_name, "browser_navigate");
        assert_eq!(params.intent_category, Some("navigation".to_string()));
    }
}
