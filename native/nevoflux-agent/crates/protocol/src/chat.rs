// crates/protocol/src/chat.rs

//! Chat channel message definitions.
//!
//! Messages exchanged between Chat Sidebar and Agent via the Chat channel.

use crate::canvas_tools::{
    CanvasToolDeleteResponse, CanvasToolEvent, CanvasToolGetRawResponse, CanvasToolInvokeRequest,
    CanvasToolInvokeResponse, CanvasToolListRequest, CanvasToolListResponse,
    CanvasToolSaveResponse, CanvasToolValidateResponse,
};
use crate::common::*;
use crate::events::{EventBusDelivery, EventBusRequest, EventBusResponse};
use serde::{Deserialize, Serialize};

// ============================================================================
// Sidebar → Agent Messages
// ============================================================================

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

/// User chat message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Session ID
    pub session_id: String,
    /// Message ID (UUID)
    pub message_id: String,
    /// Message text
    pub text: String,
    /// Attachments
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Current active tab ID.
    pub tab_id: Option<i64>,
    /// List of all available tabs with their metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_ids: Vec<TabInfo>,
}

/// Skill command
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillCommand {
    /// Session ID
    pub session_id: String,
    /// Skill name
    pub skill_name: String,
    /// Skill arguments
    pub args: Option<serde_json::Value>,
}

/// Stop generation request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopGeneration {
    /// Session ID
    pub session_id: String,
}

/// Permission response from user
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionResponse {
    /// Request ID
    pub request_id: String,
    /// Whether permission was granted
    pub granted: bool,
    /// Authorization scope
    pub scope: Option<PermissionScope>,
}

/// Plugin command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Plugin ID
    pub plugin_id: String,
    /// Action to perform
    pub action: PluginAction,
}

/// System command
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemCommand {
    /// Request ID
    pub request_id: String,
    /// Command name
    pub command: String,
    /// Command parameters
    pub params: Option<serde_json::Value>,
}

/// Browser tool response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserToolResponse {
    /// Request ID
    pub request_id: String,
    /// Session ID
    pub session_id: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Result data
    pub result: Option<serde_json::Value>,
    /// Error information
    pub error: Option<BrowserToolError>,
}

// ============================================================================
// Agent → Sidebar Messages
// ============================================================================

/// Streaming response chunk
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Session ID
    pub session_id: String,
    /// Stream ID
    pub stream_id: String,
    /// Incremental content
    pub delta: String,
    /// Content format
    pub format: StreamFormat,
    /// Optional tool event for real-time sidebar updates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<ToolEvent>,
    /// Optional thinking event for real-time reasoning display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_event: Option<ThinkingEvent>,
}

/// Streaming response end marker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamEnd {
    /// Session ID
    pub session_id: String,
    /// Stream ID
    pub stream_id: String,
    /// Stream metadata
    pub metadata: Option<StreamMetadata>,
}

/// Content block (code, image, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlock {
    /// Session ID
    pub session_id: String,
    /// Block ID
    pub block_id: String,
    /// Content type
    pub content_type: ContentType,
    /// Content data
    pub content: serde_json::Value,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Permission request to user
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Request ID
    pub request_id: String,
    /// Session ID
    pub session_id: String,
    /// Resource type
    pub resource_type: ResourceType,
    /// Action requested
    pub action: ResourceAction,
    /// Resource identifier
    pub resource: String,
    /// Who is requesting
    pub requester: Requester,
    /// Reason for the request
    pub reason: String,
    /// Suggested scope
    pub scope: PermissionScope,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

/// Agent state update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStateMessage {
    /// Session ID
    pub session_id: String,
    /// Current state
    pub state: AgentState,
    /// Step information
    pub step: Option<StepInfo>,
    /// Tool information
    pub tool: Option<ToolInfo>,
    /// Progress (0.0 - 1.0)
    pub progress: Option<f32>,
}

/// Error notification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorMessage {
    /// Session ID
    pub session_id: String,
    /// Error ID
    pub error_id: String,
    /// Error level
    pub level: ErrorLevel,
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Additional details
    pub details: Option<serde_json::Value>,
    /// Whether the error is recoverable
    pub recoverable: bool,
    /// Suggested retry action
    pub retry_action: Option<String>,
    /// Related request ID
    pub related_request_id: Option<String>,
}

/// Account status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountStatus {
    /// Whether user is logged in
    pub logged_in: bool,
    /// Account information
    pub account: Option<AccountInfo>,
    /// Plan information
    pub plan: Option<PlanInfo>,
    /// Quota information
    pub quota: Option<QuotaInfo>,
}

/// System command response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemResponse {
    /// Request ID
    pub request_id: String,
    /// Command that was executed
    pub command: String,
    /// Whether the command succeeded
    pub success: bool,
    /// Response data
    pub data: Option<serde_json::Value>,
    /// Error information
    pub error: Option<SystemError>,
}

/// Browser tool request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserToolRequest {
    /// Request ID
    pub request_id: String,
    /// Session ID
    pub session_id: String,
    /// Tab ID
    pub tab_id: Option<i64>,
    /// Browser action
    pub action: BrowserToolAction,
    /// Action parameters
    pub params: serde_json::Value,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

// ============================================================================
// Plan Types
// ============================================================================

/// A single step in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Description of what this step does.
    pub description: String,
    /// Optional model override for this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A proposed plan with a summary and ordered steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProposal {
    /// High-level summary of the plan.
    pub summary: String,
    /// Ordered list of steps.
    pub steps: Vec<PlanStep>,
}

/// User response to a plan proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanResponse {
    /// The user confirmed the plan.
    Confirmed,
    /// The user cancelled the plan.
    Cancelled,
}

// ============================================================================
// Artifact Types
// ============================================================================

/// An artifact created by the agent (HTML, code, document, etc.)
///
/// This is the internal representation used throughout the WASM agent pipeline.
/// For the wire protocol (server → sidebar), artifacts are streamed using
/// [`ArtifactStart`], [`ArtifactDelta`], and [`ArtifactComplete`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique artifact ID (UUID).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// MIME type (e.g., "text/html", "text/markdown", "project").
    pub content_type: String,
    /// Brief description of what this artifact contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The full artifact content (single-file artifacts).
    pub content: String,
    /// Multi-file project: map of file paths to content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<std::collections::HashMap<String, String>>,
    /// Multi-file project: entry point file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Whether the artifact has been saved to My Canvas.
    /// Defaults to `false` for wire-compat with older senders.
    #[serde(default)]
    pub is_persistent: bool,
}

/// Sent when an artifact begins streaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactStart {
    /// Unique artifact ID.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// MIME type (e.g., "text/html", "project").
    pub content_type: String,
    /// Brief description of what this artifact contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Multi-file project: map of file paths to content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<std::collections::HashMap<String, String>>,
    /// Multi-file project: entry point file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Whether the artifact has been saved to My Canvas.
    /// Defaults to `false` for wire-compat with older senders.
    #[serde(default)]
    pub is_persistent: bool,
}

/// A chunk of artifact content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDelta {
    /// Artifact ID (matches the ArtifactStart).
    pub id: String,
    /// Content chunk.
    pub delta: String,
}

/// Sent when artifact streaming is complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactComplete {
    /// Artifact ID.
    pub id: String,
}

// ============================================================================
// Tool Event Types
// ============================================================================

/// Tool execution event for real-time sidebar updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
        status: ToolStatus,
        duration_ms: u64,
        summary: String,
    },
}

/// Thinking/reasoning event for real-time display in stream chunks.
/// Parallel to `ToolEvent` but for LLM reasoning content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingEvent {
    /// A new thinking block has started
    #[serde(rename = "thinking_start")]
    Start { thinking_id: String },
    /// Incremental reasoning content
    #[serde(rename = "thinking_delta")]
    Delta {
        thinking_id: String,
        content: String,
    },
    /// Thinking block has completed
    #[serde(rename = "thinking_end")]
    End {
        thinking_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
}

/// Authorization request sent to sidebar when a tool needs permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAuthRequest {
    /// Tool name: "read", "grep", "bash"
    pub tool: String,
    /// Human-readable detail (path or command)
    pub detail: String,
    /// Authorization options for the user to choose from
    pub options: Vec<AuthOption>,
}

/// A single authorization option presented to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOption {
    /// Display text, e.g. "Always allow cargo *"
    pub label: String,
    /// Scope: "once", "session", "always"
    pub scope: String,
}

/// User's response to a tool authorization request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAuthResponse {
    /// Tool ID correlating to the original ToolEvent::Auth
    pub tool_id: String,
    /// Index of the selected option
    pub option_index: usize,
    /// Whether the user granted access
    pub granted: bool,
}

// ============================================================================
// Tagged Message Enums (for serialization with type field)
// ============================================================================

/// All messages from Sidebar to Agent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum SidebarMessage {
    ChatMessage(ChatMessage),
    SkillCommand(SkillCommand),
    StopGeneration(StopGeneration),
    PermissionResponse(PermissionResponse),
    PluginCommand(PluginCommand),
    SystemCommand(SystemCommand),
    BrowserToolResponse(BrowserToolResponse),
    PlanResponse(PlanResponse),
    ToolAuthResponse(ToolAuthResponse),
    /// EventBus request (subscribe/unsubscribe/publish/history)
    EventsRequest(EventBusRequest),
    /// Canvas tool invocation request
    CanvasToolInvoke(CanvasToolInvokeRequest),
    /// Canvas tool list request
    CanvasToolList(CanvasToolListRequest),
}

/// All messages from Agent to Sidebar
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AgentMessage {
    StreamChunk(StreamChunk),
    StreamEnd(StreamEnd),
    ContentBlock(ContentBlock),
    PermissionRequest(PermissionRequest),
    AgentState(AgentStateMessage),
    Error(ErrorMessage),
    AccountStatus(AccountStatus),
    SystemResponse(SystemResponse),
    BrowserToolRequest(BrowserToolRequest),
    PlanProposal(PlanProposal),
    ArtifactStart(ArtifactStart),
    ArtifactDelta(ArtifactDelta),
    ArtifactComplete(ArtifactComplete),
    /// EventBus response (one-shot reply)
    EventsResponse(EventBusResponse),
    /// EventBus push delivery (async event)
    EventsDelivery(EventBusDelivery),
    /// Canvas tool invocation response
    CanvasToolInvokeResponse(CanvasToolInvokeResponse),
    /// Canvas tool list response
    CanvasToolListResponse(CanvasToolListResponse),
    CanvasToolGetRawResponse(CanvasToolGetRawResponse),
    CanvasToolSaveResponse(CanvasToolSaveResponse),
    CanvasToolDeleteResponse(CanvasToolDeleteResponse),
    CanvasToolValidateResponse(CanvasToolValidateResponse),
    /// Canvas tool streaming event
    CanvasToolEvent(CanvasToolEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage {
            session_id: "sess-001".into(),
            message_id: "msg-001".into(),
            text: "Hello, Agent!".into(),
            attachments: vec![],
            tab_id: Some(123),
            tab_ids: vec![],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"session_id\":\"sess-001\""));

        let decoded: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_skill_command_serialization() {
        let cmd = SkillCommand {
            session_id: "sess-001".into(),
            skill_name: "web_search".into(),
            args: Some(serde_json::json!({"query": "rust async"})),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let decoded: SkillCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, decoded);
    }

    #[test]
    fn test_permission_response_serialization() {
        let resp = PermissionResponse {
            request_id: "perm-001".into(),
            granted: true,
            scope: Some(PermissionScope::Session),
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"granted\":true"));
        assert!(json.contains("\"scope\":\"session\""));
    }

    #[test]
    fn test_sidebar_message_tagged_serialization() {
        let msg = SidebarMessage::ChatMessage(ChatMessage {
            session_id: "sess-001".into(),
            message_id: "msg-001".into(),
            text: "Hello".into(),
            attachments: vec![],
            tab_id: None,
            tab_ids: vec![],
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"chat_message\""));
        assert!(json.contains("\"payload\""));

        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, SidebarMessage::ChatMessage(_)));
    }

    #[test]
    fn test_system_command_serialization() {
        let cmd = SystemCommand {
            request_id: "sys-001".into(),
            command: "mode.switch".into(),
            params: Some(serde_json::json!({"mode": "browser"})),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let decoded: SystemCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, decoded);
    }

    #[test]
    fn test_agent_message_tagged_serialization() {
        let msg = AgentMessage::StreamChunk(StreamChunk {
            session_id: "sess-001".into(),
            stream_id: "stream-001".into(),
            delta: "Hello".into(),
            format: StreamFormat::Markdown,
            event: None,
            thinking_event: None,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"stream_chunk\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::StreamChunk(_)));
    }

    #[test]
    fn test_permission_request_serialization() {
        let req = PermissionRequest {
            request_id: "perm-001".into(),
            session_id: "sess-001".into(),
            resource_type: ResourceType::File,
            action: ResourceAction::Read,
            resource: "/home/user/file.txt".into(),
            requester: Requester {
                requester_type: RequesterType::Agent,
                id: "agent-001".into(),
                name: "NevoFlux Agent".into(),
            },
            reason: "Reading configuration".into(),
            scope: PermissionScope::Session,
            timeout_ms: 60000,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: PermissionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_browser_tool_request_serialization() {
        let req = BrowserToolRequest {
            request_id: "bt-001".into(),
            session_id: "sess-001".into(),
            tab_id: Some(123),
            action: BrowserToolAction::Navigate,
            params: serde_json::json!({"url": "https://github.com"}),
            timeout_ms: 30000,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"action\":\"navigate\""));

        let decoded: BrowserToolRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_content_block_a2ui_serialization() {
        // Test A2UI content block for structured UI components
        let block = ContentBlock {
            session_id: "sess-001".into(),
            block_id: "block-001".into(),
            content_type: ContentType::A2ui,
            content: serde_json::json!({
                "component": "file_tree",
                "props": {
                    "root": "/home/user/project",
                    "files": [
                        {"name": "src", "type": "directory"},
                        {"name": "Cargo.toml", "type": "file"}
                    ]
                }
            }),
            metadata: Some(serde_json::json!({
                "interactive": true,
                "expandable": true
            })),
        };

        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"content_type\":\"a2ui\""));
        assert!(json.contains("\"component\":\"file_tree\""));

        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn test_content_block_a2ui_button_component() {
        // Test A2UI button component
        let block = ContentBlock {
            session_id: "sess-001".into(),
            block_id: "block-002".into(),
            content_type: ContentType::A2ui,
            content: serde_json::json!({
                "component": "action_buttons",
                "props": {
                    "buttons": [
                        {"label": "Approve", "action": "approve", "style": "primary"},
                        {"label": "Reject", "action": "reject", "style": "danger"}
                    ]
                }
            }),
            metadata: None,
        };

        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
        assert_eq!(decoded.content_type, ContentType::A2ui);
    }

    #[test]
    fn test_content_block_a2ui_progress_component() {
        // Test A2UI progress component
        let block = ContentBlock {
            session_id: "sess-001".into(),
            block_id: "block-003".into(),
            content_type: ContentType::A2ui,
            content: serde_json::json!({
                "component": "progress_indicator",
                "props": {
                    "current": 3,
                    "total": 10,
                    "label": "Processing files...",
                    "items": [
                        {"name": "file1.rs", "status": "complete"},
                        {"name": "file2.rs", "status": "complete"},
                        {"name": "file3.rs", "status": "in_progress"},
                        {"name": "file4.rs", "status": "pending"}
                    ]
                }
            }),
            metadata: Some(serde_json::json!({
                "auto_update": true,
                "update_interval_ms": 500
            })),
        };

        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block.block_id, decoded.block_id);
        assert!(json.contains("\"progress_indicator\""));
    }

    #[test]
    fn test_agent_message_content_block_a2ui() {
        // Test A2UI content in AgentMessage envelope
        let msg = AgentMessage::ContentBlock(ContentBlock {
            session_id: "sess-001".into(),
            block_id: "block-004".into(),
            content_type: ContentType::A2ui,
            content: serde_json::json!({
                "component": "code_diff",
                "props": {
                    "language": "rust",
                    "original": "fn old() {}",
                    "modified": "fn new() {}"
                }
            }),
            metadata: None,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"content_block\""));
        assert!(json.contains("\"content_type\":\"a2ui\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        if let AgentMessage::ContentBlock(block) = decoded {
            assert_eq!(block.content_type, ContentType::A2ui);
        } else {
            panic!("Expected ContentBlock variant");
        }
    }

    // ====================================================================
    // Plan types tests
    // ====================================================================

    #[test]
    fn test_plan_step_serialization_roundtrip() {
        let step = PlanStep {
            description: "Navigate to the settings page".into(),
            model: Some("gpt-4o".into()),
        };

        let json = serde_json::to_string(&step).unwrap();
        let decoded: PlanStep = serde_json::from_str(&json).unwrap();
        assert_eq!(step, decoded);
    }

    #[test]
    fn test_plan_step_model_none_skipped() {
        let step = PlanStep {
            description: "Click the button".into(),
            model: None,
        };

        let json = serde_json::to_string(&step).unwrap();
        assert!(!json.contains("model"));

        let decoded: PlanStep = serde_json::from_str(&json).unwrap();
        assert_eq!(step, decoded);
    }

    #[test]
    fn test_plan_proposal_serialization_roundtrip() {
        let proposal = PlanProposal {
            summary: "Automate login flow".into(),
            steps: vec![
                PlanStep {
                    description: "Open browser".into(),
                    model: None,
                },
                PlanStep {
                    description: "Enter credentials".into(),
                    model: Some("gpt-4o".into()),
                },
                PlanStep {
                    description: "Click submit".into(),
                    model: None,
                },
            ],
        };

        let json = serde_json::to_string(&proposal).unwrap();
        let decoded: PlanProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(proposal, decoded);
    }

    #[test]
    fn test_plan_proposal_empty_steps() {
        let proposal = PlanProposal {
            summary: "Empty plan".into(),
            steps: vec![],
        };

        let json = serde_json::to_string(&proposal).unwrap();
        let decoded: PlanProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(proposal, decoded);
        assert!(decoded.steps.is_empty());
    }

    #[test]
    fn test_plan_response_serialization() {
        let confirmed = PlanResponse::Confirmed;
        let json = serde_json::to_string(&confirmed).unwrap();
        assert_eq!(json, "\"confirmed\"");
        let decoded: PlanResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, PlanResponse::Confirmed);

        let cancelled = PlanResponse::Cancelled;
        let json = serde_json::to_string(&cancelled).unwrap();
        assert_eq!(json, "\"cancelled\"");
        let decoded: PlanResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, PlanResponse::Cancelled);
    }

    #[test]
    fn test_plan_response_confirmed_serialization() {
        let response = PlanResponse::Confirmed;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, "\"confirmed\"");
        let decoded: PlanResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, PlanResponse::Confirmed);
    }

    #[test]
    fn test_plan_response_cancelled_serialization() {
        let response = PlanResponse::Cancelled;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, "\"cancelled\"");
        let decoded: PlanResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, PlanResponse::Cancelled);
    }

    #[test]
    fn test_agent_message_plan_proposal_tagged() {
        let msg = AgentMessage::PlanProposal(PlanProposal {
            summary: "Test plan".into(),
            steps: vec![PlanStep {
                description: "Step one".into(),
                model: None,
            }],
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"plan_proposal\""));
        assert!(json.contains("\"payload\""));
        assert!(json.contains("\"summary\":\"Test plan\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::PlanProposal(_)));
    }

    #[test]
    fn test_sidebar_message_plan_response_tagged() {
        let msg = SidebarMessage::PlanResponse(PlanResponse::Confirmed);

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"plan_response\""));
        assert!(json.contains("\"payload\":\"confirmed\""));

        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            decoded,
            SidebarMessage::PlanResponse(PlanResponse::Confirmed)
        ));

        let msg2 = SidebarMessage::PlanResponse(PlanResponse::Cancelled);
        let json2 = serde_json::to_string(&msg2).unwrap();
        assert!(json2.contains("\"payload\":\"cancelled\""));

        let decoded2: SidebarMessage = serde_json::from_str(&json2).unwrap();
        assert!(matches!(
            decoded2,
            SidebarMessage::PlanResponse(PlanResponse::Cancelled)
        ));
    }

    // ====================================================================
    // ToolEvent / ToolAuth tests
    // ====================================================================

    #[test]
    fn test_tool_event_start_roundtrip() {
        let event = ToolEvent::Start {
            tool_id: "t-001".into(),
            tool_name: "bash".into(),
            icon: "\u{1F4BB}".into(),
            summary: "Running cargo test".into(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tool_start\""));
        assert!(json.contains("\"tool_id\":\"t-001\""));

        let decoded: ToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_tool_event_auth_roundtrip() {
        let event = ToolEvent::Auth {
            tool_id: "t-002".into(),
            request: ToolAuthRequest {
                tool: "bash".into(),
                detail: "cargo build --release".into(),
                options: vec![
                    AuthOption {
                        label: "Allow once".into(),
                        scope: "once".into(),
                    },
                    AuthOption {
                        label: "Always allow cargo *".into(),
                        scope: "always".into(),
                    },
                ],
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tool_auth\""));
        assert!(json.contains("\"tool\":\"bash\""));

        let decoded: ToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_tool_event_end_roundtrip() {
        let event = ToolEvent::End {
            tool_id: "t-003".into(),
            status: ToolStatus::Success,
            duration_ms: 1234,
            summary: "Completed successfully".into(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tool_end\""));
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"duration_ms\":1234"));

        let decoded: ToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_stream_chunk_without_event_omits_field() {
        let chunk = StreamChunk {
            session_id: "sess-001".into(),
            stream_id: "stream-001".into(),
            delta: "Hello".into(),
            format: StreamFormat::Markdown,
            event: None,
            thinking_event: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(!json.contains("\"event\""));

        let decoded: StreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(chunk, decoded);
    }

    #[test]
    fn test_stream_chunk_with_event_includes_field() {
        let chunk = StreamChunk {
            session_id: "sess-001".into(),
            stream_id: "stream-001".into(),
            delta: "".into(),
            format: StreamFormat::Markdown,
            event: Some(ToolEvent::Start {
                tool_id: "t-010".into(),
                tool_name: "grep".into(),
                icon: "\u{1F50D}".into(),
                summary: "Searching for pattern".into(),
            }),
            thinking_event: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"event\""));
        assert!(json.contains("\"tool_start\""));

        let decoded: StreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(chunk, decoded);
    }

    #[test]
    fn test_tool_auth_response_roundtrip() {
        let resp = ToolAuthResponse {
            tool_id: "t-002".into(),
            option_index: 1,
            granted: true,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"tool_id\":\"t-002\""));
        assert!(json.contains("\"option_index\":1"));
        assert!(json.contains("\"granted\":true"));

        let decoded: ToolAuthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_sidebar_message_tool_auth_response_tagged() {
        let msg = SidebarMessage::ToolAuthResponse(ToolAuthResponse {
            tool_id: "t-005".into(),
            option_index: 0,
            granted: false,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"tool_auth_response\""));
        assert!(json.contains("\"payload\""));

        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, SidebarMessage::ToolAuthResponse(_)));
    }

    // ====================================================================
    // Artifact tests
    // ====================================================================

    #[test]
    fn test_artifact_serialization_roundtrip() {
        let artifact = Artifact {
            id: "art-001".into(),
            title: "My Dashboard".into(),
            content_type: "text/html".into(),
            description: None,
            content: "<html><body><h1>Hello</h1></body></html>".into(),
            files: None,
            entry: None,
            is_persistent: false,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"id\":\"art-001\""));
        assert!(json.contains("\"content_type\":\"text/html\""));

        let decoded: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(artifact, decoded);
    }

    #[test]
    fn test_agent_message_artifact_start_tagged() {
        let msg = AgentMessage::ArtifactStart(ArtifactStart {
            id: "art-002".into(),
            title: "Report".into(),
            content_type: "text/markdown".into(),
            description: None,
            files: None,
            entry: None,
            is_persistent: false,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"artifact_start\""));
        assert!(json.contains("\"payload\""));
        assert!(json.contains("\"title\":\"Report\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::ArtifactStart(_)));
        if let AgentMessage::ArtifactStart(a) = decoded {
            assert_eq!(a.id, "art-002");
            assert_eq!(a.content_type, "text/markdown");
        }
    }

    #[test]
    fn test_agent_message_artifact_delta_tagged() {
        let msg = AgentMessage::ArtifactDelta(ArtifactDelta {
            id: "art-002".into(),
            delta: "<h1>Hello</h1>".into(),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"artifact_delta\""));
        assert!(json.contains("\"payload\""));
        assert!(json.contains("\"delta\":\"<h1>Hello</h1>\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::ArtifactDelta(_)));
        if let AgentMessage::ArtifactDelta(d) = decoded {
            assert_eq!(d.id, "art-002");
            assert_eq!(d.delta, "<h1>Hello</h1>");
        }
    }

    #[test]
    fn test_agent_message_artifact_complete_tagged() {
        let msg = AgentMessage::ArtifactComplete(ArtifactComplete {
            id: "art-002".into(),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"artifact_complete\""));
        assert!(json.contains("\"payload\""));

        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::ArtifactComplete(_)));
        if let AgentMessage::ArtifactComplete(c) = decoded {
            assert_eq!(c.id, "art-002");
        }
    }

    // ====================================================================
    // EventBus integration tests
    // ====================================================================

    #[test]
    fn test_sidebar_message_events_request_serialization() {
        use crate::events::*;
        let msg = SidebarMessage::EventsRequest(EventBusRequest::Subscribe(SubscribeOptions {
            patterns: vec!["session:*:notification".into()],
            replay_sticky: true,
            buffer_size: 256,
        }));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"events_request\""));
        assert!(json.contains("\"action\":\"subscribe\""));
        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_agent_message_events_response_serialization() {
        use crate::events::*;
        let msg = AgentMessage::EventsResponse(EventBusResponse::Subscribed {
            subscription_id: "sub-001".into(),
            patterns: vec!["session:*:notification".into()],
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"events_response\""));
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_agent_message_events_delivery_serialization() {
        use crate::events::*;
        let msg = AgentMessage::EventsDelivery(EventBusDelivery {
            subscription_id: "sub-001".into(),
            event: BusEventPayload {
                event_id: "evt-001".into(),
                topic: "task:status".into(),
                payload: serde_json::json!({"done": true}),
                delivery: DeliveryMode::Ephemeral,
                publisher: "agent:planner".into(),
                timestamp_ms: 1700000000000,
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"events_delivery\""));
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    // ====================================================================
    // Canvas Tool Whitelist tests
    // ====================================================================

    #[test]
    fn test_canvas_tool_invoke_request_roundtrip() {
        use std::collections::HashMap;
        let mut params = HashMap::new();
        params.insert("file".to_string(), "main.rs".to_string());
        let req = CanvasToolInvokeRequest {
            tool_name: "cargo_test".into(),
            params,
            args: Some(vec!["--verbose".into()]),
            session_id: "sess-100".into(),
            call_id: Some("call-100".into()),
            timeout_ms: Some(60000),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: CanvasToolInvokeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_canvas_tool_list_request_roundtrip() {
        let req = CanvasToolListRequest {
            include_disabled: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: CanvasToolListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_canvas_tool_invoke_response_roundtrip() {
        let resp = CanvasToolInvokeResponse {
            tool_name: "cargo_test".into(),
            success: true,
            stdout: Some("test result: ok".into()),
            stderr: None,
            exit_code: Some(0),
            error: None,
            duration_ms: 1500,
            call_id: "call-001".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasToolInvokeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_canvas_tool_list_response_roundtrip() {
        use crate::canvas_tools::CanvasToolSummary;
        let resp = CanvasToolListResponse {
            tools: vec![
                CanvasToolSummary {
                    name: "cargo_test".into(),
                    description: Some("Run cargo tests".into()),
                    kind: "shell".into(),
                    args_mode: Some("params".into()),
                    enabled: true,
                    source: "builtin".into(),
                    origin_source: "builtin".into(),
                    is_override: false,
                },
                CanvasToolSummary {
                    name: "eslint".into(),
                    description: None,
                    kind: "shell".into(),
                    args_mode: None,
                    enabled: false,
                    source: "user".into(),
                    origin_source: "user".into(),
                    is_override: false,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasToolListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
        assert_eq!(decoded.tools.len(), 2);
    }

    #[test]
    fn test_canvas_tool_event_variants_roundtrip() {
        // Started
        let started = CanvasToolEvent::Started {
            call_id: "call-200".into(),
            tool_name: "build".into(),
        };
        let json = serde_json::to_string(&started).unwrap();
        assert!(json.contains("\"event_type\":\"started\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(started, decoded);

        // Stdout
        let stdout = CanvasToolEvent::Stdout {
            call_id: "call-200".into(),
            data: "Compiling...\n".into(),
        };
        let json = serde_json::to_string(&stdout).unwrap();
        assert!(json.contains("\"event_type\":\"stdout\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(stdout, decoded);

        // Stderr
        let stderr = CanvasToolEvent::Stderr {
            call_id: "call-200".into(),
            data: "warning: unused variable\n".into(),
        };
        let json = serde_json::to_string(&stderr).unwrap();
        assert!(json.contains("\"event_type\":\"stderr\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(stderr, decoded);

        // Progress
        let progress = CanvasToolEvent::Progress {
            call_id: "call-200".into(),
            progress: 0.5,
            message: Some("halfway".into()),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"event_type\":\"progress\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(progress, decoded);

        // Finished (success)
        let finished = CanvasToolEvent::Finished {
            call_id: "call-200".into(),
            success: true,
            exit_code: Some(0),
            duration_ms: 5000,
        };
        let json = serde_json::to_string(&finished).unwrap();
        assert!(json.contains("\"event_type\":\"finished\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(finished, decoded);

        // Finished (failure)
        let failed = CanvasToolEvent::Finished {
            call_id: "call-200".into(),
            success: false,
            exit_code: Some(1),
            duration_ms: 100,
        };
        let json = serde_json::to_string(&failed).unwrap();
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(failed, decoded);

        // Error
        let error = CanvasToolEvent::Error {
            call_id: "call-200".into(),
            error: "Process killed by signal 9".into(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"event_type\":\"error\""));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(error, decoded);
    }

    #[test]
    fn test_sidebar_message_canvas_tool_invoke_tagged() {
        use std::collections::HashMap;
        let msg = SidebarMessage::CanvasToolInvoke(CanvasToolInvokeRequest {
            tool_name: "run_tests".into(),
            params: HashMap::new(),
            args: None,
            session_id: "sess-200".into(),
            call_id: None,
            timeout_ms: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"canvas_tool_invoke\""));
        assert!(json.contains("\"payload\""));
        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, SidebarMessage::CanvasToolInvoke(_)));
    }

    #[test]
    fn test_sidebar_message_canvas_tool_list_tagged() {
        let msg = SidebarMessage::CanvasToolList(CanvasToolListRequest {
            include_disabled: false,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"canvas_tool_list\""));
        assert!(json.contains("\"payload\""));
        let decoded: SidebarMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, SidebarMessage::CanvasToolList(_)));
    }

    #[test]
    fn test_agent_message_canvas_tool_event_tagged() {
        let msg = AgentMessage::CanvasToolEvent(CanvasToolEvent::Started {
            call_id: "call-300".into(),
            tool_name: "lint".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"canvas_tool_event\""));
        assert!(json.contains("\"payload\""));
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::CanvasToolEvent(_)));
    }

    #[test]
    fn test_agent_message_canvas_tool_invoke_response_tagged() {
        let msg = AgentMessage::CanvasToolInvokeResponse(CanvasToolInvokeResponse {
            tool_name: "build".into(),
            success: false,
            stdout: None,
            stderr: Some("error[E0308]: mismatched types".into()),
            exit_code: Some(1),
            error: Some("Compilation failed".into()),
            duration_ms: 3200,
            call_id: "call-301".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"canvas_tool_invoke_response\""));
        assert!(json.contains("\"payload\""));
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::CanvasToolInvokeResponse(_)));
        if let AgentMessage::CanvasToolInvokeResponse(r) = decoded {
            assert_eq!(r.success, false);
            assert_eq!(r.exit_code, Some(1));
        }
    }

    #[test]
    fn test_agent_message_canvas_tool_list_response_tagged() {
        use crate::canvas_tools::CanvasToolSummary;
        let msg = AgentMessage::CanvasToolListResponse(CanvasToolListResponse {
            tools: vec![CanvasToolSummary {
                name: "cargo_test".into(),
                description: Some("Run cargo tests".into()),
                kind: "shell".into(),
                args_mode: Some("params".into()),
                enabled: true,
                source: "builtin".into(),
                origin_source: "builtin".into(),
                is_override: false,
            }],
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"canvas_tool_list_response\""));
        assert!(json.contains("\"payload\""));
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AgentMessage::CanvasToolListResponse(_)));
        if let AgentMessage::CanvasToolListResponse(r) = decoded {
            assert_eq!(r.tools.len(), 1);
            assert_eq!(r.tools[0].name, "cargo_test");
        }
    }
}
