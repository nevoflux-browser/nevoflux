/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Native Messaging Protocol Implementation
//!
//! Handles communication with the browser extension using Firefox's native messaging protocol.
//! Messages are encoded as: [4-byte length (little-endian)][JSON message]
//!
//! IMPORTANT: Never use stdout/stderr for debug output!
//! - stdout is used for protocol communication
//! - stderr may cause issues with Firefox
//! Always use file-based logging for debugging.

mod handler;
mod transport;

pub use handler::MessageHandler;
pub use transport::{AsyncMessageWriter, MessageReader, MessageWriter};

use anyhow::Result;
use tracing::{debug, error};

use crate::action_router::ActionRouter;
use crate::session::SessionManager;
use crate::stream_manager::StreamManager;

/// Log to debug file (use this instead of println!/eprintln!)
fn log_debug(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/nevoflux-agent-debug.log")
    {
        use std::io::Write;
        let _ = writeln!(f, "[mod] {}", msg);
    }
}

/// Run the native messaging loop
pub async fn run(session_manager: SessionManager, action_router: ActionRouter) -> Result<()> {
    debug!("Starting native messaging loop");
    log_debug("Native messaging host starting...");

    // Create message writer
    let writer = AsyncMessageWriter::new();
    log_debug("Message writer created");

    // Create stream manager
    let stream_manager = StreamManager::new(writer.tx.clone());
    log_debug("Stream manager created");

    // Create message handler
    let handler = MessageHandler::new(session_manager, action_router, stream_manager);
    log_debug("Handler created, waiting for messages...");

    // Create message reader and start async reading
    let reader = MessageReader::new();
    let mut rx = reader.start_async();
    log_debug("Reader started");

    // Process incoming messages
    while let Some(message) = rx.recv().await {
        log_debug(&format!("Received message: {}", &message[..message.len().min(100)]));
        if let Err(e) = handler.handle_message(&message).await {
            error!("Error handling message: {:#}", e);
            log_debug(&format!("Error: {:#}", e));
        }
    }

    log_debug("Loop ended");
    debug!("Native messaging loop stopped");
    Ok(())
}
