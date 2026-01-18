/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 1: Input (Sidebar → Agent)
//!
//! Messages sent from Chat Sidebar to Native Agent.

use serde::{Deserialize, Serialize};
use crate::common::{Attachment, PermissionScope, PluginAction};

/// Chat message from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagePayload {
    pub session_id: String,
    pub message_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
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

/// System command (structured query/control)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommandPayload {
    pub request_id: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// All Channel 1 message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum InputMessage {
    ChatMessage(ChatMessagePayload),
    SkillCommand(SkillCommandPayload),
    StopGeneration(StopGenerationPayload),
    PermissionResponse(PermissionResponsePayload),
    PluginCommand(PluginCommandPayload),
    SystemCommand(SystemCommandPayload),
}

impl InputMessage {
    /// Get session_id if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::ChatMessage(p) => Some(&p.session_id),
            Self::SkillCommand(p) => Some(&p.session_id),
            Self::StopGeneration(p) => Some(&p.session_id),
            Self::PermissionResponse(_) => None,
            Self::PluginCommand(_) => None,
            Self::SystemCommand(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_serialization() {
        let msg = InputMessage::ChatMessage(ChatMessagePayload {
            session_id: "session-1".to_string(),
            message_id: "msg-1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("chat_message"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_system_command_serialization() {
        let msg = InputMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "skills.list".to_string(),
            params: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system_command"));
        assert!(json.contains("skills.list"));
    }
}
