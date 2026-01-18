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
    use crate::common::{RequesterType, AgentState, ToolStatus, ContentType};

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

    #[test]
    fn test_stream_end_serialization() {
        let msg = OutputMessage::StreamEnd(StreamEndPayload {
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
        let msg = OutputMessage::ContentBlock(ContentBlockPayload {
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
        let msg = OutputMessage::PermissionRequest(PermissionRequestPayload {
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
        let msg = OutputMessage::AgentState(AgentStatePayload {
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
    fn test_account_status_serialization() {
        let msg = OutputMessage::AccountStatus(AccountStatusPayload {
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
        let msg = OutputMessage::SystemResponse(SystemResponsePayload {
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
    fn test_session_id_stream_chunk() {
        let msg = OutputMessage::StreamChunk(StreamChunkPayload {
            session_id: "test-session".to_string(),
            stream_id: "st1".to_string(),
            delta: "Hello".to_string(),
            format: StreamFormat::default(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stream_end() {
        let msg = OutputMessage::StreamEnd(StreamEndPayload {
            session_id: "test-session".to_string(),
            stream_id: "st1".to_string(),
            metadata: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_content_block() {
        let msg = OutputMessage::ContentBlock(ContentBlockPayload {
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
        let msg = OutputMessage::PermissionRequest(PermissionRequestPayload {
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
        let msg = OutputMessage::AgentState(AgentStatePayload {
            session_id: "test-session".to_string(),
            state: AgentState::Thinking,
            step: None,
            tool: None,
            progress: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_error() {
        let msg = OutputMessage::Error(ErrorPayload {
            session_id: "test-session".to_string(),
            error_id: "err-1".to_string(),
            level: ErrorLevel::Warning,
            code: "WARN_001".to_string(),
            message: "Warning".to_string(),
            details: None,
            recoverable: false,
            retry_action: None,
            related_request_id: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_account_status() {
        let msg = OutputMessage::AccountStatus(AccountStatusPayload {
            logged_in: false,
            account: None,
            plan: None,
            quota: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_response() {
        let msg = OutputMessage::SystemResponse(SystemResponsePayload {
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
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 60000);
    }
}
