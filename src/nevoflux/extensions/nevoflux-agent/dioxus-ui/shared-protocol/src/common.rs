/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Common types shared across all channels

use serde::{Deserialize, Serialize};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "5.0.0";

/// Permission scope for authorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    Once,
    Session,
    Always,
}

/// Resource types that require permission
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Script,
    Network,
    Mcp,
    Plugin,
}

/// Actions on resources
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAction {
    Read,
    Write,
    Execute,
    Connect,
}

/// Requester information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requester {
    #[serde(rename = "type")]
    pub requester_type: RequesterType,
    pub id: String,
    pub name: String,
}

/// Type of requester
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequesterType {
    Agent,
    Plugin,
    Skill,
}

/// File attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub mime_type: String,
    /// Base64 encoded data (for images) or None (for files)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// File path (for non-image files, agent will read)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Stream format
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamFormat {
    #[default]
    Markdown,
    Plain,
    Html,
}

/// Content block type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Markdown,
    Code,
    A2ui,
    Image,
}

/// Agent execution state
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Thinking,
    #[default]
    Idle,
    /// Executing a tool (alias for computer use compatibility)
    Executing,
    /// Executing a specific tool
    ExecutingTool,
    /// Waiting for result
    Waiting,
    /// Waiting for tool result
    WaitingResult,
    /// Waiting for user confirmation (human-in-the-loop)
    WaitingConfirmation,
    Complete,
    Error,
}

/// Tool execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

/// Error level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ErrorLevel {
    Warning,
    #[default]
    Error,
    Fatal,
}

/// Plan type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanType {
    Free,
    Pro,
    Team,
}

/// Plugin action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Start,
    Stop,
    Restart,
}

/// Browser tool action types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserToolAction {
    /// Navigate to a URL
    Navigate,
    /// Go back in browser history
    GoBack,
    /// Go forward in browser history
    GoForward,
    /// Click an element
    Click,
    /// Type text
    Type,
    /// Fill form field
    Fill,
    /// Get page content
    GetContent,
    /// Take screenshot
    Screenshot,
    /// Execute JavaScript
    EvalJs,
    /// Wait for element
    WaitFor,
    /// Scroll page
    Scroll,
    /// Get element info
    GetElement,
    /// Get all elements
    QueryAll,
    /// Take snapshot of interactive elements
    Snapshot,
    /// Click element by snapshot ID
    ClickById,
    /// Fill element by snapshot ID
    FillById,
    /// Type into element by snapshot ID
    TypeById,
    /// Get page content as Markdown
    GetMarkdown,
    /// Fetch URL and convert to Markdown (saves to cache file)
    WebFetch,
    /// Cache tab content as Markdown (saves to cache file, returns path)
    CacheTabMarkdown,
    /// Web search (returns search results)
    WebSearch,
    /// Ask user a question (shows UI, waits for response)
    AskUser,
    /// Cache uploaded file to disk (returns absolute path)
    CacheFile,
    /// Wait for page to stabilize after an action
    WaitForStable,
    /// Press a keyboard key
    KeyPress,
    /// List all open browser tabs
    ListTabs,
    /// Query tabs with optional filters
    QueryTabs,
    /// Get all interactive elements on the page
    GetElements,
    /// Read the source code of a canvas artifact
    ReadArtifact,
    /// Edit a canvas artifact using search-and-replace
    EditArtifact,
}
