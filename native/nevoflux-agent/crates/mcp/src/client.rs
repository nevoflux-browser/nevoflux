//! MCP Client implementation.
//!
//! Provides a high-level API for interacting with MCP servers.

use crate::backend::McpClientBackend;
use crate::error::{McpError, Result};
use crate::transport::{McpTransport, StdioTransport};
use crate::types::{
    InitializeParams, InitializeResult, JsonRpcNotification, JsonRpcRequest, Resource,
    ResourceContent, ServerCapabilities, ServerInfo, ToolDefinition, ToolResult,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MCP client for communicating with MCP servers.
pub struct McpClient {
    /// Transport layer.
    transport: Arc<dyn McpTransport>,
    /// Server information (populated after initialization).
    server_info: RwLock<Option<ServerInfo>>,
    /// Server capabilities (populated after initialization).
    capabilities: RwLock<Option<ServerCapabilities>>,
    /// Whether the client has been initialized.
    initialized: RwLock<bool>,
}

impl McpClient {
    /// Connect to an MCP server via stdio transport.
    ///
    /// This spawns the specified command as a child process and communicates
    /// with it via stdin/stdout.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute
    /// * `args` - Arguments to pass to the command
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = McpClient::connect_stdio("npx", &["-y", "@anthropic/mcp-server-filesystem", "~"]).await?;
    /// ```
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self> {
        Self::connect_stdio_with_env(command, args, &HashMap::new()).await
    }

    /// Connect to an MCP server via stdio transport with environment variables.
    ///
    /// This spawns the specified command as a child process with custom environment
    /// variables and communicates with it via stdin/stdout.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute
    /// * `args` - Arguments to pass to the command
    /// * `env` - Environment variables to set for the process
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut env = HashMap::new();
    /// env.insert("API_KEY".to_string(), "secret".to_string());
    /// let client = McpClient::connect_stdio_with_env("node", &["server.js"], &env).await?;
    /// ```
    pub async fn connect_stdio_with_env(
        command: &str,
        args: &[&str],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let transport = StdioTransport::spawn_with_env(command, args, env).await?;
        let client = Self::from_transport(Arc::new(transport));
        client.initialize().await?;
        Ok(client)
    }

    /// Create a client from an existing transport.
    pub fn from_transport(transport: Arc<dyn McpTransport>) -> Self {
        Self {
            transport,
            server_info: RwLock::new(None),
            capabilities: RwLock::new(None),
            initialized: RwLock::new(false),
        }
    }

    /// Initialize the connection with the MCP server.
    async fn initialize(&self) -> Result<()> {
        let params = InitializeParams::default();

        let response = self
            .transport
            .request(JsonRpcRequest::new(
                "initialize",
                Some(serde_json::to_value(&params)?),
            ))
            .await?;

        if let Some(error) = response.error {
            return Err(McpError::RpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        let result: InitializeResult = response
            .result
            .ok_or_else(|| {
                McpError::UnexpectedResponse("Missing result in initialize response".to_string())
            })
            .and_then(|v| {
                serde_json::from_value(v).map_err(|e| McpError::DeserializationError(e.to_string()))
            })?;

        // Store server info and capabilities
        *self.server_info.write().await = Some(result.server_info);
        *self.capabilities.write().await = Some(result.capabilities);

        // Send initialized notification
        self.transport
            .notify(JsonRpcNotification::new("notifications/initialized", None))
            .await?;

        *self.initialized.write().await = true;

        Ok(())
    }

    /// Check if the client is initialized and connected.
    pub async fn is_ready(&self) -> bool {
        *self.initialized.read().await && self.transport.is_connected()
    }

    /// Get server information.
    pub async fn server_info(&self) -> Option<ServerInfo> {
        self.server_info.read().await.clone()
    }

    /// Get server capabilities.
    pub async fn capabilities(&self) -> Option<ServerCapabilities> {
        self.capabilities.read().await.clone()
    }

    /// List available tools.
    ///
    /// Returns a list of all tools provided by the MCP server.
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        self.ensure_initialized().await?;

        let response = self
            .transport
            .request(JsonRpcRequest::new("tools/list", None))
            .await?;

        if let Some(error) = response.error {
            return Err(McpError::RpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        let result = response
            .result
            .ok_or_else(|| McpError::UnexpectedResponse("Missing result".to_string()))?;

        // Extract tools array from result
        let tools_value = result
            .get("tools")
            .ok_or_else(|| McpError::UnexpectedResponse("Missing tools field".to_string()))?;

        let tools: Vec<ToolDefinition> = serde_json::from_value(tools_value.clone())
            .map_err(|e| McpError::DeserializationError(e.to_string()))?;

        Ok(tools)
    }

    /// Call a tool with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to call
    /// * `arguments` - Arguments to pass to the tool (as JSON value)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = client.call_tool("read_file", serde_json::json!({
    ///     "path": "/home/user/test.txt"
    /// })).await?;
    /// ```
    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolResult> {
        self.ensure_initialized().await?;

        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let response = self
            .transport
            .request(JsonRpcRequest::new("tools/call", Some(params)))
            .await?;

        if let Some(error) = response.error {
            return Err(McpError::RpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        let result = response
            .result
            .ok_or_else(|| McpError::UnexpectedResponse("Missing result".to_string()))?;

        let tool_result: ToolResult = serde_json::from_value(result)
            .map_err(|e| McpError::DeserializationError(e.to_string()))?;

        Ok(tool_result)
    }

    /// List available resources.
    ///
    /// Returns a list of all resources provided by the MCP server.
    pub async fn list_resources(&self) -> Result<Vec<Resource>> {
        self.ensure_initialized().await?;

        let response = self
            .transport
            .request(JsonRpcRequest::new("resources/list", None))
            .await?;

        if let Some(error) = response.error {
            return Err(McpError::RpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        let result = response
            .result
            .ok_or_else(|| McpError::UnexpectedResponse("Missing result".to_string()))?;

        let resources_value = result
            .get("resources")
            .ok_or_else(|| McpError::UnexpectedResponse("Missing resources field".to_string()))?;

        let resources: Vec<Resource> = serde_json::from_value(resources_value.clone())
            .map_err(|e| McpError::DeserializationError(e.to_string()))?;

        Ok(resources)
    }

    /// Read a resource by URI.
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI of the resource to read
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        self.ensure_initialized().await?;

        let params = serde_json::json!({
            "uri": uri
        });

        let response = self
            .transport
            .request(JsonRpcRequest::new("resources/read", Some(params)))
            .await?;

        if let Some(error) = response.error {
            return Err(McpError::RpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        let result = response
            .result
            .ok_or_else(|| McpError::UnexpectedResponse("Missing result".to_string()))?;

        // The result contains a "contents" array with ResourceContent items
        let contents_value = result
            .get("contents")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| McpError::UnexpectedResponse("Missing contents".to_string()))?;

        let content: ResourceContent = serde_json::from_value(contents_value.clone())
            .map_err(|e| McpError::DeserializationError(e.to_string()))?;

        Ok(content)
    }

    /// Perform a health check.
    ///
    /// Returns `true` if the server is responsive.
    pub async fn health_check(&self) -> Result<bool> {
        if !self.transport.is_connected() {
            return Ok(false);
        }

        // Try to ping by listing tools (or any simple request)
        match self
            .transport
            .request(JsonRpcRequest::new("ping", None))
            .await
        {
            Ok(_) => Ok(true),
            Err(McpError::RpcError { code: -32601, .. }) => {
                // Method not found is OK - server is responding
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Close the connection to the MCP server.
    pub async fn close(&self) -> Result<()> {
        self.transport.close().await
    }

    /// Ensure the client is initialized.
    async fn ensure_initialized(&self) -> Result<()> {
        if !*self.initialized.read().await {
            return Err(McpError::NotInitialized);
        }
        if !self.transport.is_connected() {
            return Err(McpError::ConnectionFailed(
                "Transport disconnected".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl McpClientBackend for McpClient {
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        McpClient::list_tools(self).await
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        McpClient::list_resources(self).await
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolResult> {
        McpClient::call_tool(self, name, arguments).await
    }

    async fn health_check(&self) -> Result<bool> {
        McpClient::health_check(self).await
    }

    async fn close(&self) -> Result<()> {
        McpClient::close(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JsonRpcResponse;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock transport for testing.
    struct MockTransport {
        connected: AtomicBool,
        responses: tokio::sync::Mutex<Vec<JsonRpcResponse>>,
    }

    impl MockTransport {
        fn new(responses: Vec<JsonRpcResponse>) -> Self {
            Self {
                connected: AtomicBool::new(true),
                responses: tokio::sync::Mutex::new(responses),
            }
        }
    }

    #[async_trait::async_trait]
    impl McpTransport for MockTransport {
        async fn request(&self, _request: JsonRpcRequest) -> Result<JsonRpcResponse> {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                Err(McpError::UnexpectedResponse(
                    "No more responses".to_string(),
                ))
            } else {
                Ok(responses.remove(0))
            }
        }

        async fn notify(&self, _notification: JsonRpcNotification) -> Result<()> {
            Ok(())
        }

        async fn close(&self) -> Result<()> {
            self.connected.store(false, Ordering::SeqCst);
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_client_from_transport() {
        let transport = Arc::new(MockTransport::new(vec![]));
        let client = McpClient::from_transport(transport);

        // Not initialized yet
        assert!(!*client.initialized.read().await);
    }

    #[tokio::test]
    async fn test_client_list_tools_not_initialized() {
        let transport = Arc::new(MockTransport::new(vec![]));
        let client = McpClient::from_transport(transport);

        let result = client.list_tools().await;
        assert!(matches!(result, Err(McpError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_client_call_tool_not_initialized() {
        let transport = Arc::new(MockTransport::new(vec![]));
        let client = McpClient::from_transport(transport);

        let result = client.call_tool("test", serde_json::json!({})).await;
        assert!(matches!(result, Err(McpError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_client_health_check_connected() {
        let transport = Arc::new(MockTransport::new(vec![JsonRpcResponse::error(
            1,
            crate::types::JsonRpcError::new(-32601, "Method not found"),
        )]));
        let client = McpClient::from_transport(transport);

        // Health check should return true even if method not found
        let result = client.health_check().await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_client_health_check_disconnected() {
        let transport = Arc::new(MockTransport::new(vec![]));
        transport.connected.store(false, Ordering::SeqCst);
        let client = McpClient::from_transport(transport);

        let result = client.health_check().await.unwrap();
        assert!(!result);
    }
}
