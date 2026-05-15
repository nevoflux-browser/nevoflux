//! Wasm Agent ABI contract definitions.
//!
//! This module defines the ABI (Application Binary Interface) contract between
//! the host (daemon) and guest (Wasm agent). It includes constants for function
//! names, data structures for input/output, and result codes.

use nevoflux_protocol::{Artifact, PlanProposal};
use serde::{Deserialize, Serialize};

// ============================================================================
// ABI Constants
// ============================================================================

/// Current ABI version. Incremented when breaking changes are made.
pub const ABI_VERSION: i32 = 1;

/// Name of the main entry point function that processes agent requests.
pub const ENTRY_POINT: &str = "agent_process";

/// Name of the memory export from the Wasm module.
pub const MEMORY_EXPORT: &str = "memory";

/// Name of the function that returns the ABI version.
pub const ABI_VERSION_FUNC: &str = "get_abi_version";

/// Name of the memory allocation function in the Wasm module.
pub const ALLOC_FUNC: &str = "alloc";

/// Name of the memory deallocation function in the Wasm module.
pub const FREE_FUNC: &str = "free";

// ============================================================================
// Agent Result Codes
// ============================================================================

/// Result codes returned by the agent after processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum AgentResult {
    /// Agent completed successfully and has no more work to do.
    Complete = 0,
    /// Agent needs to continue processing (e.g., waiting for tool results).
    Continue = 1,
    /// An error occurred during processing.
    Error = -1,
    /// The operation was cancelled.
    Cancelled = -2,
}

impl From<i32> for AgentResult {
    fn from(value: i32) -> Self {
        match value {
            0 => AgentResult::Complete,
            1 => AgentResult::Continue,
            -1 => AgentResult::Error,
            -2 => AgentResult::Cancelled,
            _ => AgentResult::Error,
        }
    }
}

impl From<AgentResult> for i32 {
    fn from(result: AgentResult) -> i32 {
        result as i32
    }
}

// ============================================================================
// Input Types
// ============================================================================

/// Input passed to the agent's process function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProcessInput {
    /// Unique session identifier.
    pub session_id: String,
    /// Current iteration number (starts at 0).
    pub iteration: u32,
    /// The content to process.
    pub content: AgentContent,
    /// Conversation history.
    pub history: Vec<HistoryEntry>,
    /// Optional trace summary injected by pattern detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_summary: Option<String>,
}

/// Content types that can be passed to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentContent {
    /// A message from the user.
    UserMessage {
        /// The text content of the message.
        text: String,
    },
    /// Results from tool executions.
    ToolResults {
        /// The list of tool results.
        results: Vec<ToolResult>,
    },
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The unique identifier of the tool call.
    pub call_id: String,
    /// The name of the tool that was called.
    pub name: String,
    /// The content returned by the tool (if successful).
    pub content: Option<String>,
    /// The error message (if the tool failed).
    pub error: Option<String>,
}

/// An entry in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The role of the message sender (e.g., "user", "assistant").
    pub role: String,
    /// The content of the message.
    pub content: String,
}

// ============================================================================
// Output Types
// ============================================================================

/// Output returned by the agent after processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProcessOutput {
    /// The text response from the agent.
    pub text: String,
    /// Tool calls the agent wants to make.
    pub tool_calls: Vec<PendingToolCall>,
    /// Whether the agent has completed processing.
    pub complete: bool,
    /// Optional plan proposal for multi-step tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_proposal: Option<PlanProposal>,
    /// Optional artifact created by the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
}

/// A pending tool call requested by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    /// Unique identifier for this tool call.
    pub id: String,
    /// The name of the tool to call.
    pub name: String,
    /// Arguments to pass to the tool.
    pub arguments: serde_json::Value,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_result_from_i32() {
        assert_eq!(AgentResult::from(0), AgentResult::Complete);
        assert_eq!(AgentResult::from(1), AgentResult::Continue);
        assert_eq!(AgentResult::from(-1), AgentResult::Error);
        assert_eq!(AgentResult::from(-2), AgentResult::Cancelled);
        // Unknown values map to Error
        assert_eq!(AgentResult::from(999), AgentResult::Error);
        assert_eq!(AgentResult::from(-999), AgentResult::Error);
    }

    #[test]
    fn test_agent_result_to_i32() {
        assert_eq!(i32::from(AgentResult::Complete), 0);
        assert_eq!(i32::from(AgentResult::Continue), 1);
        assert_eq!(i32::from(AgentResult::Error), -1);
        assert_eq!(i32::from(AgentResult::Cancelled), -2);
    }

    #[test]
    fn test_agent_process_input_serialization() {
        let input = AgentProcessInput {
            session_id: "sess-001".to_string(),
            iteration: 0,
            content: AgentContent::UserMessage {
                text: "Hello, agent!".to_string(),
            },
            history: vec![HistoryEntry {
                role: "user".to_string(),
                content: "Previous message".to_string(),
            }],
            trace_summary: None,
        };

        let json = serde_json::to_string(&input).unwrap();
        let deserialized: AgentProcessInput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.session_id, "sess-001");
        assert_eq!(deserialized.iteration, 0);
        assert_eq!(deserialized.history.len(), 1);
        assert_eq!(deserialized.history[0].role, "user");
    }

    #[test]
    fn test_agent_content_user_message_serialization() {
        let content = AgentContent::UserMessage {
            text: "Test message".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains(r#""type":"user_message""#));
        assert!(json.contains(r#""text":"Test message""#));

        let deserialized: AgentContent = serde_json::from_str(&json).unwrap();
        if let AgentContent::UserMessage { text } = deserialized {
            assert_eq!(text, "Test message");
        } else {
            panic!("Expected UserMessage variant");
        }
    }

    #[test]
    fn test_agent_content_tool_results_serialization() {
        let content = AgentContent::ToolResults {
            results: vec![
                ToolResult {
                    call_id: "call-001".to_string(),
                    name: "read_file".to_string(),
                    content: Some("file contents".to_string()),
                    error: None,
                },
                ToolResult {
                    call_id: "call-002".to_string(),
                    name: "write_file".to_string(),
                    content: None,
                    error: Some("Permission denied".to_string()),
                },
            ],
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains(r#""type":"tool_results""#));

        let deserialized: AgentContent = serde_json::from_str(&json).unwrap();
        if let AgentContent::ToolResults { results } = deserialized {
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].call_id, "call-001");
            assert!(results[0].content.is_some());
            assert!(results[0].error.is_none());
            assert_eq!(results[1].call_id, "call-002");
            assert!(results[1].content.is_none());
            assert!(results[1].error.is_some());
        } else {
            panic!("Expected ToolResults variant");
        }
    }

    #[test]
    fn test_agent_process_output_serialization() {
        let output = AgentProcessOutput {
            text: "I'll help you with that.".to_string(),
            tool_calls: vec![PendingToolCall {
                id: "tc-001".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({
                    "path": "/tmp/test.txt"
                }),
            }],
            complete: false,
            plan_proposal: None,
            artifact: None,
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: AgentProcessOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.text, "I'll help you with that.");
        assert_eq!(deserialized.tool_calls.len(), 1);
        assert_eq!(deserialized.tool_calls[0].id, "tc-001");
        assert_eq!(deserialized.tool_calls[0].name, "read_file");
        assert!(!deserialized.complete);
    }

    #[test]
    fn test_pending_tool_call_with_complex_arguments() {
        let tool_call = PendingToolCall {
            id: "tc-002".to_string(),
            name: "execute_command".to_string(),
            arguments: serde_json::json!({
                "command": "ls",
                "args": ["-la", "/tmp"],
                "env": {
                    "PATH": "/usr/bin"
                }
            }),
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        let deserialized: PendingToolCall = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "tc-002");
        assert_eq!(deserialized.arguments["command"], "ls");
        assert_eq!(deserialized.arguments["args"][0], "-la");
        assert_eq!(deserialized.arguments["env"]["PATH"], "/usr/bin");
    }

    #[test]
    fn test_history_entry_serialization() {
        let entry = HistoryEntry {
            role: "assistant".to_string(),
            content: "How can I help you?".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: HistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.role, "assistant");
        assert_eq!(deserialized.content, "How can I help you?");
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            call_id: "call-003".to_string(),
            name: "search".to_string(),
            content: Some("Found 5 results".to_string()),
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.call_id, "call-003");
        assert_eq!(deserialized.name, "search");
        assert_eq!(deserialized.content, Some("Found 5 results".to_string()));
        assert!(deserialized.error.is_none());
    }

    #[test]
    fn test_abi_constants() {
        assert_eq!(ABI_VERSION, 1);
        assert_eq!(ENTRY_POINT, "agent_process");
        assert_eq!(MEMORY_EXPORT, "memory");
        assert_eq!(ABI_VERSION_FUNC, "get_abi_version");
        assert_eq!(ALLOC_FUNC, "alloc");
        assert_eq!(FREE_FUNC, "free");
    }

    #[test]
    fn test_agent_process_output_with_plan_proposal() {
        use nevoflux_protocol::PlanStep;

        let proposal = PlanProposal {
            summary: "Refactor the login module".to_string(),
            steps: vec![
                PlanStep {
                    description: "Extract validation logic".to_string(),
                    model: None,
                },
                PlanStep {
                    description: "Add unit tests".to_string(),
                    model: Some("claude-opus-4-5-20251101".to_string()),
                },
            ],
        };

        let output = AgentProcessOutput {
            text: "Here is my plan.".to_string(),
            tool_calls: vec![],
            complete: false,
            plan_proposal: Some(proposal.clone()),
            artifact: None,
        };

        // Verify serialization roundtrip
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: AgentProcessOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.text, "Here is my plan.");
        assert!(!deserialized.complete);
        assert!(deserialized.tool_calls.is_empty());

        let plan = deserialized
            .plan_proposal
            .expect("plan_proposal should be Some");
        assert_eq!(plan.summary, "Refactor the login module");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].description, "Extract validation logic");
        assert!(plan.steps[0].model.is_none());
        assert_eq!(plan.steps[1].description, "Add unit tests");
        assert_eq!(
            plan.steps[1].model.as_deref(),
            Some("claude-opus-4-5-20251101")
        );

        // Verify "plan_proposal" appears in JSON
        assert!(json.contains("plan_proposal"));
    }

    #[test]
    fn test_agent_process_output_without_plan_proposal() {
        let output = AgentProcessOutput {
            text: "Done.".to_string(),
            tool_calls: vec![],
            complete: true,
            plan_proposal: None,
            artifact: None,
        };

        let json = serde_json::to_string(&output).unwrap();

        // Verify "plan_proposal" does NOT appear in JSON (skip_serializing_if)
        assert!(
            !json.contains("plan_proposal"),
            "plan_proposal should be omitted from JSON when None"
        );

        // Verify roundtrip still works
        let deserialized: AgentProcessOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text, "Done.");
        assert!(deserialized.complete);
        assert!(deserialized.plan_proposal.is_none());
    }
}
