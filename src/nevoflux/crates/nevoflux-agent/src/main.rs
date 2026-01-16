/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! NevoFlux Agent - Native Messaging Host
//!
//! This is the main agent that communicates with the browser extension
//! via native messaging protocol and orchestrates AI capabilities.

mod action_router;
mod agent;
mod llm_integration;
mod native_messaging;
mod session;
mod stream_manager;
mod tools;

use anyhow::Result;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use action_router::ActionRouter;
use session::SessionManager;

#[derive(Parser, Debug)]
#[command(name = "nevoflux-agent")]
#[command(about = "NevoFlux AI Agent - Native Messaging Host", long_about = None)]
struct Args {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Native messaging manifest path (passed by Firefox, ignored)
    #[arg(hide = true)]
    manifest_path: Option<String>,

    /// Extension ID (passed by Firefox, ignored)
    #[arg(hide = true)]
    extension_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Write startup marker to file for debugging Firefox native messaging issues
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/nevoflux-agent-debug.log")
    {
        use std::io::Write;
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] NevoFlux Agent started, args: {:?}", now, std::env::args().collect::<Vec<_>>());
    }

    let args = Args::parse();

    // IMPORTANT: Disable tracing output for native messaging!
    // Native messaging uses stdout for protocol, and stderr may also cause
    // issues with Firefox. Use file-based logging only (see log_to_file helper).
    // Only enable tracing if explicitly requested via --debug flag.
    if args.debug {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::DEBUG)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .with_writer(std::io::sink) // Discard all output
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    }

    info!("Starting NevoFlux Agent");

    // Load configuration
    let _config = if let Some(config_path) = args.config {
        nevoflux_common::config::Config::from_file(&config_path)?
    } else {
        nevoflux_common::config::Config::default()
    };

    // Initialize session manager
    let session_manager = SessionManager::new();

    // Initialize action router
    let action_router = ActionRouter::new();

    // Register example action handlers
    register_example_actions(&action_router).await;

    // Start native messaging loop
    info!("Starting native messaging loop");
    native_messaging::run(session_manager, action_router).await?;

    // Log to file before exit
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/nevoflux-agent-debug.log")
    {
        use std::io::Write;
        let _ = writeln!(f, "=== Main function returning, agent stopping ===");
    }

    info!("NevoFlux Agent stopped");
    Ok(())
}

/// Register example action handlers
async fn register_example_actions(router: &ActionRouter) {
    // Example: Export CSV action
    router
        .register(
            "export_csv".to_string(),
            |_session_id, _action_id, form_data| async move {
                info!("Exporting CSV with data: {:?}", form_data);
                Ok(serde_json::json!({
                    "status": "success",
                    "message": "CSV exported successfully"
                }))
            },
        )
        .await;

    // Example: Confirm payment action
    router
        .register(
            "confirm_payment".to_string(),
            |_session_id, _action_id, form_data| async move {
                info!("Processing payment: {:?}", form_data);
                Ok(serde_json::json!({
                    "status": "success",
                    "message": "Payment confirmed"
                }))
            },
        )
        .await;
}
