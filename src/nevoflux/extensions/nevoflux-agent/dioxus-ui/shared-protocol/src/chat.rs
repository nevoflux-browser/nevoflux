/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Chat Channel (bidirectional)
//!
//! Merged from Channel 1 (Input) and Channel 2 (Output).
//! All messages between Chat Sidebar and Native Agent.

use serde::{Deserialize, Serialize};
use crate::common::{
    AgentState, Attachment, BrowserToolAction, ContentType, ErrorLevel,
    PermissionScope, PlanType, PluginAction, Requester,
    ResourceAction, ResourceType, StreamFormat, ToolStatus,
};

// =============================================================================
// Sidebar → Agent Payloads
// =============================================================================

/// Chat mode (selected from sidebar toolbar)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMode {
    #[default]
    Chat,
    Browser,
    Agent,
}

/// Tab reference with space, id and title (for @ mention context)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabReference {
    /// The space/workspace the tab belongs to
    pub space: String,
    /// Tab id
    pub tab_id: i64,
    /// Tab title
    pub tab_title: String,
}

/// Chat message from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagePayload {
    pub session_id: String,
    pub message_id: String,
    /// Message content (renamed from "text" to match nevoflux-agent protocol)
    pub content: String,
    /// Chat mode: chat, browser, or agent
    #[serde(default)]
    pub mode: ChatMode,
    /// Image attachments (base64 encoded)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Local files/directories selected via native file picker
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_files: Vec<FileInfo>,
    /// Current active tab id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// Selected tabs for context (user selected via @ mention)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_ids: Vec<TabReference>,
}

/// Skill command trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCommandPayload {
    pub session_id: String,
    pub skill_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Stop generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopGenerationPayload {
    pub session_id: String,
}

/// Cancel request (user interruption)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelPayload {
    pub session_id: String,
}

/// Permission response from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponsePayload {
    pub request_id: String,
    pub granted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<PermissionScope>,
}

/// Plugin command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommandPayload {
    pub plugin_id: String,
    pub action: PluginAction,
}

/// System command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommandPayload {
    pub request_id: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Browser tool response from Sidebar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolResponsePayload {
    pub request_id: String,
    pub session_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserToolError>,
}

/// Browser tool error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub recoverable: bool,
}

// =============================================================================
// Agent → Sidebar Payloads
// =============================================================================

/// Stream chunk for streaming responses (matches nevoflux-agent protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    /// Response content
    pub content: String,
    /// Tool calls made by the agent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Whether generation is complete
    #[serde(default)]
    pub done: bool,
    /// Session title (only present when title is generated for first message)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    /// Real-time tool execution event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<ToolEvent>,
}

/// Tool call information from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    /// Tool call arguments — accepts both JSON string and JSON object from the daemon.
    /// The native agent sends `arguments` as `serde_json::Value` (object), but some
    /// code paths expect a string. This custom deserializer handles both.
    #[serde(deserialize_with = "deserialize_string_or_json")]
    pub arguments: String,
}

/// Deserialize a value that can be either a JSON string or a JSON object into a String.
fn deserialize_string_or_json<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

/// Stream end marker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEndPayload {
    pub session_id: String,
    pub stream_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<StreamMetadata>,
}

/// Metadata for completed stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Content block with type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockPayload {
    pub session_id: String,
    pub block_id: String,
    pub content_type: ContentType,
    pub content: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Permission request for human-in-the-loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub resource_type: ResourceType,
    pub action: ResourceAction,
    pub resource: String,
    pub requester: Requester,
    pub reason: String,
    pub scope: PermissionScope,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    60000
}

/// Agent state update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatePayload {
    pub session_id: String,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
}

/// Step information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInfo {
    pub current: u32,
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Tool execution information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub status: ToolStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Error notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub session_id: String,
    pub error_id: String,
    pub level: ErrorLevel,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default)]
    pub recoverable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_request_id: Option<String>,
}

/// Account status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatusPayload {
    pub logged_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaInfo>,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Plan information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInfo {
    #[serde(rename = "type")]
    pub plan_type: PlanType,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Quota information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_calls: Option<UsageQuota>,
}

/// Usage quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageQuota {
    pub used: u32,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

/// System command response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResponsePayload {
    pub request_id: String,
    pub command: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SystemError>,
}

/// System error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemError {
    pub code: String,
    pub message: String,
}

/// Browser tool request from Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolRequestPayload {
    pub request_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    pub action: BrowserToolAction,
    pub params: serde_json::Value,
    #[serde(default = "default_browser_timeout")]
    pub timeout_ms: u64,
}

fn default_browser_timeout() -> u64 {
    30000
}

// =============================================================================
// File Picker (Native Dialog via Agent)
// =============================================================================

/// File picker mode
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PickerMode {
    #[default]
    Files,
    Directories,
    Both,
}

/// Request to open native file picker dialog (Sidebar → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickFilesRequestPayload {
    pub request_id: String,
    #[serde(default)]
    pub mode: PickerMode,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_path: Option<String>,
}

/// File information returned from picker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    #[serde(default)]
    pub is_directory: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Response from native file picker dialog (Agent → Sidebar)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickFilesResponsePayload {
    pub request_id: String,
    #[serde(default)]
    pub files: Vec<FileInfo>,
    #[serde(default)]
    pub cancelled: bool,
}

// =============================================================================
// Plan Proposal (Agent → Sidebar) & Plan Response (Sidebar → Agent)
// =============================================================================

/// A single step in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A proposed plan with summary and ordered steps (Agent → Sidebar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProposalPayload {
    pub summary: String,
    pub steps: Vec<PlanStep>,
}

/// User response to a plan proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanResponse {
    Confirmed,
    Cancelled,
}

/// Plan response payload with session_id (Sidebar → Agent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanResponsePayload {
    pub session_id: String,
    pub response: PlanResponse,
}

// =============================================================================
// Tool Events (Real-time tool execution state)
// =============================================================================

/// Tool execution event for real-time status in stream chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolEvent {
    /// Tool started executing
    #[serde(rename = "tool_start")]
    Start {
        tool_id: String,
        tool_name: String,
        icon: String,
        summary: String,
    },
    /// Tool waiting for authorization
    #[serde(rename = "tool_auth")]
    Auth {
        tool_id: String,
        request: ToolAuthRequest,
    },
    /// Tool finished executing
    #[serde(rename = "tool_end")]
    End {
        tool_id: String,
        status: ToolEventStatus,
        duration_ms: u64,
        summary: String,
    },
}

/// Tool event completion status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolEventStatus {
    Success,
    Failed,
}

/// Authorization request for a tool execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolAuthRequest {
    /// Tool name ("read", "grep", "bash")
    pub tool: String,
    /// Unique tool call ID (for correlating response)
    pub tool_id: String,
    /// Human-readable path or command
    pub detail: String,
    /// Authorization granularity options
    pub options: Vec<AuthOption>,
}

/// A single authorization option presented to the user
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthOption {
    /// Display text, e.g. "Always allow cargo *"
    pub label: String,
    /// Authorization scope
    pub scope: AuthScope,
}

/// Authorization scope
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthScope {
    Once,
    Session,
    Always,
    Deny,
}

/// Tool authorization response from user (Sidebar → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAuthResponsePayload {
    pub tool_id: String,
    pub option_index: u32,
    pub scope: AuthScope,
}

// =============================================================================
// Chat Message Enum
// =============================================================================

/// Message direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    ToAgent,
    ToSidebar,
}

/// All Chat channel message types (bidirectional)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ChatMessage {
    // ========== Sidebar → Agent ==========
    /// User chat message
    ChatMessage(ChatMessagePayload),
    /// Skill command
    SkillCommand(SkillCommandPayload),
    /// Stop generation
    StopGeneration(StopGenerationPayload),
    /// Cancel request (user interruption)
    Cancel(CancelPayload),
    /// Permission response
    PermissionResponse(PermissionResponsePayload),
    /// Plugin command
    PluginCommand(PluginCommandPayload),
    /// System command
    SystemCommand(SystemCommandPayload),
    /// Browser tool response
    BrowserToolResponse(BrowserToolResponsePayload),
    /// Pick files request (open native file dialog)
    PickFilesRequest(PickFilesRequestPayload),
    /// Plan response (confirm/cancel)
    PlanResponse(PlanResponsePayload),
    /// Tool authorization response
    ToolAuthResponse(ToolAuthResponsePayload),

    // ========== Agent → Sidebar ==========
    /// Stream chunk
    StreamChunk(StreamChunkPayload),
    /// Stream end
    StreamEnd(StreamEndPayload),
    /// Content block
    ContentBlock(ContentBlockPayload),
    /// Permission request
    PermissionRequest(PermissionRequestPayload),
    /// Agent state
    AgentState(AgentStatePayload),
    /// Error
    Error(ErrorPayload),
    /// Account status
    AccountStatus(AccountStatusPayload),
    /// System response
    SystemResponse(SystemResponsePayload),
    /// Browser tool request
    BrowserToolRequest(BrowserToolRequestPayload),
    /// Pick files response (from native file dialog)
    PickFilesResponse(PickFilesResponsePayload),
    /// Plan proposal from agent
    PlanProposal(PlanProposalPayload),
}

impl ChatMessage {
    /// Get message direction
    pub fn direction(&self) -> MessageDirection {
        match self {
            // Sidebar → Agent
            Self::ChatMessage(_) |
            Self::SkillCommand(_) |
            Self::StopGeneration(_) |
            Self::Cancel(_) |
            Self::PermissionResponse(_) |
            Self::PluginCommand(_) |
            Self::SystemCommand(_) |
            Self::BrowserToolResponse(_) |
            Self::PickFilesRequest(_) |
            Self::PlanResponse(_) |
            Self::ToolAuthResponse(_) => MessageDirection::ToAgent,

            // Agent → Sidebar
            Self::StreamChunk(_) |
            Self::StreamEnd(_) |
            Self::ContentBlock(_) |
            Self::PermissionRequest(_) |
            Self::AgentState(_) |
            Self::Error(_) |
            Self::AccountStatus(_) |
            Self::SystemResponse(_) |
            Self::BrowserToolRequest(_) |
            Self::PickFilesResponse(_) |
            Self::PlanProposal(_) => MessageDirection::ToSidebar,
        }
    }

    /// Get session_id if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::ChatMessage(p) => Some(&p.session_id),
            Self::SkillCommand(p) => Some(&p.session_id),
            Self::StopGeneration(p) => Some(&p.session_id),
            Self::Cancel(p) => Some(&p.session_id),
            Self::PermissionResponse(_) => None,
            Self::PluginCommand(_) => None,
            Self::SystemCommand(_) => None,
            Self::BrowserToolResponse(p) => Some(&p.session_id),
            Self::PickFilesRequest(_) => None,
            Self::PlanResponse(p) => Some(&p.session_id),
            Self::ToolAuthResponse(_) => None,
            Self::StreamChunk(_) => None, // New protocol doesn't include session_id
            Self::StreamEnd(p) => Some(&p.session_id),
            Self::ContentBlock(p) => Some(&p.session_id),
            Self::PermissionRequest(p) => Some(&p.session_id),
            Self::AgentState(p) => Some(&p.session_id),
            Self::Error(p) => Some(&p.session_id),
            Self::AccountStatus(_) => None,
            Self::SystemResponse(_) => None,
            Self::BrowserToolRequest(p) => Some(&p.session_id),
            Self::PickFilesResponse(_) => None,
            Self::PlanProposal(_) => None,
        }
    }
}

// =============================================================================
// Re-export for backward compatibility (deprecated, will be removed)
// =============================================================================

/// Deprecated: Use ChatMessage instead
#[deprecated(since = "5.0.0", note = "Use ChatMessage instead")]
pub type InputMessage = ChatMessage;

/// Deprecated: Use ChatMessage instead
#[deprecated(since = "5.0.0", note = "Use ChatMessage instead")]
pub type OutputMessage = ChatMessage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::RequesterType;

    // =========================================================================
    // Direction Tests
    // =========================================================================

    #[test]
    fn test_chat_message_direction_to_agent() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_skill_command_direction_to_agent() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "s1".to_string(),
            skill_name: "test".to_string(),
            args: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_stop_generation_direction_to_agent() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "s1".to_string(),
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_cancel_direction_to_agent() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "s1".to_string(),
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_permission_response_direction_to_agent() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_plugin_command_direction_to_agent() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "p1".to_string(),
            action: PluginAction::Start,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_system_command_direction_to_agent() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            params: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_browser_tool_response_direction_to_agent() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            success: true,
            result: None,
            error: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_stream_chunk_direction_to_sidebar() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hi".to_string(),
            tool_calls: vec![],
            done: false,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_stream_end_direction_to_sidebar() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            metadata: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_content_block_direction_to_sidebar() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "s1".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Markdown,
            content: serde_json::json!("text"),
            metadata: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_permission_request_direction_to_sidebar() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            resource_type: ResourceType::File,
            action: ResourceAction::Read,
            resource: "/path".to_string(),
            requester: Requester {
                requester_type: RequesterType::Agent,
                id: "a1".to_string(),
                name: "Agent".to_string(),
            },
            reason: "Test".to_string(),
            scope: PermissionScope::Once,
            timeout_ms: 60000,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_agent_state_direction_to_sidebar() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "s1".to_string(),
            state: AgentState::Thinking,
            step: None,
            tool: None,
            progress: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_error_direction_to_sidebar() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "s1".to_string(),
            error_id: "e1".to_string(),
            level: ErrorLevel::Error,
            code: "ERR".to_string(),
            message: "Error".to_string(),
            details: None,
            recoverable: false,
            retry_action: None,
            related_request_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_account_status_direction_to_sidebar() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: true,
            account: None,
            plan: None,
            quota: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_system_response_direction_to_sidebar() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            success: true,
            data: None,
            error: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_browser_tool_request_direction_to_sidebar() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            tab_id: None,
            action: BrowserToolAction::Click,
            params: serde_json::json!({}),
            timeout_ms: 30000,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    // =========================================================================
    // Session ID Tests
    // =========================================================================

    #[test]
    fn test_session_id_chat_message() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "test-session".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_skill_command() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "test-session".to_string(),
            skill_name: "test".to_string(),
            args: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stop_generation() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "test-session".to_string(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_cancel() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "test-session".to_string(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_permission_response_none() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_plugin_command_none() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "p1".to_string(),
            action: PluginAction::Stop,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_command_none() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            params: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_browser_tool_response() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            success: true,
            result: None,
            error: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stream_chunk() {
        // New protocol: StreamChunk doesn't have session_id
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello".to_string(),
            tool_calls: vec![],
            done: false,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_stream_end() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "test-session".to_string(),
            stream_id: "st1".to_string(),
            metadata: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_content_block() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "test-session".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Text,
            content: serde_json::json!("text"),
            metadata: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_permission_request() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            resource_type: ResourceType::Network,
            action: ResourceAction::Connect,
            resource: "https://example.com".to_string(),
            requester: Requester {
                requester_type: RequesterType::Skill,
                id: "skill-1".to_string(),
                name: "Web Skill".to_string(),
            },
            reason: "API call".to_string(),
            scope: PermissionScope::Always,
            timeout_ms: default_timeout(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_agent_state() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "test-session".to_string(),
            state: AgentState::Executing,
            step: None,
            tool: None,
            progress: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_error() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "test-session".to_string(),
            error_id: "e1".to_string(),
            level: ErrorLevel::Warning,
            code: "WARN".to_string(),
            message: "Warning".to_string(),
            details: None,
            recoverable: true,
            retry_action: None,
            related_request_id: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_account_status_none() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: false,
            account: None,
            plan: None,
            quota: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_response_none() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            success: false,
            data: None,
            error: Some(SystemError {
                code: "ERR_001".to_string(),
                message: "Failed".to_string(),
            }),
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_browser_tool_request() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            tab_id: None,
            action: BrowserToolAction::Navigate,
            params: serde_json::json!({"url": "https://example.com"}),
            timeout_ms: 30000,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "session-1".to_string(),
            message_id: "msg-1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("chat_message"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_skill_command_serialization() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "session-1".to_string(),
            skill_name: "test_skill".to_string(),
            args: Some(serde_json::json!({"key": "value"})),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("skill_command"));
        assert!(json.contains("test_skill"));
    }

    #[test]
    fn test_stop_generation_serialization() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "session-1".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stop_generation"));
    }

    #[test]
    fn test_cancel_serialization() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "session-1".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"cancel\""));
        assert!(json.contains("session-1"));
    }

    #[test]
    fn test_permission_response_serialization() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: Some(PermissionScope::Session),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("permission_response"));
        assert!(json.contains("granted"));
    }

    #[test]
    fn test_plugin_command_serialization() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "plugin-1".to_string(),
            action: PluginAction::Start,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("plugin_command"));
        assert!(json.contains("plugin-1"));
    }

    #[test]
    fn test_system_command_serialization() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "skills.list".to_string(),
            params: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system_command"));
        assert!(json.contains("skills.list"));
    }

    #[test]
    fn test_stream_chunk_serialization() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello".to_string(),
            tool_calls: vec![],
            done: false,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_stream_end_serialization() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            metadata: Some(StreamMetadata {
                total_tokens: Some(100),
                duration_ms: Some(500),
                model: Some("claude-3".to_string()),
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_end"));
    }

    #[test]
    fn test_content_block_serialization() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "s1".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Markdown,
            content: serde_json::json!({"text": "Hello"}),
            metadata: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("content_block"));
    }

    #[test]
    fn test_permission_request_serialization() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            resource_type: ResourceType::File,
            action: ResourceAction::Read,
            resource: "/path/to/file".to_string(),
            requester: Requester {
                requester_type: RequesterType::Agent,
                id: "agent-1".to_string(),
                name: "Test Agent".to_string(),
            },
            reason: "Need to read config".to_string(),
            scope: PermissionScope::Once,
            timeout_ms: 30000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("permission_request"));
    }

    #[test]
    fn test_agent_state_serialization() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "s1".to_string(),
            state: AgentState::Executing,
            step: Some(StepInfo {
                current: 1,
                total: 5,
                description: Some("Processing".to_string()),
            }),
            tool: Some(ToolInfo {
                name: "file_read".to_string(),
                status: ToolStatus::Running,
                target: Some("/etc/config".to_string()),
            }),
            progress: Some(0.5),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("agent_state"));
    }

    #[test]
    fn test_error_serialization() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "session-1".to_string(),
            error_id: "err-1".to_string(),
            level: ErrorLevel::Error,
            code: "LLM_TIMEOUT".to_string(),
            message: "Request timed out".to_string(),
            details: None,
            recoverable: true,
            retry_action: Some("chat_message".to_string()),
            related_request_id: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("LLM_TIMEOUT"));
    }

    #[test]
    fn test_account_status_serialization() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: true,
            account: Some(AccountInfo {
                id: "user-1".to_string(),
                email: "test@example.com".to_string(),
                name: Some("Test User".to_string()),
            }),
            plan: Some(PlanInfo {
                plan_type: PlanType::Pro,
                name: "Pro Plan".to_string(),
                expires_at: Some("2025-01-01".to_string()),
            }),
            quota: Some(QuotaInfo {
                llm_calls: Some(UsageQuota {
                    used: 100,
                    limit: 1000,
                    resets_at: Some("2024-02-01".to_string()),
                }),
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("account_status"));
    }

    #[test]
    fn test_system_response_serialization() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "skills.list".to_string(),
            success: true,
            data: Some(serde_json::json!(["skill1", "skill2"])),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system_response"));
    }

    #[test]
    fn test_browser_tool_request_serialization() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            tab_id: None,
            action: BrowserToolAction::Click,
            params: serde_json::json!({"selector": "#btn"}),
            timeout_ms: 30000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("browser_tool_request"));
    }

    #[test]
    fn test_browser_tool_response_serialization() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            success: true,
            result: Some(serde_json::json!({"clicked": true})),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("browser_tool_response"));
    }

    // =========================================================================
    // Roundtrip Tests
    // =========================================================================

    #[test]
    fn test_chat_message_roundtrip() {
        let original = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: Some(42),
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::ChatMessage(p) => {
                assert_eq!(p.session_id, "s1");
                assert_eq!(p.content, "Hello");
                assert_eq!(p.tab_id, Some(42));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_stream_chunk_roundtrip() {
        let original = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello world".to_string(),
            tool_calls: vec![],
            done: true,
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::StreamChunk(p) => {
                assert_eq!(p.content, "Hello world");
                assert!(p.done);
                assert!(p.tool_calls.is_empty());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_browser_tool_request_roundtrip() {
        let original = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-123".to_string(),
            session_id: "sess-456".to_string(),
            tab_id: Some(789),
            action: BrowserToolAction::Navigate,
            params: serde_json::json!({"url": "https://example.com"}),
            timeout_ms: 15000,
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::BrowserToolRequest(p) => {
                assert_eq!(p.request_id, "req-123");
                assert_eq!(p.session_id, "sess-456");
                assert_eq!(p.tab_id, Some(789));
                assert_eq!(p.action, BrowserToolAction::Navigate);
                assert_eq!(p.timeout_ms, 15000);
            }
            _ => panic!("Wrong variant"),
        }
    }

    // =========================================================================
    // Default Value Tests
    // =========================================================================

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 60000);
    }

    #[test]
    fn test_default_browser_timeout() {
        assert_eq!(default_browser_timeout(), 30000);
    }
}
