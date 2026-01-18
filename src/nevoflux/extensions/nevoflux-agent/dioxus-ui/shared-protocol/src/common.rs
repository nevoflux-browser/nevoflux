/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Common types shared across all channels

use serde::{Deserialize, Serialize};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "4.0.0";

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
    /// Base64 encoded data
    pub data: String,
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
    #[default]
    Thinking,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorLevel {
    Warning,
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

/// LLM provider for page mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Claude,
    Chatgpt,
    Gemini,
}

/// Plugin action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Start,
    Stop,
    Restart,
}
