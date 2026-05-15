//! MCP Client Backend Abstraction
//!
//! This module provides a trait that abstracts over different MCP client implementations,
//! allowing the registry to work with either the custom McpClient or the official rmcp SDK.

use crate::error::Result;
use crate::types::{Resource, ToolDefinition, ToolResult};
use async_trait::async_trait;
use std::collections::HashMap;

/// Trait for MCP client backends.
///
/// This trait defines the common interface that both the custom `McpClient`
/// and the `RmcpClient` (using the official rmcp SDK) implement.
///
/// The registry uses this trait to work with any backend implementation,
/// selected at compile time via feature flags.
#[async_trait]
pub trait McpClientBackend: Send + Sync {
    /// List available tools from the server.
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;

    /// List available resources from the server.
    async fn list_resources(&self) -> Result<Vec<Resource>>;

    /// Call a tool with the given arguments.
    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolResult>;

    /// Perform a health check on the connection.
    async fn health_check(&self) -> Result<bool>;

    /// Close the connection to the server.
    async fn close(&self) -> Result<()>;
}

/// Factory function type for creating MCP client backends.
pub type ClientFactory = Box<
    dyn Fn(
            &str,
            &[String],
            &HashMap<String, String>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Box<dyn McpClientBackend>>> + Send>,
        > + Send
        + Sync,
>;
