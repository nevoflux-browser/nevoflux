// crates/protocol/src/channel.rs

//! Channel definitions for message routing.

use serde::{Deserialize, Serialize};

/// Communication channel type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Chat channel - user interaction, streaming responses, permissions
    #[default]
    Chat,
    /// MCP channel - Browser Use API via MCP protocol
    Mcp,
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Channel::Chat => write!(f, "chat"),
            Channel::Mcp => write!(f, "mcp"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_serialization_json() {
        let chat = Channel::Chat;
        let json = serde_json::to_string(&chat).unwrap();
        assert_eq!(json, "\"chat\"");

        let mcp = Channel::Mcp;
        let json = serde_json::to_string(&mcp).unwrap();
        assert_eq!(json, "\"mcp\"");
    }

    #[test]
    fn test_channel_deserialization_json() {
        let chat: Channel = serde_json::from_str("\"chat\"").unwrap();
        assert_eq!(chat, Channel::Chat);

        let mcp: Channel = serde_json::from_str("\"mcp\"").unwrap();
        assert_eq!(mcp, Channel::Mcp);
    }

    #[test]
    fn test_channel_roundtrip_messagepack() {
        let channels = [Channel::Chat, Channel::Mcp];
        for channel in channels {
            let encoded = rmp_serde::to_vec(&channel).unwrap();
            let decoded: Channel = rmp_serde::from_slice(&encoded).unwrap();
            assert_eq!(channel, decoded);
        }
    }

    #[test]
    fn test_channel_default() {
        let channel = Channel::default();
        assert_eq!(channel, Channel::Chat);
    }
}
