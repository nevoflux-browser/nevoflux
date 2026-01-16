/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Browser Agent Native Messaging Protocol V2.0
//!
//! This module implements the complete protocol specification for communication
//! between the Browser Extension (Frontend) and Rust Native Host (Backend).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "2.0";

/// Base envelope structure for all messages
///
/// All uplink (Extension -> Agent) and downlink (Agent -> Extension) messages
/// are wrapped in this unified JSON envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// Protocol version
    pub ver: String,

    /// Message unique ID (UUID v4) for tracking and cancellation
    pub msg_id: String,

    /// Session ID (UUID v4), typically bound to a browser Tab ID
    pub session_id: String,

    /// Message type (Namespace.Action)
    #[serde(rename = "type")]
    pub msg_type: String,

    /// Message payload
    pub payload: T,

    /// Unix timestamp in seconds
    pub timestamp: u64,
}

impl<T> Envelope<T> {
    /// Create a new envelope
    pub fn new(session_id: String, msg_type: String, payload: T) -> Self {
        Self {
            ver: PROTOCOL_VERSION.to_string(),
            msg_id: uuid::Uuid::new_v4().to_string(),
            session_id,
            msg_type,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

// =============================================================================
// Uplink Protocol: Extension -> Rust Agent
// =============================================================================

/// Input messages from user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InputMessage {
    /// User chat message with multimodal support
    Chat(ChatInput),

    /// A2UI component interaction event
    #[serde(rename = "ui_event")]
    UiEvent(UiEventInput),

    /// Direct command
    Command(CommandInput),
}

/// Chat input with multimodal support (text + images + attachments)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInput {
    /// Text content
    pub text: String,

    /// Attached files (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileAttachment>,

    /// Context reference (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_ref: Option<ContextRef>,
}

/// File attachment with base64 data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttachment {
    /// File name
    pub name: String,

    /// MIME type
    pub mime: String,

    /// Base64 encoded data
    pub data: String,
}

/// Context reference for including additional context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRef {
    /// Include current tab content
    #[serde(default)]
    pub include_current_tab: bool,

    /// Include selected text
    #[serde(default)]
    pub include_selection: bool,
}

/// UI event from A2UI component interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEventInput {
    /// Source UI view ID
    pub view_id: String,

    /// Action ID triggered
    pub action_id: String,

    /// Form data (if component is a form)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub form_data: HashMap<String, serde_json::Value>,
}

/// Direct command input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInput {
    /// Command name (e.g., "agent.stop_generation", "plugin.run_script")
    pub cmd: String,

    /// Command arguments
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub args: HashMap<String, serde_json::Value>,
}

/// Context synchronization messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextMessage {
    /// Tab state update
    TabUpdate(TabUpdate),
}

/// Tab status update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabUpdate {
    /// Tab ID
    pub tab_id: u32,

    /// Current URL
    pub url: String,

    /// Page title
    pub title: String,

    /// Loading status
    pub status: String,
}

// =============================================================================
// Downlink Protocol: Rust Agent -> Extension
// =============================================================================

/// Agent response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentMessage {
    /// Streaming text response
    #[serde(rename = "stream")]
    Stream(StreamMessage),

    /// UI render command
    #[serde(rename = "ui")]
    Ui(UiMessage),
}

/// Streaming message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "lowercase")]
pub enum StreamMessage {
    /// Text stream delta
    Text(TextStream),
}

/// Text stream for LLM thinking process or response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStream {
    /// Stream ID for correlation
    pub stream_id: String,

    /// Incremental text delta
    pub delta: String,

    /// Render format (default: "markdown")
    #[serde(default = "default_format")]
    pub format: String,

    /// Stream finished flag
    #[serde(default)]
    pub finish: bool,
}

fn default_format() -> String {
    "markdown".to_string()
}

/// UI message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum UiMessage {
    /// Render A2UI components
    Render(UiRender),

    /// Update existing UI component
    Update(UiUpdate),
}

/// UI render command with A2UI JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiRender {
    /// Target route: "sidebar" or "content_script"
    pub route: String,

    /// View ID
    pub view_id: String,

    /// UI component tree
    pub layout: A2UiComponent,
}

/// UI update command for existing components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiUpdate {
    /// View ID to update
    pub view_id: String,

    /// Target component ID
    pub target_component_id: String,

    /// Properties to update
    pub props: HashMap<String, serde_json::Value>,
}

/// Browser control message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserControl {
    /// Target tab ID
    pub tab_id: u32,

    /// Action type: "navigate", "click", "scroll", "highlight"
    pub action: String,

    /// CSS selector (for click/highlight actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,

    /// Value (for navigate/scroll actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

// =============================================================================
// A2UI Component Schema
// =============================================================================

/// A2UI component definition
///
/// Each node contains component type, optional id, properties, and children.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2UiComponent {
    /// Component type name
    pub component: String,

    /// Component ID (optional, auto-generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Component properties
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub props: HashMap<String, serde_json::Value>,

    /// Child components
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<A2UiComponent>,
}

impl A2UiComponent {
    /// Create a new component
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            id: None,
            props: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Set component ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Add a property
    pub fn with_prop(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.props.insert(
            key.into(),
            serde_json::to_value(value).expect("Failed to serialize property"),
        );
        self
    }

    /// Add a child component
    pub fn with_child(mut self, child: A2UiComponent) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children
    pub fn with_children(mut self, children: Vec<A2UiComponent>) -> Self {
        self.children.extend(children);
        self
    }

    /// Assign component IDs recursively (if not already assigned)
    pub fn assign_ids(&mut self, prefix: &str, counter: &mut usize) {
        if self.id.is_none() {
            *counter += 1;
            self.id = Some(format!("{}{}", prefix, counter));
        }

        for child in &mut self.children {
            child.assign_ids(prefix, counter);
        }
    }
}

// Helper functions for creating common components

/// Create a Container component
pub fn container(layout: &str) -> A2UiComponent {
    A2UiComponent::new("Container").with_prop("layout", layout)
}

/// Create a Text component
pub fn text(content: impl Into<String>, text_type: &str) -> A2UiComponent {
    A2UiComponent::new("Text")
        .with_prop("content", content.into())
        .with_prop("type", text_type)
}

/// Create a Button component
pub fn button(label: impl Into<String>, action_id: impl Into<String>) -> A2UiComponent {
    A2UiComponent::new("Button")
        .with_prop("label", label.into())
        .with_prop("action_id", action_id.into())
        .with_prop("variant", "primary")
}

/// Create an Input component
pub fn input(name: impl Into<String>, input_type: &str) -> A2UiComponent {
    A2UiComponent::new("Input")
        .with_prop("name", name.into())
        .with_prop("type", input_type)
}

/// Create a Table component
pub fn table(headers: Vec<String>, rows: Vec<Vec<String>>) -> A2UiComponent {
    A2UiComponent::new("Table")
        .with_prop("headers", headers)
        .with_prop("rows", rows)
}

/// Create a Spinner component
pub fn spinner(text: impl Into<String>) -> A2UiComponent {
    A2UiComponent::new("Spinner").with_prop("text", text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_creation() {
        let payload = ChatInput {
            text: "Hello".to_string(),
            files: Vec::new(),
            context_ref: None,
        };

        let envelope = Envelope::new("session-123".to_string(), "input.chat".to_string(), payload);

        assert_eq!(envelope.ver, "2.0");
        assert_eq!(envelope.session_id, "session-123");
        assert_eq!(envelope.msg_type, "input.chat");
    }

    #[test]
    fn test_a2ui_component_builder() {
        let component = container("flex-col")
            .with_id("container-1")
            .with_prop("gap", "sm")
            .with_child(text("Hello World", "h1").with_prop("color", "primary"))
            .with_child(button("Click Me", "btn_action"));

        assert_eq!(component.component, "Container");
        assert_eq!(component.id, Some("container-1".to_string()));
        assert_eq!(component.children.len(), 2);
    }

    #[test]
    fn test_component_id_assignment() {
        let mut component = container("flex-col")
            .with_child(text("Text 1", "p"))
            .with_child(text("Text 2", "p"));

        let mut counter = 0;
        component.assign_ids("cmp_", &mut counter);

        assert_eq!(component.id, Some("cmp_1".to_string()));
        assert_eq!(component.children[0].id, Some("cmp_2".to_string()));
        assert_eq!(component.children[1].id, Some("cmp_3".to_string()));
    }
}
