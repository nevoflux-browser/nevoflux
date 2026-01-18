/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Shared Protocol for NevoFlux Extension Communication
//!
//! This module defines the Rust enum-based protocol for communication between:
//! - Chat Sidebar (Dioxus WASM) <-> Background Script (JS)
//! - Background Script (JS) <-> Content Sidebar (Dioxus WASM in Shadow DOM)
//!
//! All messages are serialized to JSON for cross-context transmission.

pub mod common;
pub mod channel1;

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: &str = "3.1";

/// Message direction indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    /// Chat Sidebar -> Background -> Content Sidebar
    Downstream,
    /// Content Sidebar -> Background -> Chat Sidebar
    Upstream,
}

/// Main protocol enum for all inter-component messages
///
/// This enum is the single source of truth for all message types
/// in the extension. Using Rust enums provides type safety and
/// exhaustive pattern matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ExtensionMessage {
    // =========================================================================
    // Chat Sidebar -> Background (Upstream to Native Agent)
    // =========================================================================
    /// User sends a chat message
    ChatMessage(ChatMessagePayload),

    /// User requests to stop generation
    StopGeneration { session_id: String },

    /// User triggers a UI action (button click, form submit)
    UiAction(UiActionPayload),

    /// Request current tab context
    RequestTabContext,

    // =========================================================================
    // Background -> Chat Sidebar (Downstream from Native Agent)
    // =========================================================================
    /// Streaming text response from agent
    StreamChunk(StreamChunkPayload),

    /// Stream completed
    StreamEnd { stream_id: String, session_id: String },

    /// Agent error occurred
    AgentError(AgentErrorPayload),

    /// Tab context update
    TabContextUpdate(TabContext),

    /// Connection status change
    ConnectionStatus(ConnectionStatusPayload),

    // =========================================================================
    // Chat Sidebar -> Background -> Content Sidebar
    // =========================================================================
    /// Send content to display in Content Sidebar
    DisplayContent(DisplayContentPayload),

    /// Clear Content Sidebar display
    ClearContent { session_id: String },

    /// Highlight element on page
    HighlightElement(HighlightElementPayload),

    /// Remove element highlight
    ClearHighlight { session_id: String },

    // =========================================================================
    // Content Sidebar -> Background -> Chat Sidebar
    // =========================================================================
    /// Content Sidebar reports current URL (default state)
    ContentUrlReport(ContentUrlPayload),

    /// User clicked an element in Content Sidebar
    ContentElementClick(ContentElementClickPayload),

    /// Content Sidebar ready notification
    ContentSidebarReady { tab_id: u32 },

    // =========================================================================
    // Page Context (Computer Use)
    // =========================================================================
    /// Request page context from Content Sidebar
    RequestPageContext { session_id: String },

    /// Page context response from Content Sidebar
    PageContextResponse(PageContextPayload),

    // =========================================================================
    // Tool Execution (Computer Use)
    // =========================================================================
    /// Tool call request from Native Agent
    ToolCall(ToolCallPayload),

    /// Tool execution result from Content Sidebar or Background
    ToolResult(ToolResultPayload),

    /// Agent state update for UI feedback
    AgentStateUpdate(AgentStatePayload),

    // =========================================================================
    // System Messages
    // =========================================================================
    /// Ping/pong for health checking
    Ping { timestamp: u64 },
    Pong { timestamp: u64 },

    /// Request to inject Content Sidebar into current tab
    InjectContentSidebar { tab_id: u32 },

    /// Content Sidebar injection result
    ContentSidebarInjected { tab_id: u32, success: bool },
}

/// Chat message from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagePayload {
    pub session_id: String,
    pub message_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Deprecated: use page_context instead
    #[serde(default)]
    pub include_page_context: bool,
    /// Auto-injected page context for Computer Use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_context: Option<AutoPageContext>,
}

/// File or image attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub mime_type: String,
    /// Base64 encoded data
    pub data: String,
}

/// UI action from Dioxus components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiActionPayload {
    pub session_id: String,
    pub action_id: String,
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_data: Option<serde_json::Value>,
}

/// Streaming text chunk from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    pub session_id: String,
    pub stream_id: String,
    pub delta: String,
    #[serde(default)]
    pub format: StreamFormat,
}

/// Format for streamed content
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamFormat {
    #[default]
    Markdown,
    Plain,
    Html,
}

/// Error from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentErrorPayload {
    pub session_id: String,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub recoverable: bool,
}

/// Current tab context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TabContext {
    pub tab_id: u32,
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon_url: Option<String>,
    pub status: TabStatus,
}

/// Tab loading status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabStatus {
    #[default]
    Loading,
    Complete,
    Error,
}

/// Connection status with native agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusPayload {
    pub connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Content to display in Content Sidebar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayContentPayload {
    pub session_id: String,
    pub content_type: DisplayContentType,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Type of content to display
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayContentType {
    /// Markdown rendered content
    Markdown,
    /// Raw HTML (sanitized)
    Html,
    /// Plain text
    Text,
    /// JSON data (rendered as tree)
    Json,
    /// Code with syntax highlighting
    Code { language: String },
    /// URL to load in iframe
    Iframe,
}

/// Element highlight request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightElementPayload {
    pub session_id: String,
    pub selector: String,
    #[serde(default)]
    pub style: HighlightStyle,
}

/// Highlight visual style
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightStyle {
    #[default]
    Outline,
    Overlay,
    Pulse,
}

/// URL report from Content Sidebar (default state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentUrlPayload {
    pub tab_id: u32,
    pub url: String,
    pub title: String,
}

/// Click event from Content Sidebar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentElementClickPayload {
    pub tab_id: u32,
    pub element_tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_text: Option<String>,
}

// =========================================================================
// Page Context Types (Computer Use)
// =========================================================================

/// Page context payload with DOM summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContextPayload {
    pub session_id: String,
    pub tab_id: u32,
    pub context: AutoPageContext,
}

/// Automatic page context attached to messages
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoPageContext {
    pub url: String,
    pub title: String,
    pub viewport: Viewport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interactive_elements: Vec<InteractiveElement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
}

/// Browser viewport information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub scroll_height: u32,
}

/// Interactive element on the page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElement {
    /// Unique CSS selector for this element
    pub selector: String,
    /// HTML tag name (button, input, a, etc.)
    pub tag: String,
    /// Input type if applicable (text, password, submit, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_type: Option<String>,
    /// Visible text content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Placeholder text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// ARIA role
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Name attribute
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// ID attribute
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Class list
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_list: Vec<String>,
    /// Bounding box position
    pub bounding_box: BoundingBox,
    /// Element visibility
    #[serde(default = "default_true")]
    pub is_visible: bool,
    /// Element enabled state
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Element bounding box
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// =========================================================================
// Tool Execution Types (Computer Use)
// =========================================================================

/// Tool call request from Native Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallPayload {
    /// Unique call ID for tracking
    pub call_id: String,
    pub session_id: String,
    /// Tool name: click, type, scroll, read_text, etc.
    pub tool_name: String,
    /// Tool parameters as JSON
    pub parameters: serde_json::Value,
    /// Whether to show visual feedback in Content Sidebar
    #[serde(default = "default_true")]
    pub show_feedback: bool,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultPayload {
    pub call_id: String,
    pub session_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Optional screenshot after execution (base64)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

/// Agent execution state update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatePayload {
    pub session_id: String,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<String>,
    pub step_count: u32,
    pub max_steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Agent execution state
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// LLM is processing/thinking
    #[default]
    Thinking,
    /// Executing a tool
    ExecutingTool,
    /// Waiting for tool result
    WaitingResult,
    /// Task completed successfully
    Complete,
    /// Error occurred
    Error,
    /// Waiting for user confirmation
    WaitingConfirmation,
}

/// Browser tool names (for type safety)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTool {
    Click,
    Type,
    Scroll,
    ReadText,
    ReadPage,
    Screenshot,
    Navigate,
    Wait,
    EvaluateJs,
    FillForm,
    GetElements,
    Highlight,
}

impl BrowserTool {
    /// Check if this tool requires user confirmation
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, Self::Navigate | Self::EvaluateJs | Self::FillForm)
    }

    /// Check if this tool is executed by Content Sidebar (vs Background)
    pub fn executed_by_content_sidebar(&self) -> bool {
        !matches!(self, Self::Screenshot | Self::Navigate)
    }
}

// =========================================================================
// WASM Bindings for JavaScript Interop
// =========================================================================

#[cfg(feature = "wasm")]
mod wasm_bindings {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Serialize a message to JSON string for sending to JavaScript
    #[wasm_bindgen]
    pub fn serialize_message(message: JsValue) -> Result<String, JsValue> {
        let msg: ExtensionMessage = serde_wasm_bindgen::from_value(message)?;
        serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Deserialize a JSON string from JavaScript to message
    #[wasm_bindgen]
    pub fn deserialize_message(json: &str) -> Result<JsValue, JsValue> {
        let msg: ExtensionMessage = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
        serde_wasm_bindgen::to_value(&msg)
            .map_err(|e| JsValue::from_str(&format!("Conversion error: {}", e)))
    }

    /// Get protocol version
    #[wasm_bindgen]
    pub fn get_protocol_version() -> String {
        PROTOCOL_VERSION.to_string()
    }
}

// =========================================================================
// Helper Methods
// =========================================================================

impl ExtensionMessage {
    /// Create a new chat message
    pub fn chat(session_id: impl Into<String>, message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::ChatMessage(ChatMessagePayload {
            session_id: session_id.into(),
            message_id: message_id.into(),
            text: text.into(),
            attachments: Vec::new(),
            include_page_context: false,
            page_context: None,
        })
    }

    /// Create a chat message with page context
    pub fn chat_with_context(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        text: impl Into<String>,
        page_context: AutoPageContext,
    ) -> Self {
        Self::ChatMessage(ChatMessagePayload {
            session_id: session_id.into(),
            message_id: message_id.into(),
            text: text.into(),
            attachments: Vec::new(),
            include_page_context: true,
            page_context: Some(page_context),
        })
    }

    /// Create a stream chunk message
    pub fn stream_chunk(session_id: impl Into<String>, stream_id: impl Into<String>, delta: impl Into<String>) -> Self {
        Self::StreamChunk(StreamChunkPayload {
            session_id: session_id.into(),
            stream_id: stream_id.into(),
            delta: delta.into(),
            format: StreamFormat::Markdown,
        })
    }

    /// Create a display content message
    pub fn display_markdown(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::DisplayContent(DisplayContentPayload {
            session_id: session_id.into(),
            content_type: DisplayContentType::Markdown,
            content: content.into(),
            title: None,
        })
    }

    /// Create a URL report message
    pub fn url_report(tab_id: u32, url: impl Into<String>, title: impl Into<String>) -> Self {
        Self::ContentUrlReport(ContentUrlPayload {
            tab_id,
            url: url.into(),
            title: title.into(),
        })
    }

    /// Check if message is downstream (Chat -> Content)
    pub fn is_downstream(&self) -> bool {
        matches!(
            self,
            Self::DisplayContent(_)
                | Self::ClearContent { .. }
                | Self::HighlightElement(_)
                | Self::ClearHighlight { .. }
                | Self::InjectContentSidebar { .. }
        )
    }

    /// Check if message is upstream (Content -> Chat)
    pub fn is_upstream(&self) -> bool {
        matches!(
            self,
            Self::ContentUrlReport(_)
                | Self::ContentElementClick(_)
                | Self::ContentSidebarReady { .. }
                | Self::ContentSidebarInjected { .. }
        )
    }

    /// Get session ID if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::ChatMessage(p) => Some(&p.session_id),
            Self::StopGeneration { session_id } => Some(session_id),
            Self::UiAction(p) => Some(&p.session_id),
            Self::StreamChunk(p) => Some(&p.session_id),
            Self::StreamEnd { session_id, .. } => Some(session_id),
            Self::AgentError(p) => Some(&p.session_id),
            Self::DisplayContent(p) => Some(&p.session_id),
            Self::ClearContent { session_id } => Some(session_id),
            Self::HighlightElement(p) => Some(&p.session_id),
            Self::ClearHighlight { session_id } => Some(session_id),
            Self::RequestPageContext { session_id } => Some(session_id),
            Self::PageContextResponse(p) => Some(&p.session_id),
            Self::ToolCall(p) => Some(&p.session_id),
            Self::ToolResult(p) => Some(&p.session_id),
            Self::AgentStateUpdate(p) => Some(&p.session_id),
            _ => None,
        }
    }

    /// Create a tool call message
    pub fn tool_call(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self::ToolCall(ToolCallPayload {
            call_id: call_id.into(),
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            parameters,
            show_feedback: true,
        })
    }

    /// Create a tool result message
    pub fn tool_result(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        success: bool,
        result: Option<serde_json::Value>,
        error: Option<String>,
    ) -> Self {
        Self::ToolResult(ToolResultPayload {
            call_id: call_id.into(),
            session_id: session_id.into(),
            success,
            result,
            error,
            screenshot: None,
        })
    }

    /// Create an agent state update message
    pub fn agent_state(
        session_id: impl Into<String>,
        state: AgentState,
        step_count: u32,
        max_steps: u32,
    ) -> Self {
        Self::AgentStateUpdate(AgentStatePayload {
            session_id: session_id.into(),
            state,
            current_tool: None,
            step_count,
            max_steps,
            message: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = ExtensionMessage::chat("session-1", "msg-1", "Hello, world!");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("chat_message"));
        assert!(json.contains("Hello, world!"));
    }

    #[test]
    fn test_message_deserialization() {
        let json = r#"{"type":"stream_chunk","payload":{"session_id":"s1","stream_id":"st1","delta":"Hello","format":"markdown"}}"#;
        let msg: ExtensionMessage = serde_json::from_str(json).unwrap();
        match msg {
            ExtensionMessage::StreamChunk(p) => {
                assert_eq!(p.delta, "Hello");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_direction_detection() {
        let downstream = ExtensionMessage::display_markdown("s1", "# Title");
        assert!(downstream.is_downstream());
        assert!(!downstream.is_upstream());

        let upstream = ExtensionMessage::url_report(1, "https://example.com", "Example");
        assert!(upstream.is_upstream());
        assert!(!upstream.is_downstream());
    }
}
