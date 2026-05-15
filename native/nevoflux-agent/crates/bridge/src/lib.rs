//! NevoFlux Bridge - Proxy and MCP bridge for NevoFlux Agent.
//!
//! This crate provides two bridge modes:
//!
//! 1. **Proxy Mode** (`nevoflux-agent`): Bridges between the browser extension
//!    (via Native Messaging) and the daemon (via TCP).
//!
//! 2. **MCP Mode** (`nevoflux-agent --mcp`): Bridges between MCP clients like
//!    Claude Code (via stdio) and the daemon (via TCP).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐         ┌─────────────┐
//! │ Extension   │         │ Claude Code │
//! │ (Sidebar)   │         │ (MCP Client)│
//! └──────┬──────┘         └──────┬──────┘
//!        │ Native Msg            │ stdio (MCP)
//!        ▼                       ▼
//! ┌─────────────┐         ┌─────────────┐
//! │  Proxy      │         │ MCP Bridge  │
//! │  (bridge)   │         │ (bridge)    │
//! └──────┬──────┘         └──────┬──────┘
//!        │                       │
//!        │    Length-prefix TCP   │
//!        └───────────┬───────────┘
//!                    ▼
//!          ┌─────────────────┐
//!          │ nevoflux-agent --daemon │
//!          │   (Core Daemon) │
//!          └─────────────────┘
//! ```
//!
//! # Usage
//!
//! ## Proxy Mode
//!
//! ```rust,ignore
//! use nevoflux_bridge::{Proxy, ProxyConfig};
//! use tokio::io::{stdin, stdout};
//!
//! let config = ProxyConfig::new();
//! let mut proxy = Proxy::new(stdin(), stdout(), config);
//! proxy.connect().await?;
//! ```
//!
//! ## MCP Mode
//!
//! ```rust,ignore
//! use nevoflux_bridge::{McpBridge, McpBridgeConfig};
//! use tokio::io::{stdin, stdout};
//!
//! let config = McpBridgeConfig::new();
//! let mut bridge = McpBridge::new(stdin(), stdout(), config);
//! bridge.connect().await?;
//! ```

pub mod async_proxy;
pub mod config;
pub mod daemon_client;
pub mod error;
pub mod mcp_bridge;
pub mod native_messaging;
pub mod port_discovery;
pub mod proxy;
pub mod streaming;

// Re-export main types
pub use async_proxy::{
    run_async_proxy, AsyncProxyConfig, ProxyResult, StdinMessage, StdoutMessage,
};
pub use config::{BridgeConfig, ConnectionMode};
pub use daemon_client::{generate_proxy_id, DaemonClient, DaemonMessageStream};
pub use error::{BridgeError, Result};
pub use mcp_bridge::{error_codes, McpBridge, McpBridgeConfig, McpBridgeState};
pub use native_messaging::{decode_message, encode_message, read_message, write_message};
pub use port_discovery::{
    cleanup_files, discover_daemon, find_available_port, is_process_running, launch_daemon,
    launch_daemon_with_port, read_pid_file, read_port_file, wait_for_daemon_ready, write_pid_file,
    write_port_file, DaemonInfo,
};
pub use proxy::{parse_native_message, Proxy, ProxyConfig, ProxyState};
pub use streaming::{
    extract_stream_message, ActiveStream, StreamAccumulator, StreamError, StreamMessageType,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports_available() {
        // Verify all re-exports are accessible
        let _ = BridgeConfig::new();
        let _ = ProxyConfig::new();
        let _ = McpBridgeConfig::new();
        let _ = generate_proxy_id();
    }

    #[test]
    fn test_version_info() {
        // Verify we can access protocol version through re-exported types
        use nevoflux_protocol::PROTOCOL_VERSION;
        assert_eq!(PROTOCOL_VERSION, "5.0.0");
    }
}
