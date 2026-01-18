/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 1: Input handler (Sidebar → Agent)

use anyhow::Result;
use shared_protocol::InputMessage;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Handler for Channel 1 (Input)
pub struct InputChannelHandler {
    sender: mpsc::UnboundedSender<InputMessage>,
}

impl InputChannelHandler {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<InputMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    /// Process incoming message from the channel
    pub async fn handle_message(&self, json: &str) -> Result<()> {
        let message: InputMessage = serde_json::from_str(json)?;
        debug!("Input channel received: {:?}", message);

        match &message {
            InputMessage::ChatMessage(payload) => {
                info!("Chat message: session={}, text={}", payload.session_id, payload.text);
            }
            InputMessage::SkillCommand(payload) => {
                info!("Skill command: session={}, skill={}", payload.session_id, payload.skill_name);
            }
            InputMessage::StopGeneration(payload) => {
                info!("Stop generation: session={}", payload.session_id);
            }
            InputMessage::PermissionResponse(payload) => {
                info!("Permission response: request={}, granted={}", payload.request_id, payload.granted);
            }
            InputMessage::PluginCommand(payload) => {
                info!("Plugin command: plugin={}, action={:?}", payload.plugin_id, payload.action);
            }
            InputMessage::SystemCommand(payload) => {
                info!("System command: request={}, command={}", payload.request_id, payload.command);
            }
        }

        self.sender.send(message)?;
        Ok(())
    }
}

impl Default for InputChannelHandler {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_chat_message() {
        let (handler, mut receiver) = InputChannelHandler::new();

        let json = r#"{"type":"chat_message","payload":{"session_id":"s1","message_id":"m1","text":"Hello"}}"#;
        handler.handle_message(json).await.unwrap();

        let msg = receiver.recv().await.unwrap();
        match msg {
            InputMessage::ChatMessage(p) => assert_eq!(p.text, "Hello"),
            _ => panic!("Wrong type"),
        }
    }
}
