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
    use crate::common::PluginAction;

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

    #[test]
    fn test_skill_command_serialization() {
        let msg = InputMessage::SkillCommand(SkillCommandPayload {
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
        let msg = InputMessage::StopGeneration(StopGenerationPayload {
            session_id: "session-1".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stop_generation"));
    }

    #[test]
    fn test_permission_response_serialization() {
        let msg = InputMessage::PermissionResponse(PermissionResponsePayload {
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
        let msg = InputMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "plugin-1".to_string(),
            action: PluginAction::Start,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("plugin_command"));
        assert!(json.contains("plugin-1"));
    }

    #[test]
    fn test_session_id_chat_message() {
        let msg = InputMessage::ChatMessage(ChatMessagePayload {
            session_id: "test-session".to_string(),
            message_id: "msg-1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_skill_command() {
        let msg = InputMessage::SkillCommand(SkillCommandPayload {
            session_id: "test-session".to_string(),
            skill_name: "test".to_string(),
            args: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stop_generation() {
        let msg = InputMessage::StopGeneration(StopGenerationPayload {
            session_id: "test-session".to_string(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_permission_response() {
        let msg = InputMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_plugin_command() {
        let msg = InputMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "plugin-1".to_string(),
            action: PluginAction::Stop,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_command() {
        let msg = InputMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            params: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_chat_message_roundtrip() {
        let original = InputMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: InputMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            InputMessage::ChatMessage(p) => {
                assert_eq!(p.session_id, "s1");
                assert_eq!(p.text, "Hello");
            }
            _ => panic!("Wrong variant"),
        }
    }
}
