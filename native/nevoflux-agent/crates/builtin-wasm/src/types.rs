//! Types for host-guest communication.
//!
//! These types are serialized via MessagePack for efficient transfer.

use nevoflux_protocol::subagent::ToolsConfig;
use nevoflux_protocol::{Artifact, LocalFileRef, PlanProposal};
use serde::{Deserialize, Serialize};

/// Information about a browser tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabInfo {
    /// The space/group the tab belongs to (e.g., workspace name).
    #[serde(default)]
    pub space: String,
    /// The tab's unique ID.
    pub tab_id: i64,
    /// The tab's title.
    #[serde(default)]
    pub tab_title: String,
    /// The tab's URL.
    #[serde(default)]
    pub url: String,
}

/// Agent execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Chat mode - dialogue + current page understanding.
    #[default]
    Chat,
    /// Browser mode - active browser control.
    Browser,
    /// Agent mode - full capabilities including file/bash/computer use.
    Agent,
    /// Code mode - DEPRECATED. Maps to Agent mode internally.
    /// Kept for protocol backward compatibility.
    #[deprecated(note = "Use AgentMode::Agent instead. Code mode maps to Agent internally.")]
    Code,
}

/// Message role in conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System message (instructions).
    System,
    /// User message.
    User,
    /// Assistant (AI) message.
    Assistant,
    /// Tool use message.
    Tool,
}

/// A conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message role.
    pub role: MessageRole,
    /// Message content.
    pub content: String,
    /// Optional tool call ID (for tool responses).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by assistant (only for assistant role).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Attachments for multimodal messages (images, files).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Reasoning / thinking content from the model that produced this turn
    /// (assistant role only). Some providers (DeepSeek with tool calls)
    /// require it to be echoed back on subsequent turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

impl Message {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create a user message with attachments.
    pub fn user_with_attachments(content: impl Into<String>, attachments: Vec<Attachment>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            attachments,
            reasoning: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create an assistant message with tool calls.
    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
            attachments: Vec::new(),
            reasoning: None,
        }
    }

    /// Create an assistant message with tool calls and reasoning content.
    ///
    /// Used when the model returns reasoning_content alongside tool_calls
    /// (e.g. DeepSeek thinking-mode). The reasoning string must be echoed
    /// back to the provider on the next turn.
    pub fn assistant_with_tool_calls_and_reasoning(
        content: impl Into<String>,
        tool_calls: Vec<ToolCall>,
        reasoning: Option<String>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
            attachments: Vec::new(),
            reasoning,
        }
    }

    /// Create a tool response message.
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            reasoning: None,
        }
    }
}

/// Tool definition for LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON schema for input parameters.
    pub input_schema: serde_json::Value,
}

/// Tool call request from LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call.
    pub id: String,
    /// Call ID used to match tool results with tool calls.
    /// For OpenAI Responses API, this is different from `id` and MUST be used
    /// when sending tool results back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// Tool name to invoke.
    pub name: String,
    /// Tool arguments as JSON.
    pub arguments: serde_json::Value,
    /// Optional cryptographic signature for the tool call.
    /// Used by Gemini 3 (thought_signature) to verify the tool call was generated
    /// by the model. MUST be preserved and sent back with tool results for multi-turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool call ID.
    pub tool_call_id: String,
    /// Result content.
    pub content: String,
    /// Whether the execution was successful.
    pub success: bool,
}

/// LLM request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    /// Messages to send.
    pub messages: Vec<Message>,
    /// Available tools.
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
}

/// An image generated by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImage {
    /// MIME type (e.g., "image/png").
    pub media_type: String,
    /// Base64-encoded image data.
    pub data: String,
}

/// LLM response chunk (for streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChunk {
    /// Text content delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Tool calls in this chunk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Whether this is the final chunk.
    #[serde(default)]
    pub done: bool,
    /// Reasoning / thinking content delta from the model (e.g. DeepSeek
    /// `reasoning_content` for thinking-mode models). Must be round-tripped
    /// back to providers that require it (DeepSeek with tool calls).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Generated images.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<GeneratedImage>,
}

/// LLM response (non-streaming or aggregated from a stream).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// Full response text.
    pub text: String,
    /// Tool calls from the response.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Aggregated reasoning / thinking content for the turn (DeepSeek
    /// thinking-mode and similar). None when the model produced none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Agent input from host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    /// Session ID.
    pub session_id: String,
    /// Current mode.
    pub mode: AgentMode,
    /// User message.
    pub user_message: String,
    /// Conversation history.
    #[serde(default)]
    pub history: Vec<Message>,
    /// Attachments for multimodal input (images, files).
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Local file references attached by user (paths only, not content).
    /// These are displayed in the prompt so LLM can decide to read them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_files: Vec<LocalFileRef>,
    /// Custom system prompt (overrides default mode-based prompt).
    ///
    /// When set, this replaces the built-in system prompt for the agent mode.
    /// This is primarily used for sub-agents that need specialized instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_system_prompt: Option<String>,
    /// Current active browser tab ID.
    ///
    /// When set, the agent knows which tab to interact with using browser tools.
    /// This is passed from the browser sidebar when the user sends a message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// List of all available browser tabs with their metadata.
    ///
    /// This provides the agent with context about all open tabs,
    /// allowing it to navigate between tabs or reference content from multiple tabs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_ids: Vec<TabInfo>,
    /// Skill context to prepend to system prompt.
    ///
    /// When a user invokes a skill (e.g., `/design-md`), the skill's instructions
    /// are stored here instead of in user_message. This gives skill instructions
    /// higher priority by placing them at the beginning of the system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_context: Option<SkillContext>,
    /// Available LLM models for plan step model selection.
    /// Each entry is (provider_name, model_name).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_models: Vec<(String, String)>,
    /// Names of enabled MCP servers for system prompt injection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    /// Soul document context for system prompt injection.
    ///
    /// Contains formatted content from IDENTITY.md, SOUL.md, USER.md,
    /// TOOLS.md, and AGENTS.md for persona and knowledge injection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soul_context: Option<String>,
    /// Tool filtering config for this agent instance.
    ///
    /// - `None` (Option::None): inherit the mode's full tool set
    /// - `Some(ToolsConfig::None)`: disable all tools (single text response)
    /// - `Some(ToolsConfig::Allow(list))`: allowlist with wildcard support
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_config: Option<ToolsConfig>,
    /// Host operating system (e.g., "windows", "linux", "macos").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_platform: Option<String>,
}

/// Skill context for injection into system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContext {
    /// Skill name.
    pub name: String,
    /// Base path for skill's auxiliary files.
    pub base_path: String,
    /// Skill content/instructions.
    pub content: String,
    /// Files available in the skill directory (filenames only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_files: Vec<String>,
}

/// Agent output to host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// Response text.
    pub text: String,
    /// Tool calls made during execution.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Whether the agent loop should continue.
    pub continue_loop: bool,
    /// Optional plan proposal for multi-step tasks.
    ///
    /// When the agent calls the `plan` tool, the proposed plan is returned here
    /// so the runner can pause execution and wait for user confirmation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_proposal: Option<PlanProposal>,
    /// Optional artifact created by the agent.
    ///
    /// When the agent calls the `create_artifact` tool, the artifact is returned here
    /// so the runner can send it to the sidebar for rendering in a canvas tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
}

/// Skill summary (from skill_list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    /// Skill name.
    pub name: String,
    /// Brief description.
    pub description: String,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Memory chunk for search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    /// Chunk ID.
    pub id: String,
    /// Content text.
    pub content: String,
    /// Source session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Relevance score.
    pub score: f32,
}

/// Entry returned by memory_view, representing a hot knowledge item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeViewEntry {
    /// Knowledge entry ID.
    pub id: String,
    /// Category: user_preference, site_interaction, or tool_optimization.
    pub category: String,
    /// One-line summary.
    pub summary: String,
    /// Associated domain (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// When the entry was created.
    pub created_at: String,
}

/// Attachment for multimodal messages (images, files, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment name/filename.
    pub name: String,
    /// MIME type (e.g., "image/png", "image/jpeg", "application/pdf").
    pub mime_type: String,
    /// Base64 encoded data.
    pub data: String,
}

/// Result from browser tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolResult {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Result data (action-specific).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error message if the operation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Base64 encoded screenshot (for screenshot action).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

impl BrowserToolResult {
    /// Create a successful result with data.
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            screenshot: None,
        }
    }

    /// Create a successful result with no data.
    pub fn ok() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            screenshot: None,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            screenshot: None,
        }
    }

    /// Create a screenshot result.
    pub fn screenshot(base64_data: impl Into<String>) -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            screenshot: Some(base64_data.into()),
        }
    }
}

/// Information about a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    /// Sub-agent ID.
    pub id: u64,
    /// Task description.
    pub task: String,
    /// Execution mode (chat, browser, agent).
    pub mode: String,
    /// Current status (running, completed, failed).
    pub status: String,
}

/// Result from tool search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchResult {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// BM25 relevance score.
    pub score: f64,
    /// JSON Schema for tool input parameters.
    pub input_schema: serde_json::Value,
    /// Source of the tool (e.g., "mcp:filesystem", "builtin").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Result of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub total_lines: u64,
    pub total_bytes: u64,
    pub returned_lines: u64,
    pub offset: u64,
    pub content: String,
    pub truncated: bool,
}

/// A single grep match entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: u64,
    pub content: String,
}

/// Result of a grep search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    pub total_matches: u64,
    pub total_files: u64,
    pub returned: u64,
    pub results: Vec<GrepMatch>,
    pub truncated: bool,
}

/// Status of a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BashStatus {
    Success,
    Error,
    Timeout,
    Killed,
}

/// Result of a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashResult {
    pub exit_code: Option<i32>,
    pub status: BashStatus,
    pub total_lines: u64,
    pub total_bytes: u64,
    pub returned_lines: u64,
    pub stdout: String,
    pub stderr: Option<String>,
    pub truncated: bool,
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mode_default() {
        assert_eq!(AgentMode::default(), AgentMode::Chat);
    }

    #[test]
    fn test_message_constructors() {
        let sys = Message::system("You are helpful");
        assert_eq!(sys.role, MessageRole::System);
        assert_eq!(sys.content, "You are helpful");

        let user = Message::user("Hello");
        assert_eq!(user.role, MessageRole::User);

        let assistant = Message::assistant("Hi there");
        assert_eq!(assistant.role, MessageRole::Assistant);

        let tool = Message::tool("call-123", "Result");
        assert_eq!(tool.role, MessageRole::Tool);
        assert_eq!(tool.tool_call_id, Some("call-123".into()));
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test message");
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, MessageRole::User);
        assert_eq!(decoded.content, "Test message");
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("read_file"));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "bash".into(),
            arguments: serde_json::json!({"command": "ls -la"}),
            signature: None,
        };
        assert_eq!(call.name, "bash");
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult {
            tool_call_id: "call-001".into(),
            content: "file1.txt\nfile2.txt".into(),
            success: true,
        };
        assert!(result.success);
    }

    #[test]
    fn test_agent_input() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Agent,
            user_message: "Help me".into(),
            history: vec![Message::system("You are helpful")],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        assert_eq!(input.mode, AgentMode::Agent);
        assert_eq!(input.history.len(), 1);
        assert!(input.custom_system_prompt.is_none());
    }

    #[test]
    fn test_agent_input_with_custom_prompt() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Agent,
            user_message: "Search for files".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: Some("You are a sub-agent focused on file search.".into()),
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        assert!(input.custom_system_prompt.is_some());
        assert!(input
            .custom_system_prompt
            .as_ref()
            .unwrap()
            .contains("sub-agent"));
    }

    #[test]
    fn test_agent_input_custom_prompt_serialization() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: Some("Custom prompt".into()),
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("custom_system_prompt"));
        assert!(json.contains("Custom prompt"));

        // Verify None is not serialized
        let input_no_prompt = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json2 = serde_json::to_string(&input_no_prompt).unwrap();
        assert!(!json2.contains("custom_system_prompt"));
    }

    #[test]
    fn test_agent_output() {
        let output = AgentOutput {
            text: "Here's my response".into(),
            tool_calls: vec![],
            continue_loop: false,
            plan_proposal: None,
            artifact: None,
        };
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_llm_request() {
        let req = LlmRequest {
            messages: vec![Message::user("Hello")],
            tools: vec![],
            stream: false,
        };
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn test_llm_response() {
        let resp = LlmResponse {
            text: "Hello there".into(),
            tool_calls: vec![],
            reasoning: None,
        };
        assert!(resp.tool_calls.is_empty());
    }

    #[test]
    fn llm_response_carries_reasoning_across_serde() {
        let r = LlmResponse {
            text: "hi".into(),
            tool_calls: vec![],
            reasoning: Some("thinking...".into()),
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: LlmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.reasoning.as_deref(), Some("thinking..."));
    }

    #[test]
    fn test_llm_chunk() {
        let chunk = LlmChunk {
            text: Some("Hello".into()),
            tool_calls: vec![],
            done: false,
            reasoning: None,
            images: vec![],
        };
        assert!(!chunk.done);
    }

    #[test]
    fn test_skill_summary() {
        let skill = SkillSummary {
            name: "code-review".into(),
            description: "Review code".into(),
            tags: vec!["code".into()],
        };
        assert_eq!(skill.tags.len(), 1);
    }

    #[test]
    fn test_memory_chunk() {
        let chunk = MemoryChunk {
            id: "mem-001".into(),
            content: "Some memory".into(),
            session_id: Some("sess-001".into()),
            score: 0.95,
        };
        assert!(chunk.score > 0.9);
    }

    #[test]
    fn test_tool_search_result() {
        let result = ToolSearchResult {
            name: "read_file".into(),
            description: "Read a file from disk".into(),
            score: 1.5,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
            source: Some("mcp:filesystem".into()),
        };
        assert_eq!(result.name, "read_file");
        assert!(result.score > 0.0);
        assert!(result.source.is_some());
    }

    #[test]
    fn test_tool_search_result_serialization() {
        let result = ToolSearchResult {
            name: "test_tool".into(),
            description: "A test tool".into(),
            score: 2.0,
            input_schema: serde_json::json!({"type": "object"}),
            source: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test_tool"));
        // source should be omitted when None
        assert!(!json.contains("source"));

        let decoded: ToolSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test_tool");
        assert!(decoded.source.is_none());
    }

    #[test]
    fn test_browser_tool_result_success() {
        let result = BrowserToolResult::success(serde_json::json!({"url": "https://example.com"}));
        assert!(result.success);
        assert!(result.data.is_some());
        assert!(result.error.is_none());
        assert!(result.screenshot.is_none());
    }

    #[test]
    fn test_browser_tool_result_ok() {
        let result = BrowserToolResult::ok();
        assert!(result.success);
        assert!(result.data.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_browser_tool_result_error() {
        let result = BrowserToolResult::error("Element not found");
        assert!(!result.success);
        assert!(result.data.is_none());
        assert_eq!(result.error, Some("Element not found".into()));
    }

    #[test]
    fn test_browser_tool_result_screenshot() {
        let result = BrowserToolResult::screenshot("iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB");
        assert!(result.success);
        assert!(result.screenshot.is_some());
    }

    #[test]
    fn test_browser_tool_result_serialization() {
        let result = BrowserToolResult::success(serde_json::json!({"content": "Hello"}));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"content\":\"Hello\""));
        // error and screenshot should be omitted when None
        assert!(!json.contains("error"));
        assert!(!json.contains("screenshot"));

        let decoded: BrowserToolResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert!(decoded.data.is_some());
    }

    #[test]
    fn test_subagent_info() {
        let info = SubagentInfo {
            id: 42,
            task: "Search for information".into(),
            mode: "agent".into(),
            status: "running".into(),
        };
        assert_eq!(info.id, 42);
        assert_eq!(info.task, "Search for information");
        assert_eq!(info.mode, "agent");
        assert_eq!(info.status, "running");
    }

    #[test]
    fn test_subagent_info_serialization() {
        let info = SubagentInfo {
            id: 1,
            task: "Test task".into(),
            mode: "chat".into(),
            status: "completed".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"task\":\"Test task\""));
        assert!(json.contains("\"mode\":\"chat\""));
        assert!(json.contains("\"status\":\"completed\""));

        let decoded: SubagentInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, 1);
        assert_eq!(decoded.task, "Test task");
    }

    #[test]
    fn test_agent_input_with_local_files() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Analyze this file".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![LocalFileRef {
                path: "/home/user/test.rs".into(),
                is_directory: false,
                size: Some(1024),
                modified: Some(1706600000),
            }],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("local_files"));
        assert!(json.contains("/home/user/test.rs"));

        let decoded: AgentInput = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.local_files.len(), 1);
        assert_eq!(decoded.local_files[0].path, "/home/user/test.rs");
    }

    #[test]
    fn test_agent_input_local_files_empty_not_serialized() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        // Empty vec should not be serialized
        assert!(!json.contains("local_files"));
    }

    #[test]
    fn test_agent_input_with_tab_id() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Browser,
            user_message: "Summarize this page".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: Some(42),
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        assert_eq!(input.tab_id, Some(42));

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("tab_id"));
        assert!(json.contains("42"));

        // Verify None is not serialized
        let input_no_tab = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json2 = serde_json::to_string(&input_no_tab).unwrap();
        assert!(!json2.contains("tab_id"));
    }

    #[test]
    fn test_agent_input_with_tab_ids() {
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Browser,
            user_message: "Compare these tabs".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: Some(1),
            tab_ids: vec![
                TabInfo {
                    space: "Work".into(),
                    tab_id: 1,
                    tab_title: "GitHub".into(),
                    url: String::new(),
                },
                TabInfo {
                    space: "Work".into(),
                    tab_id: 2,
                    tab_title: "Docs".into(),
                    url: String::new(),
                },
                TabInfo {
                    space: "Personal".into(),
                    tab_id: 3,
                    tab_title: "Email".into(),
                    url: String::new(),
                },
            ],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        assert_eq!(input.tab_ids.len(), 3);
        assert_eq!(input.tab_ids[0].space, "Work");
        assert_eq!(input.tab_ids[0].tab_id, 1);
        assert_eq!(input.tab_ids[0].tab_title, "GitHub");

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("tab_ids"));
        assert!(json.contains("GitHub"));
        assert!(json.contains("Work"));

        let decoded: AgentInput = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.tab_ids.len(), 3);
        assert_eq!(decoded.tab_ids[2].space, "Personal");

        // Verify empty vec is not serialized
        let input_no_tabs = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };
        let json2 = serde_json::to_string(&input_no_tabs).unwrap();
        assert!(!json2.contains("tab_ids"));
    }

    #[test]
    fn llm_chunk_carries_reasoning_across_serde() {
        let chunk = LlmChunk {
            text: None,
            tool_calls: vec![],
            done: false,
            reasoning: Some("step 1: pick the right tool".into()),
            images: vec![],
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let parsed: LlmChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.reasoning.as_deref(),
            Some("step 1: pick the right tool")
        );
    }

    #[test]
    fn message_assistant_with_tool_calls_and_reasoning_roundtrips() {
        let tc = ToolCall {
            id: "call_1".into(),
            call_id: None,
            name: "browser_get_markdown".into(),
            arguments: serde_json::json!({"tab_id": 4}),
            signature: None,
        };
        let msg = Message::assistant_with_tool_calls_and_reasoning(
            "",
            vec![tc.clone()],
            Some("I should fetch the page first.".into()),
        );

        assert!(matches!(msg.role, MessageRole::Assistant));
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(
            msg.reasoning.as_deref(),
            Some("I should fetch the page first.")
        );

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.reasoning.as_deref(),
            Some("I should fetch the page first.")
        );
    }
}
