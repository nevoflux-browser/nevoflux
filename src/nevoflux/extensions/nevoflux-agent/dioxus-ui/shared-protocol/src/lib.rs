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
//! - Chat Channel: Bidirectional (Sidebar <-> Agent)
//! - MCP Channel: Bidirectional (Browser Use MCP)

pub mod common;
pub mod chat;
pub mod mcp;

// Re-export common types
pub use common::*;

// Re-export chat types
pub use chat::{
    ChatMessage, MessageDirection, ChatMode,
    // Sidebar -> Agent
    TabReference, ChatMessagePayload, SkillCommandPayload, StopGenerationPayload, CancelPayload,
    PermissionResponsePayload, PluginCommandPayload, SystemCommandPayload,
    BrowserToolResponsePayload, BrowserToolError,
    // File picker (native dialog)
    PickerMode, PickFilesRequestPayload, FileInfo, PickFilesResponsePayload,
    // Agent -> Sidebar
    StreamChunkPayload, ToolCallInfo, StreamEndPayload, StreamMetadata,
    ContentBlockPayload, PermissionRequestPayload, AgentStatePayload,
    StepInfo, ToolInfo, ErrorPayload, AccountStatusPayload, AccountInfo,
    PlanInfo, QuotaInfo, UsageQuota, SystemResponsePayload, SystemError,
    BrowserToolRequestPayload,
    // Plan proposal/response
    PlanStep, PlanProposalPayload, PlanResponse, PlanResponsePayload,
    // Tool events & authorization
    ToolEvent, ToolEventStatus, ToolAuthRequest, AuthOption, AuthScope,
    ToolAuthResponsePayload, ThinkingEvent,
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
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: Some(123),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::ChatMessage(p) => {
                assert_eq!(p.content, "Hello");
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

    #[test]
    fn test_stream_chunk_roundtrip() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "World".to_string(),
            tool_calls: vec![],
            done: false,
            session_title: None,
            event: None,
            thinking_event: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::StreamChunk(p) => assert_eq!(p.content, "World"),
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_message_direction() {
        // ToAgent messages
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);

        // ToSidebar messages
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hi".to_string(),
            tool_calls: vec![],
            done: false,
            session_title: None,
            event: None,
            thinking_event: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    #[allow(deprecated)]
    fn test_backward_compatibility_types() {
        // InputMessage and OutputMessage should be aliases for ChatMessage
        let msg: InputMessage = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);

        let msg: OutputMessage = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Response".to_string(),
            tool_calls: vec![],
            done: true,
            session_title: None,
            event: None,
            thinking_event: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }
}
