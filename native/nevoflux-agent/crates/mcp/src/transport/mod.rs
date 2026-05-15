//! Transport layer for MCP communication.
//!
//! Provides different transport implementations:
//! - [`StdioTransport`] - Communication via stdin/stdout with a child process
//!
//! Future transports (not yet implemented):
//! - SSE Transport - Communication via HTTP/SSE (Server-Sent Events)

mod stdio;

pub use stdio::StdioTransport;

use crate::error::Result;
use crate::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;

/// Trait for MCP transport implementations.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a request and wait for a response.
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;

    /// Send a notification (no response expected).
    async fn notify(&self, notification: JsonRpcNotification) -> Result<()>;

    /// Close the transport connection.
    async fn close(&self) -> Result<()>;

    /// Check if the transport is connected.
    fn is_connected(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Transport trait tests will be in integration tests
    // since they require actual process spawning

    #[test]
    fn test_transport_trait_is_object_safe() {
        // This test ensures McpTransport can be used as a trait object
        fn _takes_transport(_t: &dyn McpTransport) {}
    }
}
