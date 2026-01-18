/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 2: Output (Agent → Sidebar)
//!
//! Messages sent from Native Agent to Chat Sidebar.

use serde::{Deserialize, Serialize};
use crate::common::{
    AgentState, ContentType, ErrorLevel, PermissionScope, PlanType,
    Requester, ResourceAction, ResourceType, StreamFormat, ToolStatus,
};

/// Stream chunk for streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    pub session_id: String,
    pub stream_id: String,
    pub delta: String,
    #[serde(default)]
    pub format: StreamFormat,
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

/// All Channel 2 message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum OutputMessage {
    StreamChunk(StreamChunkPayload),
    StreamEnd(StreamEndPayload),
    ContentBlock(ContentBlockPayload),
    PermissionRequest(PermissionRequestPayload),
    AgentState(AgentStatePayload),
    Error(ErrorPayload),
    AccountStatus(AccountStatusPayload),
    SystemResponse(SystemResponsePayload),
}

impl OutputMessage {
    /// Get session_id if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::StreamChunk(p) => Some(&p.session_id),
            Self::StreamEnd(p) => Some(&p.session_id),
            Self::ContentBlock(p) => Some(&p.session_id),
            Self::PermissionRequest(p) => Some(&p.session_id),
            Self::AgentState(p) => Some(&p.session_id),
            Self::Error(p) => Some(&p.session_id),
            Self::AccountStatus(_) => None,
            Self::SystemResponse(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_chunk_serialization() {
        let msg = OutputMessage::StreamChunk(StreamChunkPayload {
            session_id: "session-1".to_string(),
            stream_id: "stream-1".to_string(),
            delta: "Hello".to_string(),
            format: StreamFormat::Markdown,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_error_serialization() {
        let msg = OutputMessage::Error(ErrorPayload {
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
}
