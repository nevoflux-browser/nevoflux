# 通道架构重构实现计划 (4通道 → 2通道)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 4 通道架构重构为 2 通道架构（Chat + MCP），简化通信层，让 background.js 成为纯通道。

**Architecture:** 合并 Input/Output 为 Chat 通道，删除 PageLLM 通道，保留 MCP 通道。background.js 使用 `bg:` 前缀 API 命名空间，Sidebar 控制 Browser Tool 执行流程。

**Tech Stack:** Rust (shared-protocol), JavaScript (background.js), Dioxus (Sidebar)

**Design Doc:** `docs/plans/2026-01-22-channel-architecture-refactor-design.md`

---

## Task 1: 创建 chat.rs（合并 channel1 + channel2）

**Files:**
- Create: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/chat.rs`

**Step 1: 创建 chat.rs 文件**

```rust
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Chat Channel (bidirectional)
//!
//! Merged from Channel 1 (Input) and Channel 2 (Output).
//! All messages between Chat Sidebar and Native Agent.

use serde::{Deserialize, Serialize};
use crate::common::{
    AgentState, Attachment, BrowserToolAction, ContentType, ErrorLevel,
    PermissionScope, PlanType, PluginAction, Requester, RequesterType,
    ResourceAction, ResourceType, StreamFormat, ToolStatus,
};

// =============================================================================
// Sidebar → Agent Payloads
// =============================================================================

/// Chat message from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagePayload {
    pub session_id: String,
    pub message_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
}

/// Skill command trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCommandPayload {
    pub session_id: String,
    pub skill_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Stop generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopGenerationPayload {
    pub session_id: String,
}

/// Permission response from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponsePayload {
    pub request_id: String,
    pub granted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<PermissionScope>,
}

/// Plugin command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommandPayload {
    pub plugin_id: String,
    pub action: PluginAction,
}

/// System command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommandPayload {
    pub request_id: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Browser tool response from Sidebar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolResponsePayload {
    pub request_id: String,
    pub session_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserToolError>,
}

/// Browser tool error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub recoverable: bool,
}

// =============================================================================
// Agent → Sidebar Payloads
// =============================================================================

/// Stream chunk for streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    pub session_id: String,
    pub stream_id: String,
    pub delta: String,
    #[serde(default)]
    pub format: StreamFormat,
}

/// Stream end marker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEndPayload {
    pub session_id: String,
    pub stream_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<StreamMetadata>,
}

/// Metadata for completed stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Content block with type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockPayload {
    pub session_id: String,
    pub block_id: String,
    pub content_type: ContentType,
    pub content: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Permission request for human-in-the-loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub resource_type: ResourceType,
    pub action: ResourceAction,
    pub resource: String,
    pub requester: Requester,
    pub reason: String,
    pub scope: PermissionScope,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    60000
}

/// Agent state update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatePayload {
    pub session_id: String,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
}

/// Step information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInfo {
    pub current: u32,
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Tool execution information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub status: ToolStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Error notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub session_id: String,
    pub error_id: String,
    pub level: ErrorLevel,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default)]
    pub recoverable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_request_id: Option<String>,
}

/// Account status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatusPayload {
    pub logged_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaInfo>,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Plan information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInfo {
    #[serde(rename = "type")]
    pub plan_type: PlanType,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Quota information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_calls: Option<UsageQuota>,
}

/// Usage quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageQuota {
    pub used: u32,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

/// System command response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResponsePayload {
    pub request_id: String,
    pub command: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SystemError>,
}

/// System error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemError {
    pub code: String,
    pub message: String,
}

/// Browser tool request from Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolRequestPayload {
    pub request_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    pub action: BrowserToolAction,
    pub params: serde_json::Value,
    #[serde(default = "default_browser_timeout")]
    pub timeout_ms: u64,
}

fn default_browser_timeout() -> u64 {
    30000
}

// =============================================================================
// Chat Message Enum
// =============================================================================

/// Message direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    ToAgent,
    ToSidebar,
}

/// All Chat channel message types (bidirectional)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ChatMessage {
    // ========== Sidebar → Agent ==========
    /// User chat message
    ChatMessage(ChatMessagePayload),
    /// Skill command
    SkillCommand(SkillCommandPayload),
    /// Stop generation
    StopGeneration(StopGenerationPayload),
    /// Permission response
    PermissionResponse(PermissionResponsePayload),
    /// Plugin command
    PluginCommand(PluginCommandPayload),
    /// System command
    SystemCommand(SystemCommandPayload),
    /// Browser tool response
    BrowserToolResponse(BrowserToolResponsePayload),

    // ========== Agent → Sidebar ==========
    /// Stream chunk
    StreamChunk(StreamChunkPayload),
    /// Stream end
    StreamEnd(StreamEndPayload),
    /// Content block
    ContentBlock(ContentBlockPayload),
    /// Permission request
    PermissionRequest(PermissionRequestPayload),
    /// Agent state
    AgentState(AgentStatePayload),
    /// Error
    Error(ErrorPayload),
    /// Account status
    AccountStatus(AccountStatusPayload),
    /// System response
    SystemResponse(SystemResponsePayload),
    /// Browser tool request
    BrowserToolRequest(BrowserToolRequestPayload),
}

impl ChatMessage {
    /// Get message direction
    pub fn direction(&self) -> MessageDirection {
        match self {
            // Sidebar → Agent
            Self::ChatMessage(_) |
            Self::SkillCommand(_) |
            Self::StopGeneration(_) |
            Self::PermissionResponse(_) |
            Self::PluginCommand(_) |
            Self::SystemCommand(_) |
            Self::BrowserToolResponse(_) => MessageDirection::ToAgent,

            // Agent → Sidebar
            Self::StreamChunk(_) |
            Self::StreamEnd(_) |
            Self::ContentBlock(_) |
            Self::PermissionRequest(_) |
            Self::AgentState(_) |
            Self::Error(_) |
            Self::AccountStatus(_) |
            Self::SystemResponse(_) |
            Self::BrowserToolRequest(_) => MessageDirection::ToSidebar,
        }
    }

    /// Get session_id if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::ChatMessage(p) => Some(&p.session_id),
            Self::SkillCommand(p) => Some(&p.session_id),
            Self::StopGeneration(p) => Some(&p.session_id),
            Self::PermissionResponse(_) => None,
            Self::PluginCommand(_) => None,
            Self::SystemCommand(_) => None,
            Self::BrowserToolResponse(p) => Some(&p.session_id),
            Self::StreamChunk(p) => Some(&p.session_id),
            Self::StreamEnd(p) => Some(&p.session_id),
            Self::ContentBlock(p) => Some(&p.session_id),
            Self::PermissionRequest(p) => Some(&p.session_id),
            Self::AgentState(p) => Some(&p.session_id),
            Self::Error(p) => Some(&p.session_id),
            Self::AccountStatus(_) => None,
            Self::SystemResponse(_) => None,
            Self::BrowserToolRequest(p) => Some(&p.session_id),
        }
    }
}

// =============================================================================
// Re-export for backward compatibility (deprecated, will be removed)
// =============================================================================

/// Deprecated: Use ChatMessage instead
#[deprecated(since = "5.0.0", note = "Use ChatMessage instead")]
pub type InputMessage = ChatMessage;

/// Deprecated: Use ChatMessage instead
#[deprecated(since = "5.0.0", note = "Use ChatMessage instead")]
pub type OutputMessage = ChatMessage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_direction() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);

        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            delta: "Hi".to_string(),
            format: StreamFormat::Markdown,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_chat_message_roundtrip() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
            tab_id: Some(42),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id(), Some("s1"));
    }

    #[test]
    fn test_stream_chunk_serialization() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            delta: "Hello".to_string(),
            format: StreamFormat::Markdown,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_chunk"));
    }

    #[test]
    fn test_browser_tool_request_serialization() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            tab_id: None,
            action: BrowserToolAction::Click,
            params: serde_json::json!({"selector": "#btn"}),
            timeout_ms: 30000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("browser_tool_request"));
    }

    #[test]
    fn test_browser_tool_response_serialization() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            success: true,
            result: Some(serde_json::json!({"clicked": true})),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("browser_tool_response"));
    }
}
```

**Step 2: 验证文件创建**

Run: `ls -la src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/chat.rs`
Expected: 文件存在

---

## Task 2: 重命名 channel3.rs 为 mcp.rs

**Files:**
- Rename: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel3.rs` → `mcp.rs`

**Step 1: 复制 channel3.rs 为 mcp.rs 并更新注释**

```rust
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! MCP Channel (bidirectional)
//!
//! Messages for MCP Server functionality (Browser Use API).
//! Renamed from Channel 3 in protocol v5.0.

use serde::{Deserialize, Serialize};

/// MCP request source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSource {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// MCP request (Agent → Extension)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequestPayload {
    pub request_id: String,
    pub source: McpSource,
    pub payload: JsonRpcRequest,
}

/// MCP response (Extension → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponsePayload {
    pub request_id: String,
    pub payload: JsonRpcResponse,
}

/// All MCP channel message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum McpMessage {
    McpRequest(McpRequestPayload),
    McpResponse(McpResponsePayload),
}

impl McpMessage {
    /// Get request_id
    pub fn request_id(&self) -> &str {
        match self {
            Self::McpRequest(p) => &p.request_id,
            Self::McpResponse(p) => &p.request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_request_serialization() {
        let msg = McpMessage::McpRequest(McpRequestPayload {
            request_id: "req-1".to_string(),
            source: McpSource {
                agent: "claude-code".to_string(),
                session_id: None,
            },
            payload: JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "browser_use/click".to_string(),
                params: Some(serde_json::json!({"selector": "#btn"})),
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("mcp_request"));
    }

    #[test]
    fn test_mcp_response_serialization() {
        let msg = McpMessage::McpResponse(McpResponsePayload {
            request_id: "req-1".to_string(),
            payload: JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                result: Some(serde_json::json!({"success": true})),
                error: None,
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("mcp_response"));
    }

    #[test]
    fn test_request_id() {
        let msg = McpMessage::McpRequest(McpRequestPayload {
            request_id: "test-id".to_string(),
            source: McpSource {
                agent: "test".to_string(),
                session_id: None,
            },
            payload: JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "test".to_string(),
                params: None,
            },
        });
        assert_eq!(msg.request_id(), "test-id");
    }
}
```

---

## Task 3: 更新 common.rs（版本号 + 移除 LlmProvider）

**Files:**
- Modify: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/common.rs`

**Step 1: 更新 PROTOCOL_VERSION 为 5.0.0**

修改第 10 行：
```rust
pub const PROTOCOL_VERSION: &str = "5.0.0";
```

**Step 2: 移除 LlmProvider 枚举（第 138-145 行）**

删除：
```rust
/// LLM provider for page mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Claude,
    Chatgpt,
    Gemini,
}
```

---

## Task 4: 更新 lib.rs（新导出结构）

**Files:**
- Modify: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/lib.rs`

**Step 1: 重写 lib.rs**

```rust
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Shared Protocol for NevoFlux Extension Communication (v5.0)
//!
//! This crate defines the Rust types for communication between:
//! - Chat Sidebar (Dioxus WASM) <-> Native Agent (Rust)
//!
//! ## Channel Architecture (v5.0)
//!
//! - Chat Channel: Bidirectional (Sidebar ↔ Agent)
//! - MCP Channel: Bidirectional (Browser Use MCP)

pub mod common;
pub mod chat;
pub mod mcp;

// Re-export common types
pub use common::*;

// Re-export chat types
pub use chat::{
    ChatMessage, MessageDirection,
    // Sidebar → Agent
    ChatMessagePayload, SkillCommandPayload, StopGenerationPayload,
    PermissionResponsePayload, PluginCommandPayload, SystemCommandPayload,
    BrowserToolResponsePayload, BrowserToolError,
    // Agent → Sidebar
    StreamChunkPayload, StreamEndPayload, StreamMetadata,
    ContentBlockPayload, PermissionRequestPayload, AgentStatePayload,
    StepInfo, ToolInfo, ErrorPayload, AccountStatusPayload, AccountInfo,
    PlanInfo, QuotaInfo, UsageQuota, SystemResponsePayload, SystemError,
    BrowserToolRequestPayload,
};

// Re-export MCP types
pub use mcp::{
    McpMessage, McpRequestPayload, McpResponsePayload,
    McpSource, JsonRpcRequest, JsonRpcResponse, JsonRpcError,
};

// Backward compatibility re-exports (deprecated)
#[allow(deprecated)]
pub use chat::{InputMessage, OutputMessage};

#[cfg(feature = "wasm")]
mod wasm_bindings {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Get protocol version
    #[wasm_bindgen]
    pub fn get_protocol_version() -> String {
        PROTOCOL_VERSION.to_string()
    }

    /// Serialize ChatMessage to JSON
    #[wasm_bindgen]
    pub fn serialize_chat_message(message: JsValue) -> Result<String, JsValue> {
        let msg: ChatMessage = serde_wasm_bindgen::from_value(message)?;
        serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Deserialize JSON to ChatMessage
    #[wasm_bindgen]
    pub fn deserialize_chat_message(json: &str) -> Result<JsValue, JsValue> {
        let msg: ChatMessage = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
        serde_wasm_bindgen::to_value(&msg)
            .map_err(|e| JsValue::from_str(&format!("Conversion error: {}", e)))
    }

    /// Serialize McpMessage to JSON
    #[wasm_bindgen]
    pub fn serialize_mcp_message(message: JsValue) -> Result<String, JsValue> {
        let msg: McpMessage = serde_wasm_bindgen::from_value(message)?;
        serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Deserialize JSON to McpMessage
    #[wasm_bindgen]
    pub fn deserialize_mcp_message(json: &str) -> Result<JsValue, JsValue> {
        let msg: McpMessage = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
        serde_wasm_bindgen::to_value(&msg)
            .map_err(|e| JsValue::from_str(&format!("Conversion error: {}", e)))
    }

    // Backward compatibility (deprecated)
    #[wasm_bindgen]
    #[deprecated(since = "5.0.0", note = "Use serialize_chat_message instead")]
    pub fn serialize_input_message(message: JsValue) -> Result<String, JsValue> {
        serialize_chat_message(message)
    }

    #[wasm_bindgen]
    #[deprecated(since = "5.0.0", note = "Use deserialize_chat_message instead")]
    pub fn deserialize_output_message(json: &str) -> Result<JsValue, JsValue> {
        deserialize_chat_message(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "5.0.0");
    }

    #[test]
    fn test_chat_message_roundtrip() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            text: "Hello".to_string(),
            attachments: vec![],
            tab_id: Some(123),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::ChatMessage(p) => {
                assert_eq!(p.text, "Hello");
                assert_eq!(p.tab_id, Some(123));
            }
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_mcp_message_roundtrip() {
        let msg = McpMessage::McpRequest(McpRequestPayload {
            request_id: "req-1".to_string(),
            source: McpSource {
                agent: "test".to_string(),
                session_id: None,
            },
            payload: JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "test".to_string(),
                params: None,
            },
        });
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: McpMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id(), "req-1");
    }
}
```

---

## Task 5: 删除旧通道文件

**Files:**
- Delete: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel1.rs`
- Delete: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel2.rs`
- Delete: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel3.rs`
- Delete: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel4.rs`

**Step 1: 删除文件**

Run: `rm src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel1.rs src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel2.rs src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel3.rs src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/channel4.rs`

**Step 2: 验证**

Run: `ls src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/src/`
Expected: 只有 `lib.rs`, `chat.rs`, `mcp.rs`, `common.rs`

---

## Task 6: 运行 Rust 测试验证

**Step 1: 运行 shared-protocol 测试**

Run: `cd src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol && cargo test`
Expected: 所有测试通过

**Step 2: Commit shared-protocol 变更**

```bash
git add src/nevoflux/extensions/nevoflux-agent/dioxus-ui/shared-protocol/
git commit -m "refactor(protocol): merge 4 channels into 2 (Chat + MCP)

- Merge channel1 (Input) and channel2 (Output) into chat.rs
- Rename channel3.rs to mcp.rs
- Delete channel4.rs (PageLLM)
- Update protocol version to 5.0.0
- Add MessageDirection helper
- Add backward compatibility re-exports (deprecated)

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 7: 重构 background.js

**Files:**
- Modify: `src/nevoflux/extensions/nevoflux-agent/background/background.js`

**Step 1: 更新头部注释和常量**

替换第 1-40 行：
```javascript
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

console.log("[NevoFlux] Background script starting...");

/**
 * NevoFlux Agent Background Script
 * Manages communication between:
 * - Chat Sidebar (Dioxus WASM) <-> Native Messaging Host (Rust)
 *
 * Protocol Version: 5.0 (2-channel architecture)
 *
 * Channels:
 * - Chat (Bidirectional): All Sidebar ↔ Agent messages
 * - MCP (Bidirectional): Browser Use MCP requests/responses
 *
 * API Namespace: "bg:" prefix for all Sidebar-callable APIs
 */

// =============================================================================
// Channel Names (Native Messaging Application IDs)
// =============================================================================

const CHANNEL_NAMES = {
  CHAT: "com.nevoflux.agent",      // Chat channel (bidirectional)
  MCP: "com.nevoflux.agent.mcp",   // MCP channel (bidirectional)
};

// =============================================================================
// Background API (Sidebar callable, "bg:" prefix)
// =============================================================================

const BackgroundAPI = {
  // Channel management
  CONNECT: "bg:connect",
  DISCONNECT: "bg:disconnect",
  GET_STATUS: "bg:get_status",

  // MCP channel management
  MCP_ENABLE: "bg:mcp_enable",
  MCP_DISABLE: "bg:mcp_disable",

  // Send message to Native Agent
  SEND_TO_AGENT: "bg:send_to_agent",

  // Browser tool execution
  EXEC_TOOL: "bg:exec_tool",

  // Tab context
  GET_TAB_CONTEXT: "bg:get_tab_context",
};
```

**Step 2: 更新 MessageTypes 常量（移除 PageLLM 相关）**

替换 MessageTypes 定义：
```javascript
// =============================================================================
// Message Type Constants
// =============================================================================

const MessageTypes = {
  // Sidebar → Agent
  CHAT_MESSAGE: "chat_message",
  SKILL_COMMAND: "skill_command",
  STOP_GENERATION: "stop_generation",
  PERMISSION_RESPONSE: "permission_response",
  PLUGIN_COMMAND: "plugin_command",
  SYSTEM_COMMAND: "system_command",
  BROWSER_TOOL_RESPONSE: "browser_tool_response",

  // Agent → Sidebar
  STREAM_CHUNK: "stream_chunk",
  STREAM_END: "stream_end",
  CONTENT_BLOCK: "content_block",
  PERMISSION_REQUEST: "permission_request",
  AGENT_STATE: "agent_state",
  ERROR: "error",
  ACCOUNT_STATUS: "account_status",
  SYSTEM_RESPONSE: "system_response",
  BROWSER_TOOL_REQUEST: "browser_tool_request",

  // MCP Channel
  MCP_REQUEST: "mcp_request",
  MCP_RESPONSE: "mcp_response",

  // System messages
  PING: "ping",
  PONG: "pong",
  CONNECTION_STATUS: "connection_status",
};
```

**Step 3: 简化 ChannelManager 类**

替换 ChannelManager 类：
```javascript
// =============================================================================
// Channel Manager (Simplified for 2-channel architecture)
// =============================================================================

class ChannelManager {
  constructor() {
    // Chat channel: Sidebar ↔ Agent (bidirectional)
    this.chat = new NativeChannel(
      CHANNEL_NAMES.CHAT,
      "Chat",
      (msg) => this.handleChatMessage(msg),
      (connected, error) => this.handleChatStatusChange(connected, error)
    );

    // MCP channel: Browser Use MCP (bidirectional)
    this.mcp = new NativeChannel(
      CHANNEL_NAMES.MCP,
      "MCP",
      (msg) => this.handleMcpMessage(msg),
      (connected, error) => this.handleMcpStatusChange(connected, error)
    );

    this.connectionStatus = { chat: false, mcp: false };
    this.mcpEnabled = false;
  }

  connect() {
    console.log("[NevoFlux] Connecting Chat channel...");
    this.chat.connect();
  }

  disconnect() {
    this.chat.disconnect();
    if (this.mcpEnabled) {
      this.mcp.disconnect();
    }
  }

  setMcpEnabled(enabled) {
    this.mcpEnabled = enabled;
    if (enabled && !this.mcp.isConnected()) {
      this.mcp.connect();
    } else if (!enabled && this.mcp.isConnected()) {
      this.mcp.disconnect();
    }
  }

  sendToAgent(message) {
    if (!this.chat.isConnected()) {
      console.warn("[NevoFlux] Chat channel not connected, attempting to connect...");
      this.chat.connect();
    }
    return this.chat.send(message);
  }

  sendToMcp(message) {
    if (!this.mcp.isConnected()) {
      if (!this.mcpEnabled) {
        console.warn("[NevoFlux] MCP channel is disabled");
        return false;
      }
      this.mcp.connect();
    }
    return this.mcp.send(message);
  }

  handleChatMessage(message) {
    // Check if this is a chunk that needs reassembly
    if (chunkReassembler.isChunk(message)) {
      const reassembled = chunkReassembler.processChunk(message);
      if (reassembled) {
        broadcastToSidebar(reassembled);
      }
      return;
    }

    // All messages go to Sidebar - Sidebar decides how to handle
    broadcastToSidebar(message);
  }

  handleMcpMessage(message) {
    const msgType = message.type;
    if (msgType === MessageTypes.MCP_REQUEST) {
      handleMcpRequest(message.payload);
    } else if (msgType === MessageTypes.MCP_RESPONSE) {
      broadcastToSidebar(message);
    }
  }

  handleChatStatusChange(connected, error) {
    this.connectionStatus.chat = connected;
    this.broadcastConnectionStatus();
  }

  handleMcpStatusChange(connected, error) {
    this.connectionStatus.mcp = connected;
    this.broadcastConnectionStatus();
  }

  broadcastConnectionStatus() {
    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: {
        connected: this.connectionStatus.chat,
        channels: { ...this.connectionStatus },
      },
    });
  }

  getStatus() {
    return {
      connected: this.connectionStatus.chat,
      channels: { ...this.connectionStatus },
    };
  }
}
```

**Step 4: 重写消息监听器**

替换 `browser.runtime.onMessage.addListener` 部分：
```javascript
// =============================================================================
// Message Listener (Background API)
// =============================================================================

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  const msgType = message.type;

  console.log("[NevoFlux] Background received:", msgType);

  // Handle Background API calls ("bg:" prefix)
  if (msgType && msgType.startsWith("bg:")) {
    return handleBackgroundAPI(msgType, message, sendResponse);
  }

  // Handle legacy ping/pong
  if (msgType === MessageTypes.PING) {
    sendResponse({ type: MessageTypes.PONG, payload: { timestamp: message.payload?.timestamp } });
    channelManager.broadcastConnectionStatus();
    return;
  }

  // Ignore other messages (Sidebar handles them)
  return false;
});

/**
 * Handle Background API calls
 */
function handleBackgroundAPI(apiType, message, sendResponse) {
  switch (apiType) {
    case BackgroundAPI.CONNECT:
      channelManager.connect();
      sendResponse({ success: true });
      break;

    case BackgroundAPI.DISCONNECT:
      channelManager.disconnect();
      sendResponse({ success: true });
      break;

    case BackgroundAPI.GET_STATUS:
      sendResponse(channelManager.getStatus());
      break;

    case BackgroundAPI.MCP_ENABLE:
      channelManager.setMcpEnabled(true);
      sendResponse({ success: true });
      break;

    case BackgroundAPI.MCP_DISABLE:
      channelManager.setMcpEnabled(false);
      sendResponse({ success: true });
      break;

    case BackgroundAPI.SEND_TO_AGENT:
      const sent = channelManager.sendToAgent(message.payload);
      sendResponse({ success: sent });
      break;

    case BackgroundAPI.EXEC_TOOL:
      executeBrowserTool(message.payload, "sidebar")
        .then((result) => sendResponse(result))
        .catch((err) => sendResponse({
          success: false,
          error: { code: -1, message: err.message, recoverable: true },
        }));
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.GET_TAB_CONTEXT:
      getActiveTabContext()
        .then((ctx) => sendResponse(ctx))
        .catch(() => sendResponse(null));
      return true;

    default:
      console.warn("[NevoFlux] Unknown Background API:", apiType);
      sendResponse({ success: false, error: "Unknown API" });
  }
}
```

**Step 5: 移除旧代码**

删除以下内容：
- `SINGLE_CHANNEL_MODE` 常量
- `INPUT_CHANNEL_TYPES`, `MCP_CHANNEL_TYPES`, `PAGELLM_CHANNEL_TYPES` 集合
- `handleInputResponse`, `handleOutputMessage`, `handlePageLlmMessage` 方法
- `handlePageLlmRequest` 函数
- 所有 PageLLM 相关代码

**Step 6: 更新初始化日志**

```javascript
// =============================================================================
// Initialization
// =============================================================================

console.log("[NevoFlux] Background script initialized (Protocol v5.0 - 2-channel architecture)");
console.log("[NevoFlux] Channels: Chat (com.nevoflux.agent), MCP (com.nevoflux.agent.mcp)");
console.log("[NevoFlux] API namespace: bg:*");
```

---

## Task 8: 测试 background.js 变更

**Step 1: 重新打包 Extension**

Run: `npm run reload-ext`

**Step 2: 启动浏览器测试**

Run: `npm run start`

**Step 3: 验证控制台日志**

打开 `about:debugging` → This Firefox → NevoFlux Agent → Inspect
Expected: 看到 "Protocol v5.0 - 2-channel architecture" 日志

**Step 4: Commit background.js 变更**

```bash
git add src/nevoflux/extensions/nevoflux-agent/background/background.js
git commit -m "refactor(extension): simplify background.js to 2-channel architecture

- Remove PageLLM channel and related code
- Merge Input/Output into single Chat channel handler
- Add bg: prefixed API namespace for Sidebar calls
- Simplify ChannelManager class
- Update protocol version to 5.0

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 9: 更新 Sidebar messaging 模块

**Files:**
- Modify: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/chat-sidebar/src/messaging/sender.rs`
- Modify: `src/nevoflux/extensions/nevoflux-agent/dioxus-ui/chat-sidebar/src/messaging/handler.rs`

**Step 1: 查看当前文件结构**

Run: `ls src/nevoflux/extensions/nevoflux-agent/dioxus-ui/chat-sidebar/src/messaging/`

**Step 2: 更新 sender.rs 使用 bg: API**

需要将直接发送消息改为通过 `bg:send_to_agent` API。

**Step 3: 更新 handler.rs 处理 browser_tool_request**

当收到 `browser_tool_request` 时，调用 `bg:exec_tool`，然后发送 `browser_tool_response`。

（具体代码需要根据当前文件内容调整）

---

## Task 10: 最终测试和提交

**Step 1: 重新构建 WASM**

Run: `cd src/nevoflux/extensions/nevoflux-agent/dioxus-ui && ./build.sh`

**Step 2: 重新打包 Extension**

Run: `npm run reload-ext`

**Step 3: 启动浏览器完整测试**

Run: `npm run start`

测试项：
1. Sidebar 加载
2. 发送消息到 Agent
3. 接收 Agent 响应
4. Browser Tool 执行流程

**Step 4: 最终提交**

```bash
git add .
git commit -m "feat(agent): complete 2-channel architecture refactor

- Protocol v5.0: Chat + MCP channels
- background.js: pure channel with bg: API namespace
- Sidebar controls browser tool execution
- All tests passing

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | 创建 chat.rs | shared-protocol/src/chat.rs |
| 2 | 重命名 channel3 为 mcp.rs | shared-protocol/src/mcp.rs |
| 3 | 更新 common.rs | shared-protocol/src/common.rs |
| 4 | 更新 lib.rs | shared-protocol/src/lib.rs |
| 5 | 删除旧通道文件 | channel1-4.rs |
| 6 | 运行 Rust 测试 | cargo test |
| 7 | 重构 background.js | background/background.js |
| 8 | 测试 background.js | npm run reload-ext |
| 9 | 更新 Sidebar messaging | messaging/*.rs |
| 10 | 最终测试和提交 | all |
