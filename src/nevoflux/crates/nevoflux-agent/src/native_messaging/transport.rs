/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Native Messaging Transport Layer
//!
//! Implements the Chrome/Firefox Native Messaging protocol:
//! - Messages are prefixed with 4-byte little-endian length
//! - Content is UTF-8 JSON

use anyhow::{Context, Result};
use std::io::{self, Read, Write};
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

/// Native Messaging message reader
///
/// Reads messages from stdin with 4-byte length prefix
pub struct MessageReader {
    stdin: io::Stdin,
}

impl MessageReader {
    /// Create a new message reader
    pub fn new() -> Self {
        Self { stdin: io::stdin() }
    }

    /// Read a single message from stdin
    ///
    /// Returns Ok(None) when EOF is reached
    pub fn read_message(&mut self) -> Result<Option<String>> {
        // Read 4-byte length prefix (little-endian)
        let mut length_bytes = [0u8; 4];
        match self.stdin.read_exact(&mut length_bytes) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                debug!("EOF reached on stdin");
                return Ok(None);
            }
            Err(e) => {
                return Err(e).context("Failed to read message length prefix");
            }
        }

        let length = u32::from_le_bytes(length_bytes) as usize;
        trace!("Reading message of length: {}", length);

        // Validate length (max 1MB to prevent DOS)
        if length > 1024 * 1024 {
            anyhow::bail!("Message too large: {} bytes", length);
        }

        if length == 0 {
            anyhow::bail!("Invalid message length: 0");
        }

        // Read message content
        let mut buffer = vec![0u8; length];
        self.stdin
            .read_exact(&mut buffer)
            .context("Failed to read message content")?;

        let message = String::from_utf8(buffer).context("Message is not valid UTF-8")?;
        trace!("Received message: {}", message);

        Ok(Some(message))
    }

    /// Start reading messages asynchronously
    ///
    /// Returns a channel receiver for incoming messages
    pub fn start_async(mut self) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            // File-based logging helper
            fn log_to_file(msg: &str) {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nevoflux-agent-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "[Reader] {}", msg);
                }
            }

            log_to_file("Reader thread started");
            loop {
                log_to_file("Waiting to read next message...");
                match self.read_message() {
                    Ok(Some(message)) => {
                        log_to_file(&format!("Got message: {}...", &message[..message.len().min(50)]));
                        if tx.send(message).is_err() {
                            log_to_file("Receiver dropped, stopping reader");
                            debug!("Receiver dropped, stopping reader");
                            break;
                        }
                        log_to_file("Message sent to channel, continuing loop");
                    }
                    Ok(None) => {
                        log_to_file("EOF reached on stdin, stopping reader");
                        debug!("EOF reached, stopping reader");
                        break;
                    }
                    Err(e) => {
                        log_to_file(&format!("Error reading message: {:#}", e));
                        error!("Error reading message: {:#}", e);
                        break;
                    }
                }
            }
            log_to_file("Reader thread exiting");
        });

        rx
    }
}

/// Native Messaging message writer
///
/// Writes messages to stdout with 4-byte length prefix
pub struct MessageWriter {
    stdout: io::Stdout,
}

impl MessageWriter {
    /// Create a new message writer
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
        }
    }

    /// Write a message to stdout
    pub fn write_message(&mut self, message: &str) -> Result<()> {
        // File-based logging helper
        fn log_to_file(msg: &str) {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/nevoflux-agent-debug.log")
            {
                use std::io::Write;
                let _ = writeln!(f, "[MessageWriter] {}", msg);
            }
        }

        let bytes = message.as_bytes();
        let length = bytes.len() as u32;

        log_to_file(&format!("Writing {} bytes message", length));

        // Build complete message buffer
        let mut buffer = Vec::with_capacity(4 + bytes.len());
        buffer.extend_from_slice(&length.to_le_bytes());
        buffer.extend_from_slice(bytes);

        // Write atomically with lock
        {
            let mut handle = self.stdout.lock();
            handle.write_all(&buffer).context("Failed to write")?;
            handle.flush().context("Failed to flush")?;
        }

        log_to_file("Write complete");
        Ok(())
    }
}

/// Thread-safe message writer wrapper
pub struct AsyncMessageWriter {
    pub tx: mpsc::UnboundedSender<String>,
}

impl AsyncMessageWriter {
    /// Create a new async message writer
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        std::thread::spawn(move || {
            // Debug log to file
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/nevoflux-agent-debug.log")
            {
                use std::io::Write;
                let _ = writeln!(f, "Writer thread started");
            }

            let mut writer = MessageWriter::new();

            while let Some(message) = rx.blocking_recv() {
                // Debug log to file
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nevoflux-agent-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "Writer received message: {}", &message[..message.len().min(100)]);
                }

                if let Err(e) = writer.write_message(&message) {
                    // Debug log to file
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/nevoflux-agent-debug.log")
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "Writer error: {:#}", e);
                    }
                    error!("Error writing message: {:#}", e);
                    break;
                }

                // Debug log to file
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/nevoflux-agent-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "Writer successfully wrote message to stdout");
                }
            }

            // Debug log to file
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/nevoflux-agent-debug.log")
            {
                use std::io::Write;
                let _ = writeln!(f, "Writer thread exiting");
            }

            debug!("Message writer stopped");
        });

        Self { tx }
    }

    /// Send a message asynchronously
    pub fn send(&self, message: String) -> Result<()> {
        self.tx
            .send(message)
            .context("Failed to send message to writer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_encoding() {
        let length: u32 = 42;
        let bytes = length.to_le_bytes();
        let decoded = u32::from_le_bytes(bytes);
        assert_eq!(length, decoded);
    }

    #[test]
    fn test_message_format() {
        let message = r#"{"type":"test","data":"hello"}"#;
        let length = message.len() as u32;

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&length.to_le_bytes());
        buffer.extend_from_slice(message.as_bytes());

        // Verify we can extract length
        let length_bytes: [u8; 4] = buffer[0..4].try_into().unwrap();
        let extracted_length = u32::from_le_bytes(length_bytes);
        assert_eq!(extracted_length, message.len() as u32);

        // Verify we can extract message
        let extracted_message = String::from_utf8(buffer[4..].to_vec()).unwrap();
        assert_eq!(extracted_message, message);
    }
}
