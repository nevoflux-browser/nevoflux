/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 2: Output handler (Agent → Sidebar)

use anyhow::Result;
use shared_protocol::OutputMessage;
use tokio::sync::mpsc;
use tracing::debug;

/// Handler for Channel 2 (Output)
pub struct OutputChannelHandler {
    sender: mpsc::UnboundedSender<String>,
}

impl OutputChannelHandler {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    /// Send message to sidebar
    pub fn send(&self, message: OutputMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        debug!("Output channel sending: {}", &json[..json.len().min(100)]);
        self.sender.send(json)?;
        Ok(())
    }

    /// Send raw JSON (for compatibility)
    pub fn send_raw(&self, json: String) -> Result<()> {
        self.sender.send(json)?;
        Ok(())
    }
}

impl Default for OutputChannelHandler {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_protocol::{StreamChunkPayload, StreamFormat};

    #[test]
    fn test_send_stream_chunk() {
        let (handler, mut receiver) = OutputChannelHandler::new();

        let msg = OutputMessage::StreamChunk(StreamChunkPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            delta: "Hello".to_string(),
            format: StreamFormat::Markdown,
        });

        handler.send(msg).unwrap();

        let json = receiver.try_recv().unwrap();
        assert!(json.contains("stream_chunk"));
        assert!(json.contains("Hello"));
    }
}
