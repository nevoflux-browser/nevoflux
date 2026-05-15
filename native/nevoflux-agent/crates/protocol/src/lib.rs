// crates/protocol/src/lib.rs

//! NevoFlux Protocol - IPC message definitions for Agent communication.
//!
//! This crate defines all message types exchanged between:
//! - Proxy (Native Messaging bridge) and Daemon
//! - MCP Bridge and Daemon
//! - Chat Sidebar and Agent
//!
//! # Architecture
//!
//! Messages are wrapped in envelopes for routing:
//! - [`ProxyEnvelope`] - Messages from Proxy/MCP to Daemon
//! - [`DaemonEnvelope`] - Messages from Daemon to Proxy/MCP
//!
//! # Channels
//!
//! Two channels are supported:
//! - [`Channel::Chat`] - User interaction, streaming responses, permissions
//! - [`Channel::Mcp`] - Browser Use API via MCP protocol
//!
//! # Serialization
//!
//! All types support both JSON and MessagePack serialization via serde.

pub mod canvas_persist;
pub mod canvas_share;
pub mod canvas_tools;
pub mod canvas_video;
pub mod channel;
pub mod chat;
pub mod common;
pub mod envelope;
mod error;
pub mod events;
pub mod extract;
pub mod json_repair;
pub mod mcp;
pub mod subagent;
pub mod tts;

// Re-export canvas share types
pub use canvas_share::{
    CanvasImportRequest, CanvasImportResponse, CanvasShareDeleteRequest, CanvasShareDeleteResponse,
    CanvasShareExtendRequest, CanvasShareExtendResponse, CanvasShareInfo, CanvasShareListRequest,
    CanvasShareListResponse, CanvasShareRequest, CanvasShareResponse,
};

// Re-export canvas tool types
pub use canvas_tools::{
    CanvasToolDeleteRequest, CanvasToolDeleteResponse, CanvasToolError, CanvasToolEvent,
    CanvasToolGetRawRequest, CanvasToolGetRawResponse, CanvasToolInvokeRequest,
    CanvasToolInvokeResponse, CanvasToolListRequest, CanvasToolListResponse, CanvasToolSaveRequest,
    CanvasToolSaveResponse, CanvasToolSummary, CanvasToolValidateRequest,
    CanvasToolValidateResponse,
};

// Re-export main types at crate root
pub use channel::Channel;
pub use envelope::{AuthInfo, DaemonEnvelope, ProxyEnvelope};
pub use error::{ProtocolError, Result};

// Re-export chat messages
pub use chat::{
    AccountStatus, AgentMessage, AgentStateMessage, Artifact, ArtifactComplete, ArtifactDelta,
    ArtifactStart, AuthOption, BrowserToolRequest, BrowserToolResponse, ChatMessage, ContentBlock,
    ErrorMessage, PermissionRequest, PermissionResponse, PlanProposal, PlanResponse, PlanStep,
    PluginCommand, SidebarMessage, SkillCommand, StopGeneration, StreamChunk, StreamEnd,
    SystemCommand, SystemResponse, TabInfo, ThinkingEvent, ToolAuthRequest, ToolAuthResponse,
    ToolEvent,
};

// Re-export MCP messages
pub use mcp::{
    JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse, McpMessage, McpRequest, McpResponse,
    McpSource,
};

// Re-export EventBus types
pub use events::{
    BusEventPayload, DeliveryMode, EventBusDelivery, EventBusRequest, EventBusResponse,
    HistoryQuery, PublishOptions, SubscribeOptions,
};

// Re-export common types
pub use common::{
    tool_icon, AccountInfo, AgentState, Attachment, BashResult, BashStatus, BrowserToolAction,
    BrowserToolError, ContentType, ErrorLevel, FileInfo, GrepMatch, GrepResult, LocalFileRef,
    PermissionScope, PickFilesError, PickFilesRequest, PickFilesResponse, PickerMode, PlanInfo,
    PlanType, PluginAction, QuotaInfo, ReadResult, Requester, RequesterType, ResourceAction,
    ResourceType, StepInfo, StreamFormat, StreamMetadata, SystemError, ToolInfo, ToolStatus,
    UsageQuota,
};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "5.0.0";

/// Get the protocol version
pub fn get_protocol_version() -> &'static str {
    PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(get_protocol_version(), "5.0.0");
        assert_eq!(PROTOCOL_VERSION, "5.0.0");
    }

    #[test]
    fn test_reexports_available() {
        // Verify all re-exports are accessible
        let _: Channel = Channel::Chat;
        let _: AgentState = AgentState::Idle;
        let _: PermissionScope = PermissionScope::Once;
    }
}
