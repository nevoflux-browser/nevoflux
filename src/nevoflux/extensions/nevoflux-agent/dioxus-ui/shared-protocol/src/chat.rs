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
    PermissionScope, PlanType, PluginAction, Requester,
    ResourceAction, ResourceType, StreamFormat, ToolStatus,
};

// =============================================================================
// Sidebar → Agent Payloads
// =============================================================================

/// Chat mode (selected from sidebar toolbar)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMode {
    #[default]
    Chat,
    Browser,
    Agent,
}

/// The cookieStoreId of a tab that belongs to no container.
///
/// Firefox reports this for ordinary tabs, and it is the single canonical value
/// for "no container" on the wire: never an empty string, never `"default"`.
pub const DEFAULT_COOKIE_STORE_ID: &str = "firefox-default";

/// Tab reference with space, id, title and url (for tab mention context)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabReference {
    /// The tab's cookieStoreId (`firefox-default` or `firefox-container-N`).
    ///
    /// Named `space` because Zen derives a Space's identity from its container.
    /// Tabs outside any container carry [`DEFAULT_COOKIE_STORE_ID`].
    pub space: String,
    /// Tab id
    pub tab_id: i64,
    /// Tab title
    pub tab_title: String,
    /// Tab URL
    #[serde(default)]
    pub url: String,
}

/// Chat message from user
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessagePayload {
    pub session_id: String,
    pub message_id: String,
    /// Message content (renamed from "text" to match nevoflux-agent protocol)
    pub content: String,
    /// Chat mode: chat, browser, or agent
    #[serde(default)]
    pub mode: ChatMode,
    /// Image attachments (base64 encoded)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Local files/directories selected via native file picker
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_files: Vec<FileInfo>,
    /// Current active tab id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// Selected tabs for context (user selected via # mention)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tab_ids: Vec<TabReference>,
    /// What the user's `@` said about which soul should answer.
    ///
    /// Absent means "the user said nothing about it" — keep whatever the session
    /// already had. Present means they did, and [`SoulMention::slug`] carries
    /// either the soul they picked or `None` for "go back to this Space's own".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soul_mention: Option<SoulMention>,
}

/// The user's intent about which soul answers this turn.
///
/// The sidebar resolves `@name` against the soul registry and sends the slug;
/// the daemon never scans message text for mentions, so an email address in a
/// sentence can never switch persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulMention {
    /// The soul to use, or `None` to drop back to this Space's own soul.
    #[serde(default)]
    pub slug: Option<String>,
}

impl SoulMention {
    /// The user picked a soul.
    pub fn soul(slug: impl Into<String>) -> Self {
        Self {
            slug: Some(slug.into()),
        }
    }

    /// The user asked to go back to this Space's own soul.
    pub fn clear() -> Self {
        Self { slug: None }
    }
}

/// Find the `@name` a message is addressed to, if any.
///
/// Only a run at the start of a line or after whitespace counts, so an email
/// address never changes who is answering. The name is returned verbatim; it is
/// the caller's job to look it up in the soul registry, and a name that matches
/// nothing is simply text.
///
/// The first mention wins: a message that names two souls is asking the first one.
pub fn find_mention(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut search_from = 0;

    while let Some(offset) = text[search_from..].find('@') {
        let at = search_from + offset;
        let preceded_by_boundary = at == 0
            || bytes
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_whitespace());

        if preceded_by_boundary {
            let name = &text[at + 1..];
            let end = name
                .find(|c: char| !is_mention_char(c))
                .unwrap_or(name.len());
            if end > 0 {
                return Some(&name[..end]);
            }
        }
        search_from = at + 1;
    }
    None
}

/// Characters a soul name may contain in a mention.
///
/// Deliberately narrow: names are validated on load to be a single word, so
/// stopping at anything else keeps `@alex,` and `@alex.` from swallowing
/// punctuation.
fn is_mention_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// The partial name being typed after a trailing `@`, if the user is mid-mention.
///
/// Returns `Some("")` for a bare `@` (show everyone) and `Some("al")` for `@al`
/// (narrow it down). Returns `None` once the mention is over — a space ends it,
/// because soul names are a single word.
///
/// Only the run at the very end of the text counts: this drives a picker for what
/// is being typed right now, not for an `@` from three sentences ago.
pub fn active_mention_prefix(text: &str) -> Option<&str> {
    let at = text.rfind('@')?;
    let preceded_by_boundary = at == 0
        || text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
    if !preceded_by_boundary {
        return None;
    }

    let typed = &text[at + 1..];
    typed.chars().all(is_mention_char).then_some(typed)
}

/// Whether the text ends in a bare `@` — the user asking who is available.
///
/// Same boundary rule as [`find_mention`]: an `@` glued to a word is part of that
/// word, not a request for the soul picker.
pub fn ends_with_mention_trigger(text: &str) -> bool {
    ends_with_trigger(text, '@')
}

/// Whether the text ends in a bare `#` — the user asking for the tab picker.
///
/// A pasted URL fragment (`…/page#section`) does not qualify, because the `#`
/// is not at a boundary; nor does a Markdown heading in progress (`# `), because
/// the trigger has to be the last character.
pub fn ends_with_tab_trigger(text: &str) -> bool {
    ends_with_trigger(text, '#')
}

fn ends_with_trigger(text: &str, trigger: char) -> bool {
    let Some(rest) = text.strip_suffix(trigger) else {
        return false;
    };
    rest.is_empty() || rest.ends_with(char::is_whitespace)
}

/// Skill command trigger
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillCommandPayload {
    pub session_id: String,
    pub skill_name: String,
    /// Chat mode at command-issue time. The /loop skill uses this to
    /// pick the iteration's tool catalog (chat/browser/agent).
    #[serde(default)]
    pub mode: ChatMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Stop generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopGenerationPayload {
    pub session_id: String,
}

/// Cancel request (user interruption)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelPayload {
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

/// Skip `false` when serializing.
fn is_false(b: &bool) -> bool {
    !*b
}

/// Token accounting for one party (main agent or subagents) within a reply.
///
/// Hand-synced with the daemon's `nevoflux_protocol::UsageBucket` — changes
/// must be made in both places.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UsageBucket {
    /// Input tokens summed over this party's LLM calls.
    pub input: u64,
    /// Output tokens summed over this party's LLM calls.
    pub output: u64,
    /// Number of LLM calls, streaming and non-streaming alike.
    pub calls: u32,
    /// True when at least one call's numbers came from estimation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub estimated: bool,
}

/// Token usage snapshot for one assistant reply.
///
/// Hand-synced with the daemon's `nevoflux_protocol::TurnUsage` — changes must
/// be made in both places. Carries raw facts only; tok/s is derived here from
/// `main.output / decode_ms`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TurnUsage {
    /// The main agent's accounting.
    pub main: UsageBucket,
    /// Subagent accounting, absent when no subagent ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<UsageBucket>,
    /// Input of the main agent's last call, i.e. the current context size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_input: Option<u64>,
    /// Generation time summed over the main agent's streaming calls, in ms.
    /// `None` means no usable window, and tok/s is then not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decode_ms: Option<u64>,
    /// Request-to-first-chunk latency of the first streaming call, in ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token_ms: Option<u64>,
    /// Wall-clock duration of the reply in ms, tool time included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    /// Model used by the main agent's last call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The provider runs its own agent loop, so generation time cannot be
    /// separated from its internal tool execution and tok/s is hidden.
    #[serde(default, skip_serializing_if = "is_false")]
    pub external_agent: bool,
}

/// Stream chunk for streaming responses (matches nevoflux-agent protocol)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    /// Response content
    pub content: String,
    /// Tool calls made by the agent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Whether generation is complete
    #[serde(default)]
    pub done: bool,
    /// Session title (only present when title is generated for first message)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    /// Real-time tool execution event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<ToolEvent>,
    /// Real-time thinking/reasoning event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_event: Option<ThinkingEvent>,
    /// When true, replace accumulated content instead of appending
    #[serde(default)]
    pub replace_content: bool,
    /// Token usage for the reply; only present on the final frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TurnUsage>,
}

/// Tool call information from agent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    /// Tool call arguments — accepts both JSON string and JSON object from the daemon.
    /// The native agent sends `arguments` as `serde_json::Value` (object), but some
    /// code paths expect a string. This custom deserializer handles both.
    #[serde(deserialize_with = "deserialize_string_or_json")]
    pub arguments: String,
}

/// Deserialize a value that can be either a JSON string or a JSON object into a String.
fn deserialize_string_or_json<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
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
    // session_id/error_id/level are optional on the wire: the daemon's
    // generic error sites (e.g. SKILL_TOOLS_UNAVAILABLE in server.rs) emit a
    // minimal `{code, message, recoverable}` payload. Without defaults the
    // untagged IncomingMessage deserialization failed and the message was
    // silently dropped — leaving the sidebar spinner stuck forever. Defaults
    // let handle_error() run (it only needs code/message/recoverable) so the
    // error renders and the spinner clears.
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub error_id: String,
    #[serde(default)]
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

/// Optimistic setup hint pushed by the proxy BEFORE the daemon finishes
/// connecting, so onboarding ("Start Setup") can render during the daemon's
/// cold boot instead of after it. Only sent when no provider is configured.
/// The authoritative `status` system_response reconciles (overrides) these
/// values once the daemon is up — see `setup_authoritative` in the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStatusPayload {
    /// No config file exists yet (first launch).
    #[serde(default)]
    pub first_run: bool,
    /// Whether an LLM provider is already configured.
    #[serde(default)]
    pub has_configured_provider: bool,
    /// True when this is the pre-daemon optimistic hint (vs. authoritative).
    #[serde(default)]
    pub optimistic: bool,
    /// Daemon/proxy version string, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
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
// File Picker (Native Dialog via Agent)
// =============================================================================

/// File picker mode
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PickerMode {
    #[default]
    Files,
    Directories,
    Both,
}

/// Request to open native file picker dialog (Sidebar → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickFilesRequestPayload {
    pub request_id: String,
    #[serde(default)]
    pub mode: PickerMode,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_path: Option<String>,
}

/// File information returned from picker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    #[serde(default)]
    pub is_directory: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Response from native file picker dialog (Agent → Sidebar)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickFilesResponsePayload {
    pub request_id: String,
    #[serde(default)]
    pub files: Vec<FileInfo>,
    #[serde(default)]
    pub cancelled: bool,
}

// =============================================================================
// Plan Proposal (Agent → Sidebar) & Plan Response (Sidebar → Agent)
// =============================================================================

/// A single step in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A proposed plan with summary and ordered steps (Agent → Sidebar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProposalPayload {
    pub summary: String,
    pub steps: Vec<PlanStep>,
}

/// User response to a plan proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanResponse {
    Confirmed,
    Cancelled,
}

/// Plan response payload with session_id (Sidebar → Agent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanResponsePayload {
    pub session_id: String,
    pub response: PlanResponse,
}

/// The plan was decided — possibly somewhere else (Agent → Sidebar).
///
/// The agent parks one oneshot per session, so the first answer settles it and
/// any other surface still showing the panel is offering buttons that decide
/// nothing. Same role as the `browser_tool_resolved` announce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanResolvedPayload {
    pub session_id: String,
    pub response: PlanResponse,
}

/// Agent → Sidebar: this session's contents are gone.
///
/// Sent after `/clear` succeeds, and only then — a failed clear says nothing,
/// so every surface keeps the same (stale) contents rather than one of them
/// showing an emptiness the database does not share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClearedPayload {
    pub session_id: String,
    /// How many messages went. Counts are for the log and any confirmation
    /// copy; nothing depends on them, so an older daemon that omits them is
    /// still understood.
    #[serde(default)]
    pub messages: u32,
    #[serde(default)]
    pub artifacts: usize,
}

/// Agent → Sidebar: the bundled default skills changed since they were last
/// applied; offer to replace the user's skills or keep them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillsUpdateRequestPayload {
    /// Number of skill entries in the bundled defaults (for the prompt copy).
    pub bundled_count: usize,
}

/// Sidebar → Agent: the user's choice for a pending skills update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillsUpdateResponsePayload {
    /// True = replace the user's skills with the bundled defaults; false = keep.
    pub replace: bool,
}

// =============================================================================
// Tool Events (Real-time tool execution state)
// =============================================================================

/// Tool execution event for real-time status in stream chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolEvent {
    /// Tool started executing
    #[serde(rename = "tool_start")]
    Start {
        tool_id: String,
        tool_name: String,
        icon: String,
        summary: String,
    },
    /// Tool waiting for authorization
    #[serde(rename = "tool_auth")]
    Auth {
        tool_id: String,
        request: ToolAuthRequest,
    },
    /// Tool finished executing
    #[serde(rename = "tool_end")]
    End {
        tool_id: String,
        status: ToolEventStatus,
        duration_ms: u64,
        summary: String,
    },
}

/// Tool event completion status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolEventStatus {
    Success,
    Failed,
}

/// Thinking/reasoning event for real-time display in stream chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingEvent {
    /// A new thinking block has started
    #[serde(rename = "thinking_start")]
    Start { thinking_id: String },
    /// Incremental reasoning content
    #[serde(rename = "thinking_delta")]
    Delta { thinking_id: String, content: String },
    /// Thinking block has completed
    #[serde(rename = "thinking_end")]
    End {
        thinking_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
}

/// Authorization request for a tool execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolAuthRequest {
    /// Tool name ("read", "grep", "bash")
    pub tool: String,
    /// Unique tool call ID (for correlating response)
    pub tool_id: String,
    /// Human-readable path or command
    pub detail: String,
    /// Authorization granularity options
    pub options: Vec<AuthOption>,
}

/// A single authorization option presented to the user
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthOption {
    /// Display text, e.g. "Always allow cargo *"
    pub label: String,
    /// Authorization scope
    pub scope: AuthScope,
}

/// Authorization scope
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthScope {
    Once,
    Session,
    Always,
    Deny,
}

/// Tool authorization response from user (Sidebar → Agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAuthResponsePayload {
    pub tool_id: String,
    pub option_index: u32,
    pub scope: AuthScope,
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
    /// Cancel request (user interruption)
    Cancel(CancelPayload),
    /// Permission response
    PermissionResponse(PermissionResponsePayload),
    /// Plugin command
    PluginCommand(PluginCommandPayload),
    /// System command
    SystemCommand(SystemCommandPayload),
    /// Browser tool response
    BrowserToolResponse(BrowserToolResponsePayload),
    /// Pick files request (open native file dialog)
    PickFilesRequest(PickFilesRequestPayload),
    /// Plan response (confirm/cancel)
    PlanResponse(PlanResponsePayload),
    /// Tool authorization response
    ToolAuthResponse(ToolAuthResponsePayload),
    /// Skills update response (replace/keep)
    SkillsUpdateResponse(SkillsUpdateResponsePayload),
    /// Cancel a /loop (Sidebar → Agent). Force=true is the second-click
    /// hard-cancel per spec §8.3; false is the soft cancel that lets the
    /// current iteration finish.
    LoopCancelCommand(LoopCancelCommandPayload),
    /// Ask the daemon to run the `/loop evolve` meta-pass for a loop
    /// (Sidebar → Agent, W4 evolve).
    LoopEvolveCommand(LoopEvolveCommandPayload),
    /// Accept/reject a pending `/loop evolve` proposal (Sidebar → Agent,
    /// W4 evolve).
    LoopProposalRespondCommand(LoopProposalRespondCommandPayload),

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
    /// Pick files response (from native file dialog)
    PickFilesResponse(PickFilesResponsePayload),
    /// Plan proposal from agent
    PlanProposal(PlanProposalPayload),
    /// The plan was answered — possibly on another surface.
    PlanResolved(PlanResolvedPayload),
    /// This session's contents were cleared (`/clear`). The session itself
    /// remains, and the next message lands in it as usual.
    SessionCleared(SessionClearedPayload),
    /// Bundled default skills changed; prompt to replace or keep.
    SkillsUpdateRequest(SkillsUpdateRequestPayload),
    /// Optimistic setup status pushed before the daemon connects, so the
    /// onboarding screen can render during cold boot. Reconciled by the
    /// authoritative `status` system_response.
    SetupStatus(SetupStatusPayload),

    // ========== EventBus ==========
    /// EventBus request (Sidebar -> Agent)
    EventsRequest(crate::events::EventBusRequest),
    /// EventBus response (Agent -> Sidebar)
    EventsResponse(crate::events::EventBusResponse),
    /// EventBus delivery (Agent -> Sidebar)
    EventsDelivery(crate::events::EventBusDelivery),

    // ========== canvas.video ==========
    /// Reply to `canvas_video_reveal_path` (Agent -> Sidebar). Fire-and-forget
    /// from the sidebar's POV but still parsed so the IncomingMessage router
    /// doesn't log a `did not match any variant` warning per click.
    CanvasVideoRevealPathResponse(crate::canvas_video::RevealPathResponsePayload),
}

impl ChatMessage {
    /// Get message direction
    pub fn direction(&self) -> MessageDirection {
        match self {
            // Sidebar → Agent
            Self::ChatMessage(_) |
            Self::SkillCommand(_) |
            Self::StopGeneration(_) |
            Self::Cancel(_) |
            Self::PermissionResponse(_) |
            Self::PluginCommand(_) |
            Self::SystemCommand(_) |
            Self::BrowserToolResponse(_) |
            Self::PickFilesRequest(_) |
            Self::PlanResponse(_) |
            Self::ToolAuthResponse(_) |
            Self::SkillsUpdateResponse(_) |
            Self::LoopCancelCommand(_) |
            Self::LoopEvolveCommand(_) |
            Self::LoopProposalRespondCommand(_) |
            Self::EventsRequest(_) => MessageDirection::ToAgent,

            // Agent → Sidebar
            Self::StreamChunk(_) |
            Self::StreamEnd(_) |
            Self::ContentBlock(_) |
            Self::PermissionRequest(_) |
            Self::AgentState(_) |
            Self::Error(_) |
            Self::AccountStatus(_) |
            Self::SystemResponse(_) |
            Self::BrowserToolRequest(_) |
            Self::PickFilesResponse(_) |
            Self::PlanProposal(_) |
            Self::PlanResolved(_) |
            Self::SessionCleared(_) |
            Self::SkillsUpdateRequest(_) |
            Self::SetupStatus(_) |
            Self::EventsResponse(_) |
            Self::EventsDelivery(_) |
            Self::CanvasVideoRevealPathResponse(_) => MessageDirection::ToSidebar,
        }
    }

    /// Get session_id if present
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::ChatMessage(p) => Some(&p.session_id),
            Self::SkillCommand(p) => Some(&p.session_id),
            Self::StopGeneration(p) => Some(&p.session_id),
            Self::Cancel(p) => Some(&p.session_id),
            Self::PermissionResponse(_) => None,
            Self::PluginCommand(_) => None,
            Self::SystemCommand(_) => None,
            Self::BrowserToolResponse(p) => Some(&p.session_id),
            Self::PickFilesRequest(_) => None,
            Self::PlanResponse(p) => Some(&p.session_id),
            Self::ToolAuthResponse(_) => None,
            Self::SkillsUpdateResponse(_) => None,
            Self::LoopCancelCommand(p) => Some(&p.session_id),
            Self::LoopEvolveCommand(_) => None,
            Self::LoopProposalRespondCommand(_) => None,
            Self::StreamChunk(_) => None, // New protocol doesn't include session_id
            Self::StreamEnd(p) => Some(&p.session_id),
            Self::ContentBlock(p) => Some(&p.session_id),
            Self::PermissionRequest(p) => Some(&p.session_id),
            Self::AgentState(p) => Some(&p.session_id),
            Self::Error(p) => Some(&p.session_id),
            Self::AccountStatus(_) => None,
            Self::SystemResponse(_) => None,
            Self::BrowserToolRequest(p) => Some(&p.session_id),
            Self::PickFilesResponse(_) => None,
            Self::PlanProposal(_) => None,
            Self::PlanResolved(p) => Some(&p.session_id),
            Self::SessionCleared(p) => Some(&p.session_id),
            Self::SkillsUpdateRequest(_) => None,
            Self::SetupStatus(_) => None,
            Self::EventsRequest(_) => None,
            Self::EventsResponse(_) => None,
            Self::EventsDelivery(_) => None,
            Self::CanvasVideoRevealPathResponse(_) => None,
        }
    }
}

// =============================================================================
// /loop skill payloads (spec §11)
// =============================================================================
//
// system:loop:* event payloads — typed deserializers for the JSON content
// arriving via `EventsDelivery`. Sidebar's event-listener routes by topic
// and uses these structs to parse the payload.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopCreatedPayload {
    pub session_id: String,
    pub loop_id: String,
    pub trigger_expr: String,
    #[serde(default)]
    pub prompt_text: Option<String>,
    #[serde(default)]
    pub wrapped_skill: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopStateChangedPayload {
    pub session_id: String,
    pub loop_id: String,
    pub new_state: String,
    pub prev_state: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopIterationStartPayload {
    pub session_id: String,
    pub loop_id: String,
    pub sequence_number: i64,
    pub started_at: i64,
    pub fire_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopIterationEndPayload {
    pub session_id: String,
    pub loop_id: String,
    pub sequence_number: i64,
    pub ended_at: i64,
    pub status: String,
    #[serde(default)]
    pub tool_calls_summary: serde_json::Value,
    /// LLM's final text output for this iteration. Capped at 4KB by the
    /// publisher. `None` for error iterations.
    #[serde(default)]
    pub final_text: Option<String>,
    /// W5 §verify verdict for this iteration's programmatic check. `None`
    /// when the loop has no `verify_check` or the check failed to parse.
    #[serde(default)]
    pub verify_passed: Option<bool>,
    /// Human-readable reason paired with `verify_passed`.
    #[serde(default)]
    pub verify_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopScratchpadChangedPayload {
    pub session_id: String,
    pub loop_id: String,
    pub preview: String,
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopTriggerDroppedPayload {
    pub session_id: String,
    pub loop_id: String,
    pub skipped_count: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopCancelledPayload {
    pub session_id: String,
    pub loop_id: String,
    pub cancelled_by: String,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopCancelCommandPayload {
    pub session_id: String,
    pub loop_id: String,
    #[serde(default)]
    pub force: bool,
}

/// `system:loop:proposal` (W4 evolve) — a self-improvement proposal became
/// pending for a loop. Carries everything the accept/reject UI needs to
/// render the diff without a follow-up fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopProposalPayload {
    pub session_id: String,
    pub loop_id: String,
    pub id: String,
    pub rationale: String,
    #[serde(default)]
    pub proposed_prompt_text: Option<String>,
    #[serde(default)]
    pub proposed_gate_spec: Option<String>,
}

/// `system:loop:proposal_resolved` (W4 evolve) — a pending proposal was
/// accepted or rejected (by a human via `loop_proposal_respond`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopProposalResolvedPayload {
    pub session_id: String,
    pub loop_id: String,
    pub proposal_id: String,
    pub accepted: bool,
}

/// Ask the daemon to run the `/loop evolve` meta-pass for a loop
/// (Sidebar → Agent). Mirrors the `loop_evolve {loop_id}` tool's arg shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopEvolveCommandPayload {
    pub loop_id: String,
}

/// Accept or reject a pending `/loop evolve` proposal (Sidebar → Agent).
/// Mirrors the `loop_proposal_respond {proposal_id, accept}` tool's arg
/// shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopProposalRespondCommandPayload {
    pub proposal_id: String,
    pub accept: bool,
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
    use crate::common::RequesterType;

    // =========================================================================
    // Direction Tests
    // =========================================================================

    #[test]
    fn test_chat_message_direction_to_agent() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
            ..Default::default()
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_skill_command_direction_to_agent() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "s1".to_string(),
            skill_name: "test".to_string(),
            args: None,
            ..Default::default()
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_stop_generation_direction_to_agent() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "s1".to_string(),
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_cancel_direction_to_agent() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "s1".to_string(),
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_permission_response_direction_to_agent() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_plugin_command_direction_to_agent() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "p1".to_string(),
            action: PluginAction::Start,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_system_command_direction_to_agent() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            params: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_browser_tool_response_direction_to_agent() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            success: true,
            result: None,
            error: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToAgent);
    }

    #[test]
    fn test_stream_chunk_direction_to_sidebar() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hi".to_string(),
            tool_calls: vec![],
            done: false,
            session_title: None,
            event: None,
            thinking_event: None,
            ..Default::default()
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_stream_end_direction_to_sidebar() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            metadata: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_content_block_direction_to_sidebar() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "s1".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Markdown,
            content: serde_json::json!("text"),
            metadata: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_permission_request_direction_to_sidebar() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            resource_type: ResourceType::File,
            action: ResourceAction::Read,
            resource: "/path".to_string(),
            requester: Requester {
                requester_type: RequesterType::Agent,
                id: "a1".to_string(),
                name: "Agent".to_string(),
            },
            reason: "Test".to_string(),
            scope: PermissionScope::Once,
            timeout_ms: 60000,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_agent_state_direction_to_sidebar() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "s1".to_string(),
            state: AgentState::Thinking,
            step: None,
            tool: None,
            progress: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_error_direction_to_sidebar() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "s1".to_string(),
            error_id: "e1".to_string(),
            level: ErrorLevel::Error,
            code: "ERR".to_string(),
            message: "Error".to_string(),
            details: None,
            recoverable: false,
            retry_action: None,
            related_request_id: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_account_status_direction_to_sidebar() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: true,
            account: None,
            plan: None,
            quota: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_system_response_direction_to_sidebar() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            success: true,
            data: None,
            error: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    #[test]
    fn test_browser_tool_request_direction_to_sidebar() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            tab_id: None,
            action: BrowserToolAction::Click,
            params: serde_json::json!({}),
            timeout_ms: 30000,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
    }

    // =========================================================================
    // Session ID Tests
    // =========================================================================

    #[test]
    fn test_session_id_chat_message() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "test-session".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
            ..Default::default()
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_skill_command() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "test-session".to_string(),
            skill_name: "test".to_string(),
            args: None,
            ..Default::default()
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stop_generation() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "test-session".to_string(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_cancel() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "test-session".to_string(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_permission_response_none() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_plugin_command_none() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "p1".to_string(),
            action: PluginAction::Stop,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_command_none() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            params: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_browser_tool_response() {
        let msg = ChatMessage::BrowserToolResponse(BrowserToolResponsePayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            success: true,
            result: None,
            error: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_stream_chunk() {
        // New protocol: StreamChunk doesn't have session_id
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello".to_string(),
            tool_calls: vec![],
            done: false,
            session_title: None,
            event: None,
            thinking_event: None,
            ..Default::default()
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_stream_end() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "test-session".to_string(),
            stream_id: "st1".to_string(),
            metadata: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_content_block() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "test-session".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Text,
            content: serde_json::json!("text"),
            metadata: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_permission_request() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            resource_type: ResourceType::Network,
            action: ResourceAction::Connect,
            resource: "https://example.com".to_string(),
            requester: Requester {
                requester_type: RequesterType::Skill,
                id: "skill-1".to_string(),
                name: "Web Skill".to_string(),
            },
            reason: "API call".to_string(),
            scope: PermissionScope::Always,
            timeout_ms: default_timeout(),
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_agent_state() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "test-session".to_string(),
            state: AgentState::Executing,
            step: None,
            tool: None,
            progress: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_error() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "test-session".to_string(),
            error_id: "e1".to_string(),
            level: ErrorLevel::Warning,
            code: "WARN".to_string(),
            message: "Warning".to_string(),
            details: None,
            recoverable: true,
            retry_action: None,
            related_request_id: None,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    #[test]
    fn test_session_id_account_status_none() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: false,
            account: None,
            plan: None,
            quota: None,
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_system_response_none() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "test".to_string(),
            success: false,
            data: None,
            error: Some(SystemError {
                code: "ERR_001".to_string(),
                message: "Failed".to_string(),
            }),
        });
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_session_id_browser_tool_request() {
        let msg = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "test-session".to_string(),
            tab_id: None,
            action: BrowserToolAction::Navigate,
            params: serde_json::json!({"url": "https://example.com"}),
            timeout_ms: 30000,
        });
        assert_eq!(msg.session_id(), Some("test-session"));
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "session-1".to_string(),
            message_id: "msg-1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: None,
            ..Default::default()
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("chat_message"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_skill_command_serialization() {
        let msg = ChatMessage::SkillCommand(SkillCommandPayload {
            session_id: "session-1".to_string(),
            skill_name: "test_skill".to_string(),
            args: Some(serde_json::json!({"key": "value"})),
            ..Default::default()
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("skill_command"));
        assert!(json.contains("test_skill"));
    }

    #[test]
    fn test_stop_generation_serialization() {
        let msg = ChatMessage::StopGeneration(StopGenerationPayload {
            session_id: "session-1".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stop_generation"));
    }

    #[test]
    fn test_cancel_serialization() {
        let msg = ChatMessage::Cancel(CancelPayload {
            session_id: "session-1".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"cancel\""));
        assert!(json.contains("session-1"));
    }

    #[test]
    fn test_permission_response_serialization() {
        let msg = ChatMessage::PermissionResponse(PermissionResponsePayload {
            request_id: "req-1".to_string(),
            granted: true,
            scope: Some(PermissionScope::Session),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("permission_response"));
        assert!(json.contains("granted"));
    }

    #[test]
    fn test_plugin_command_serialization() {
        let msg = ChatMessage::PluginCommand(PluginCommandPayload {
            plugin_id: "plugin-1".to_string(),
            action: PluginAction::Start,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("plugin_command"));
        assert!(json.contains("plugin-1"));
    }

    #[test]
    fn test_system_command_serialization() {
        let msg = ChatMessage::SystemCommand(SystemCommandPayload {
            request_id: "req-1".to_string(),
            command: "skills.list".to_string(),
            params: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system_command"));
        assert!(json.contains("skills.list"));
    }

    #[test]
    fn test_stream_chunk_serialization() {
        let msg = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello".to_string(),
            tool_calls: vec![],
            done: false,
            session_title: None,
            event: None,
            thinking_event: None,
            ..Default::default()
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_stream_end_serialization() {
        let msg = ChatMessage::StreamEnd(StreamEndPayload {
            session_id: "s1".to_string(),
            stream_id: "st1".to_string(),
            metadata: Some(StreamMetadata {
                total_tokens: Some(100),
                duration_ms: Some(500),
                model: Some("claude-3".to_string()),
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("stream_end"));
    }

    #[test]
    fn test_content_block_serialization() {
        let msg = ChatMessage::ContentBlock(ContentBlockPayload {
            session_id: "s1".to_string(),
            block_id: "b1".to_string(),
            content_type: ContentType::Markdown,
            content: serde_json::json!({"text": "Hello"}),
            metadata: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("content_block"));
    }

    #[test]
    fn test_permission_request_serialization() {
        let msg = ChatMessage::PermissionRequest(PermissionRequestPayload {
            request_id: "req-1".to_string(),
            session_id: "s1".to_string(),
            resource_type: ResourceType::File,
            action: ResourceAction::Read,
            resource: "/path/to/file".to_string(),
            requester: Requester {
                requester_type: RequesterType::Agent,
                id: "agent-1".to_string(),
                name: "Test Agent".to_string(),
            },
            reason: "Need to read config".to_string(),
            scope: PermissionScope::Once,
            timeout_ms: 30000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("permission_request"));
    }

    #[test]
    fn test_agent_state_serialization() {
        let msg = ChatMessage::AgentState(AgentStatePayload {
            session_id: "s1".to_string(),
            state: AgentState::Executing,
            step: Some(StepInfo {
                current: 1,
                total: 5,
                description: Some("Processing".to_string()),
            }),
            tool: Some(ToolInfo {
                name: "file_read".to_string(),
                status: ToolStatus::Running,
                target: Some("/etc/config".to_string()),
            }),
            progress: Some(0.5),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("agent_state"));
    }

    #[test]
    fn test_error_serialization() {
        let msg = ChatMessage::Error(ErrorPayload {
            session_id: "session-1".to_string(),
            error_id: "err-1".to_string(),
            level: ErrorLevel::Error,
            code: "LLM_TIMEOUT".to_string(),
            message: "Request timed out".to_string(),
            details: None,
            recoverable: true,
            retry_action: Some("chat_message".to_string()),
            related_request_id: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("LLM_TIMEOUT"));
    }

    #[test]
    fn test_account_status_serialization() {
        let msg = ChatMessage::AccountStatus(AccountStatusPayload {
            logged_in: true,
            account: Some(AccountInfo {
                id: "user-1".to_string(),
                email: "test@example.com".to_string(),
                name: Some("Test User".to_string()),
            }),
            plan: Some(PlanInfo {
                plan_type: PlanType::Pro,
                name: "Pro Plan".to_string(),
                expires_at: Some("2025-01-01".to_string()),
            }),
            quota: Some(QuotaInfo {
                llm_calls: Some(UsageQuota {
                    used: 100,
                    limit: 1000,
                    resets_at: Some("2024-02-01".to_string()),
                }),
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("account_status"));
    }

    #[test]
    fn test_system_response_serialization() {
        let msg = ChatMessage::SystemResponse(SystemResponsePayload {
            request_id: "req-1".to_string(),
            command: "skills.list".to_string(),
            success: true,
            data: Some(serde_json::json!(["skill1", "skill2"])),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system_response"));
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

    // =========================================================================
    // Roundtrip Tests
    // =========================================================================

    #[test]
    fn test_chat_message_roundtrip() {
        let original = ChatMessage::ChatMessage(ChatMessagePayload {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            content: "Hello".to_string(),
            attachments: vec![],
            tab_id: Some(42),
            ..Default::default()
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::ChatMessage(p) => {
                assert_eq!(p.session_id, "s1");
                assert_eq!(p.content, "Hello");
                assert_eq!(p.tab_id, Some(42));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_stream_chunk_roundtrip() {
        let original = ChatMessage::StreamChunk(StreamChunkPayload {
            content: "Hello world".to_string(),
            tool_calls: vec![],
            done: true,
            session_title: None,
            event: None,
            thinking_event: None,
            ..Default::default()
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::StreamChunk(p) => {
                assert_eq!(p.content, "Hello world");
                assert!(p.done);
                assert!(p.tool_calls.is_empty());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_browser_tool_request_roundtrip() {
        let original = ChatMessage::BrowserToolRequest(BrowserToolRequestPayload {
            request_id: "req-123".to_string(),
            session_id: "sess-456".to_string(),
            tab_id: Some(789),
            action: BrowserToolAction::Navigate,
            params: serde_json::json!({"url": "https://example.com"}),
            timeout_ms: 15000,
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ChatMessage::BrowserToolRequest(p) => {
                assert_eq!(p.request_id, "req-123");
                assert_eq!(p.session_id, "sess-456");
                assert_eq!(p.tab_id, Some(789));
                assert_eq!(p.action, BrowserToolAction::Navigate);
                assert_eq!(p.timeout_ms, 15000);
            }
            _ => panic!("Wrong variant"),
        }
    }

    // =========================================================================
    // Default Value Tests
    // =========================================================================

    #[test]
    fn test_setup_status_contract_deserialization() {
        // Exact wire contract sent by the proxy before the daemon connects.
        let json = r#"{"type":"setup_status","payload":{"first_run":true,"has_configured_provider":false,"optimistic":true,"version":"1.2.3"}}"#;
        let parsed: ChatMessage = serde_json::from_str(json).unwrap();
        match parsed {
            ChatMessage::SetupStatus(p) => {
                assert!(p.first_run);
                assert!(!p.has_configured_provider);
                assert!(p.optimistic);
                assert_eq!(p.version.as_deref(), Some("1.2.3"));
            }
            other => panic!("Wrong variant: {other:?}"),
        }
    }

    #[test]
    fn test_setup_status_direction_to_sidebar() {
        let msg = ChatMessage::SetupStatus(SetupStatusPayload {
            first_run: true,
            has_configured_provider: false,
            optimistic: true,
            version: None,
        });
        assert_eq!(msg.direction(), MessageDirection::ToSidebar);
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 60000);
    }

    #[test]
    fn test_default_browser_timeout() {
        assert_eq!(default_browser_timeout(), 30000);
    }
}

#[cfg(test)]
mod mention_tests {
    use super::*;

    // ── find_mention ───────────────────────────────────────────────────

    #[test]
    fn finds_a_mention_at_the_start_or_after_a_space() {
        assert_eq!(find_mention("@alex what do you think?"), Some("alex"));
        assert_eq!(find_mention("hey @alex"), Some("alex"));
        assert_eq!(find_mention("line one\n@alex"), Some("alex"));
    }

    /// An address in a sentence is not a request to switch persona.
    #[test]
    fn an_email_address_is_not_a_mention() {
        assert_eq!(find_mention("mail me at sam@example.com"), None);
        assert_eq!(find_mention("sam@example.com"), None);
    }

    /// A message naming two souls is asking the first one.
    #[test]
    fn the_first_mention_wins() {
        assert_eq!(find_mention("@alex and @nova, thoughts?"), Some("alex"));
    }

    #[test]
    fn a_mention_stops_at_punctuation() {
        assert_eq!(find_mention("@alex, hello"), Some("alex"));
        assert_eq!(find_mention("@alex."), Some("alex"));
        assert_eq!(find_mention("@alex?"), Some("alex"));
        assert_eq!(find_mention("@my-soul_2 hi"), Some("my-soul_2"));
    }

    #[test]
    fn a_bare_at_is_not_a_mention() {
        assert_eq!(find_mention("@"), None);
        assert_eq!(find_mention("what @ even is this"), None);
    }

    /// The name is returned as typed; matching it to a soul is the caller's job,
    /// which is what makes case-insensitive lookup possible without guessing here.
    #[test]
    fn the_name_is_returned_verbatim() {
        assert_eq!(find_mention("@Alex hi"), Some("Alex"));
    }

    /// An email later in the message must not hide a real mention earlier, and a
    /// real mention later must still be found past an email.
    #[test]
    fn scanning_continues_past_a_non_boundary_at() {
        assert_eq!(find_mention("mail sam@example.com then @alex"), Some("alex"));
    }

    // ── active_mention_prefix ──────────────────────────────────────────

    /// A bare `@` asks who is available.
    #[test]
    fn a_bare_at_asks_for_everyone() {
        assert_eq!(active_mention_prefix("@"), Some(""));
        assert_eq!(active_mention_prefix("hey @"), Some(""));
    }

    #[test]
    fn typing_after_an_at_narrows_the_picker() {
        assert_eq!(active_mention_prefix("@al"), Some("al"));
        assert_eq!(active_mention_prefix("hey @nov"), Some("nov"));
    }

    /// A space ends the mention: names are one word.
    #[test]
    fn a_finished_mention_closes_the_picker() {
        assert_eq!(active_mention_prefix("@alex "), None);
        assert_eq!(active_mention_prefix("@alex what do you think"), None);
    }

    /// Only the mention being typed right now counts.
    #[test]
    fn an_earlier_mention_does_not_reopen_the_picker() {
        assert_eq!(active_mention_prefix("@alex said hi"), None);
    }

    /// An email address is not someone typing a mention.
    #[test]
    fn an_email_being_typed_does_not_open_the_picker() {
        assert_eq!(active_mention_prefix("sam@"), None);
        assert_eq!(active_mention_prefix("sam@exa"), None);
    }

    #[test]
    fn text_with_no_at_opens_nothing() {
        assert_eq!(active_mention_prefix("hello"), None);
        assert_eq!(active_mention_prefix(""), None);
    }

    // ── ends_with_mention_trigger / ends_with_tab_trigger ───────────────

    #[test]
    fn a_bare_trigger_opens_the_picker() {
        assert!(ends_with_mention_trigger("@"));
        assert!(ends_with_mention_trigger("hello @"));
        assert!(ends_with_tab_trigger("#"));
        assert!(ends_with_tab_trigger("look at #"));
    }

    /// A pasted URL fragment must not open the tab picker.
    #[test]
    fn a_url_fragment_does_not_open_the_tab_picker() {
        assert!(!ends_with_tab_trigger("https://example.com/page#section"));
        assert!(
            !ends_with_tab_trigger("https://example.com/page#"),
            "the # here is glued to the url, not a trigger"
        );
    }

    /// A Markdown heading in progress must not open the tab picker.
    #[test]
    fn a_markdown_heading_does_not_open_the_tab_picker() {
        assert!(!ends_with_tab_trigger("# Heading"));
        assert!(!ends_with_tab_trigger("## "));
    }

    /// A trigger glued to a word is part of that word.
    #[test]
    fn a_glued_trigger_is_not_a_trigger() {
        assert!(!ends_with_mention_trigger("sam@"));
        assert!(!ends_with_tab_trigger("C#"));
    }

    #[test]
    fn text_without_a_trailing_trigger_opens_nothing() {
        assert!(!ends_with_mention_trigger("@alex"));
        assert!(!ends_with_tab_trigger("#tab"));
        assert!(!ends_with_mention_trigger(""));
    }

    // ── SoulMention on the wire ────────────────────────────────────────

    /// A client that says nothing about souls must not look like one that asked
    /// to change them.
    #[test]
    fn absent_mention_is_absent_on_the_wire() {
        let payload = ChatMessagePayload {
            session_id: "s".into(),
            message_id: "m".into(),
            content: "hi".into(),
            mode: ChatMode::Chat,
            attachments: vec![],
            local_files: vec![],
            tab_id: None,
            tab_ids: vec![],
            soul_mention: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(
            !json.contains("soul_mention"),
            "an absent mention must not be serialized at all"
        );
    }

    #[test]
    fn picking_and_clearing_are_distinguishable_on_the_wire() {
        let pick = serde_json::to_value(SoulMention::soul("research")).unwrap();
        assert_eq!(pick["slug"], "research");

        let clear = serde_json::to_value(SoulMention::clear()).unwrap();
        assert!(clear["slug"].is_null(), "clearing carries no soul");

        let back: SoulMention = serde_json::from_value(pick).unwrap();
        assert_eq!(back.slug.as_deref(), Some("research"));
    }
}

#[cfg(test)]
mod turn_usage_tests {
    use super::*;

    #[test]
    fn done_payload_with_usage_deserializes() {
        let json = r#"{
            "content": "",
            "done": true,
            "usage": {
                "main": {"input": 8329, "output": 646, "calls": 5},
                "subagent": {"input": 4102, "output": 210, "calls": 3, "estimated": true},
                "last_input": 3204,
                "decode_ms": 15300,
                "first_token_ms": 1200,
                "total_ms": 28400,
                "model": "claude-sonnet-5"
            }
        }"#;
        let payload: StreamChunkPayload = serde_json::from_str(json).unwrap();
        let usage = payload.usage.expect("usage present");
        assert_eq!(usage.main.input, 8329);
        assert_eq!(usage.subagent.unwrap().output, 210);
        assert_eq!(usage.decode_ms, Some(15300));
        assert!(!usage.external_agent);
    }

    #[test]
    fn done_payload_without_usage_still_deserializes() {
        let payload: StreamChunkPayload =
            serde_json::from_str(r#"{"content": "hi", "done": true}"#).unwrap();
        assert!(payload.usage.is_none());
    }
}
