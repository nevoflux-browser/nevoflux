// crates/protocol/src/common.rs

//! Common types shared across protocol messages.

use serde::{Deserialize, Serialize};

/// Permission scope for authorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    /// Only this operation
    Once,
    /// This session only
    Session,
    /// Permanently authorized
    Always,
}

/// Resource type for permission requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Script,
    Network,
    Mcp,
    Plugin,
}

/// Resource action for permission requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAction {
    Read,
    Write,
    Execute,
    Connect,
}

/// Requester type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequesterType {
    Agent,
    Plugin,
    Skill,
}

/// Stream format for responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamFormat {
    #[default]
    Markdown,
    Plain,
    Html,
}

/// Content type for content blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Markdown,
    Code,
    A2ui,
    Image,
}

/// Agent execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    #[default]
    Idle,
    Thinking,
    Executing,
    ExecutingTool,
    Waiting,
    WaitingResult,
    WaitingConfirmation,
    Complete,
    Error,
}

/// Tool execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorLevel {
    Warning,
    Error,
    Fatal,
}

/// Plan type for subscriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanType {
    Free,
    Pro,
    Team,
}

/// Plugin action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Start,
    Stop,
    Restart,
}

/// Browser tool action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserToolAction {
    Navigate,
    GoBack,
    GoForward,
    Click,
    Type,
    Fill,
    GetContent,
    Screenshot,
    EvalJs,
    WaitFor,
    Scroll,
    GetElement,
    QueryAll,
    Snapshot,
    ClickById,
    FillById,
    TypeById,
    GetMarkdown,
    /// Wait for page to stabilize after an action.
    ///
    /// Params:
    /// - `strategy`: "navigation" | "interaction" | "scroll"
    /// - `maxWait`: Optional max wait time in ms (default 3000)
    ///
    /// Returns:
    /// - `stable`: boolean
    /// - `strategy`: which strategy was used
    /// - `duration_ms`: how long it took to stabilize
    WaitForStable,
    /// Press a keyboard key (keydown + keyup).
    ///
    /// Params:
    /// - `key`: Key name (e.g., "Enter", "Tab", "Escape")
    /// - `modifiers`: Optional array of modifiers (e.g., ["Ctrl", "Shift"])
    KeyPress,
    /// Fetch URL content and save to cache file.
    ///
    /// Params:
    /// - `url`: URL to fetch
    /// - `timeout_ms`: Optional timeout (default 30000)
    /// - `include_images`: Optional, include images (default false)
    /// - `max_length`: Optional, max content length
    ///
    /// Returns:
    /// - `file_path`: Path to cached markdown file
    /// - `url`: Original URL
    /// - `title`: Page title
    /// - `content_length`: Content size in bytes
    /// - `cached`: Whether result was from cache
    WebFetch,
    /// Perform web search and return results.
    ///
    /// Params:
    /// - `query`: Search query string
    /// - `max_results`: Optional, max number of results (default 10)
    /// - `timeout_ms`: Optional timeout (default 30000)
    ///
    /// Returns:
    /// - `results`: Array of {title, url, snippet}
    /// - `query`: Original query
    /// - `total_results`: Total number of results found
    WebSearch,
    /// Ask the user a question and wait for response.
    ///
    /// Params:
    /// - `question`: Question text to display
    /// - `options`: Array of option strings (can be empty for free text)
    /// - `allow_custom`: Optional, allow custom text input (default true)
    /// - `timeout_ms`: Optional timeout for user response (default 60000)
    ///
    /// Returns:
    /// - `answer`: User's response text
    /// - `is_custom`: Whether the answer was custom input
    /// - `selected_index`: Index of selected option (-1 if custom)
    AskUser,
    /// List all open browser tabs.
    ///
    /// Params: none
    ///
    /// Returns:
    /// - `tabs`: Array of {id, url, title, active, windowId}
    ListTabs,
    /// Query tabs with optional filters.
    ///
    /// Params:
    /// - `url`: Optional glob pattern (e.g., "https://example.com/*")
    /// - `title`: Optional glob pattern (case-insensitive)
    /// - `active`: Optional boolean
    ///
    /// Returns:
    /// - `tabs`: Filtered array of {id, url, title, active, windowId}
    QueryTabs,
    /// Get all interactive elements on the page (alias for Snapshot).
    ///
    /// Params: none
    ///
    /// Returns:
    /// - Array of {id, tag, text, role} for interactive elements
    GetElements,
    /// Read the source code of a canvas artifact.
    ///
    /// Params:
    /// - `id`: Artifact ID (e.g., "art-xxx")
    /// - `offset`: Optional start line (1-based)
    /// - `limit`: Optional number of lines
    /// - `grep`: Optional search keyword
    /// - `context`: Optional lines around grep matches
    ///
    /// Returns:
    /// - `success`: boolean
    /// - `content`: code string
    /// - `totalLines`: total line count
    /// - `truncated`: whether output was truncated
    /// - `title`: artifact title
    /// - `type`: artifact type
    ReadArtifact,
    /// Edit a canvas artifact using search-and-replace.
    ///
    /// Params:
    /// - `id`: Artifact ID
    /// - `old_str`: Exact string to find (must match exactly once)
    /// - `new_str`: Replacement string
    ///
    /// Returns:
    /// - `success`: boolean
    /// - `lines`: new line count (on success)
    /// - `error`: error message (on failure)
    EditArtifact,
    /// Probe an element and return a Fingerprint (PR #2, browser_input strategy engine)
    Probe,
    /// High-level structured input tool (PR #2, browser_input)
    Input,
    /// Paste text into a contentEditable target via synthetic ClipboardEvent + execCommand
    /// (PR #1 Actor method, dispatched by strategy engine Executor)
    Paste,
    /// Replace contentEditable content (clear + paste). (PR #1 Actor method)
    #[serde(rename = "fillRichText")]
    FillRichText,
    /// Upload a file to an <input type="file"> element via HTTP bridge.
    /// (PR #5 browser input file upload)
    #[serde(rename = "uploadFile")]
    UploadFile,
    /// Activate (switch to) a specific tab by ID.
    #[serde(rename = "activateTab")]
    ActivateTab,
    /// Extract visual identity (brand colors / fonts / logo / hero screenshot)
    /// from a URL or existing tab, returning a `VisualIdentity` struct that
    /// can auto-fill a composition's DESIGN.md.
    ///
    /// Used by `/video` Mode 3 (website-to-video) per umbrella spec §6.
    ///
    /// Params:
    /// - `target.url`: URL string (mutually exclusive with `target.tab_id`)
    /// - `target.tab_id`: existing tab ID
    /// - `timeout_sec`: optional, default 20
    /// - `viewport`: optional `[w, h]`, default `[1920, 1080]`
    ///
    /// Returns: `VisualIdentity` JSON (see `nevoflux_protocol::extract`).
    ///
    /// Tab handling: URL mode opens a background tab and closes it after
    /// extraction; TabId mode reuses the existing tab without closing.
    #[serde(rename = "extractVisualIdentity")]
    ExtractVisualIdentity,
}

/// File attachment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Filename
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Base64 encoded data
    pub data: String,
}

/// Entity that requested a permission
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requester {
    /// Type of requester
    #[serde(rename = "type")]
    pub requester_type: RequesterType,
    /// Requester ID
    pub id: String,
    /// Requester display name
    pub name: String,
}

/// Step information for multi-step operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepInfo {
    /// Current step number
    pub current: u32,
    /// Total steps
    pub total: u32,
    /// Step description
    pub description: Option<String>,
}

/// Tool execution information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name
    pub name: String,
    /// Execution status
    pub status: ToolStatus,
    /// Target resource
    pub target: Option<String>,
    /// Emoji icon for sidebar display (auto-populated from name)
    #[serde(default, skip_deserializing)]
    pub icon: String,
}

impl ToolInfo {
    /// Create a new ToolInfo with auto-populated icon.
    pub fn new(name: impl Into<String>, status: ToolStatus, target: Option<String>) -> Self {
        let name = name.into();
        let icon = tool_icon(&name).to_string();
        Self {
            name,
            status,
            target,
            icon,
        }
    }
}

/// Map a tool name to an emoji icon for sidebar display.
///
/// Handles both PascalCase (Claude Code CLI native tools) and
/// lowercase/snake_case (WASM agent custom tools) variants.
pub fn tool_icon(name: &str) -> &'static str {
    match name {
        // CLI / agent file tools
        "Read" | "read" => "\u{1F4C4}", // 📄
        "Write" | "Edit" | "write" | "edit" => "\u{270F}\u{FE0F}", // ✏️
        "NotebookEdit" => "\u{1F4D3}",  // 📓

        // CLI / agent shell tools
        "Bash" | "bash" | "browser_eval_js" => "\u{1F4BB}", // 💻

        // CLI / agent search tools
        "Grep"
        | "Glob"
        | "grep"
        | "glob"
        | "browser_get_elements"
        | "browser_find_elements"
        | "browser_element_info" => {
            "\u{1F50D}" // 🔍
        }

        // CLI / agent web tools
        "WebFetch" | "WebSearch" | "web_fetch" | "web_search" | "browser_navigate"
        | "browser_go_back" | "browser_go_forward" | "navigate" | "goto" | "open_url" => {
            "\u{1F310}" // 🌐
        }

        // Agent-specific tools
        "Task" | "think" => "\u{1F4AD}",                    // 💭
        "plan" => "\u{1F4DD}",                              // 📝
        "switch_model" => "\u{1F504}",                      // 🔄
        "ask_user" => "\u{2753}",                           // ❓
        "memory_search" => "\u{1F9E0}",                     // 🧠
        "skill_load" => "\u{1F4E6}",                        // 📦
        "tool_search" | "tool_call_dynamic" => "\u{1F50E}", // 🔎
        "subagent_spawn" | "subagent_status" | "subagent_wait" | "subagent_kill"
        | "subagent_list" => "\u{1F916}", // 🤖

        // Browser click tools
        "browser_click" | "browser_click_by_id" | "click_element" | "click" => {
            "\u{1F5B1}" // 🖱
        }

        // Browser typing tools
        "browser_type" | "browser_type_by_id" | "browser_fill" | "browser_fill_by_id"
        | "type_text" | "type" | "input" => "\u{2328}\u{FE0F}", // ⌨️

        // Browser screenshot
        "browser_screenshot" | "screenshot" | "capture" => "\u{1F4F7}", // 📷

        // Browser scroll
        "browser_scroll" | "scroll" | "scroll_page" => "\u{2195}\u{FE0F}", // ↕️

        // Browser content extraction
        "browser_get_content" | "browser_get_markdown" | "extract_content" | "get_text" => {
            "\u{1F4CB}"
        } // 📋

        // Browser wait
        "browser_wait_for" | "wait" | "sleep" | "waitForStable" => "\u{23F1}", // ⏱

        // Browser tab management
        "browser_list_tabs" | "browser_query_tabs" | "list_tabs" | "query_tabs" => {
            "\u{1F4C2}" // 📂
        }

        // Artifact editing
        "browser_read_artifact" | "read_artifact" => "\u{1F4C4}", // 📄
        "browser_edit_artifact" | "edit_artifact" => "\u{270F}\u{FE0F}", // ✏️

        // Default
        _ => "\u{2699}\u{FE0F}", // ⚙️
    }
}

/// Stream metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StreamMetadata {
    /// Total tokens used
    pub total_tokens: Option<u32>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Model used
    pub model: Option<String>,
}

/// Account information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

/// Plan information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanInfo {
    #[serde(rename = "type")]
    pub plan_type: PlanType,
    pub name: String,
    pub expires_at: Option<String>,
}

/// Usage quota
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageQuota {
    pub used: u32,
    pub limit: u32,
    pub resets_at: Option<String>,
}

/// Quota information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub llm_calls: Option<UsageQuota>,
}

/// Browser tool error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserToolError {
    pub code: i32,
    pub message: String,
    pub recoverable: bool,
}

/// System error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemError {
    pub code: String,
    pub message: String,
}

/// File picker mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickerMode {
    /// Select files only
    Files,
    /// Select directories only
    Directories,
    /// Select both files and directories
    Both,
}

/// Request to pick files/directories via native dialog
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickFilesRequest {
    /// What type of items can be selected
    pub mode: PickerMode,
    /// Allow multiple selection
    pub multiple: bool,
    /// Dialog title
    pub title: Option<String>,
    /// Default directory to open
    pub default_path: Option<String>,
}

/// Information about a selected file or directory
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    /// Absolute path
    pub path: String,
    /// Whether this is a directory
    pub is_directory: bool,
    /// File size in bytes (None for directories)
    pub size: Option<u64>,
    /// Modification timestamp (Unix seconds)
    pub modified: Option<u64>,
}

/// Type alias for local file references in messages.
/// Uses FileInfo which contains path, is_directory, size, and modified.
pub type LocalFileRef = FileInfo;

/// Response from file picker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickFilesResponse {
    /// Selected files/directories (empty if cancelled)
    pub files: Vec<FileInfo>,
    /// Whether the user cancelled the dialog
    pub cancelled: bool,
}

/// Error from file picker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickFilesError {
    /// Dialog failed to open
    DialogFailed(String),
    /// No display available (Linux headless)
    NoDisplay,
    /// Another dialog is already open
    AlreadyPicking,
}

/// Result of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    /// Total lines in the file.
    pub total_lines: u64,
    /// Total bytes in the file.
    pub total_bytes: u64,
    /// Number of lines actually returned.
    pub returned_lines: u64,
    /// Start line offset (0-based).
    pub offset: u64,
    /// File content (may be truncated).
    pub content: String,
    /// Whether content was truncated.
    pub truncated: bool,
}

/// A single grep match entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    /// File path of the match.
    pub file: String,
    /// Line number (1-based).
    pub line: u64,
    /// Content of the matching line.
    pub content: String,
}

/// Result of a grep search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    /// Total matches found (even if exceeding max_results).
    pub total_matches: u64,
    /// Number of files with matches.
    pub total_files: u64,
    /// Number of results returned.
    pub returned: u64,
    /// Match results.
    pub results: Vec<GrepMatch>,
    /// Whether results were truncated.
    pub truncated: bool,
}

/// Status of a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BashStatus {
    Success,
    Error,
    Timeout,
    Killed,
}

/// Result of a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashResult {
    /// Exit code, None means timeout or killed.
    pub exit_code: Option<i32>,
    /// Execution status.
    pub status: BashStatus,
    /// Total output lines (before truncation).
    pub total_lines: u64,
    /// Total output bytes (before truncation).
    pub total_bytes: u64,
    /// Lines actually returned.
    pub returned_lines: u64,
    /// stdout content (may be truncated).
    pub stdout: String,
    /// stderr content (only on failure).
    pub stderr: Option<String>,
    /// Whether output was truncated.
    pub truncated: bool,
    /// Hint for model (timeout, binary, etc.).
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_scope_serialization() {
        assert_eq!(
            serde_json::to_string(&PermissionScope::Once).unwrap(),
            "\"once\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionScope::Session).unwrap(),
            "\"session\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionScope::Always).unwrap(),
            "\"always\""
        );
    }

    #[test]
    fn test_resource_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ResourceType::File).unwrap(),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&ResourceType::Script).unwrap(),
            "\"script\""
        );
    }

    #[test]
    fn test_agent_state_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentState::Idle).unwrap(),
            "\"idle\""
        );
        assert_eq!(
            serde_json::to_string(&AgentState::Thinking).unwrap(),
            "\"thinking\""
        );
        assert_eq!(
            serde_json::to_string(&AgentState::ExecutingTool).unwrap(),
            "\"executing_tool\""
        );
    }

    #[test]
    fn test_error_level_serialization() {
        assert_eq!(
            serde_json::to_string(&ErrorLevel::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorLevel::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorLevel::Fatal).unwrap(),
            "\"fatal\""
        );
    }

    #[test]
    fn test_attachment_roundtrip() {
        let attachment = Attachment {
            name: "test.txt".into(),
            mime_type: "text/plain".into(),
            data: "SGVsbG8gV29ybGQ=".into(),
        };
        let json = serde_json::to_string(&attachment).unwrap();
        let decoded: Attachment = serde_json::from_str(&json).unwrap();
        assert_eq!(attachment, decoded);
    }

    #[test]
    fn test_requester_roundtrip() {
        let requester = Requester {
            requester_type: RequesterType::Agent,
            id: "nevoflux-agent".into(),
            name: "NevoFlux Agent".into(),
        };
        let json = serde_json::to_string(&requester).unwrap();
        let decoded: Requester = serde_json::from_str(&json).unwrap();
        assert_eq!(requester, decoded);
    }

    #[test]
    fn test_picker_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&PickerMode::Files).unwrap(),
            "\"files\""
        );
        assert_eq!(
            serde_json::to_string(&PickerMode::Directories).unwrap(),
            "\"directories\""
        );
        assert_eq!(
            serde_json::to_string(&PickerMode::Both).unwrap(),
            "\"both\""
        );
    }

    #[test]
    fn test_pick_files_request_roundtrip() {
        let req = PickFilesRequest {
            mode: PickerMode::Both,
            multiple: true,
            title: Some("Select files".into()),
            default_path: Some("/home/user".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: PickFilesRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_file_info_roundtrip() {
        let info = FileInfo {
            path: "/home/user/test.txt".into(),
            is_directory: false,
            size: Some(1024),
            modified: Some(1706600000),
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: FileInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, decoded);
    }

    #[test]
    fn test_pick_files_response_roundtrip() {
        let resp = PickFilesResponse {
            files: vec![FileInfo {
                path: "/home/user/test.txt".into(),
                is_directory: false,
                size: Some(1024),
                modified: Some(1706600000),
            }],
            cancelled: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: PickFilesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_pick_files_error_serialization() {
        let err = PickFilesError::NoDisplay;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("no_display"));
    }

    #[test]
    fn test_tool_icon_cli_tools() {
        // PascalCase (Claude Code CLI native tool names)
        assert_eq!(tool_icon("Read"), "\u{1F4C4}");
        assert_eq!(tool_icon("Write"), "\u{270F}\u{FE0F}");
        assert_eq!(tool_icon("Edit"), "\u{270F}\u{FE0F}");
        assert_eq!(tool_icon("Bash"), "\u{1F4BB}");
        assert_eq!(tool_icon("Grep"), "\u{1F50D}");
        assert_eq!(tool_icon("Glob"), "\u{1F50D}");
        assert_eq!(tool_icon("WebFetch"), "\u{1F310}");
        assert_eq!(tool_icon("WebSearch"), "\u{1F310}");
        assert_eq!(tool_icon("Task"), "\u{1F4AD}");
        assert_eq!(tool_icon("NotebookEdit"), "\u{1F4D3}");
    }

    #[test]
    fn test_tool_icon_wasm_agent_tools() {
        // lowercase/snake_case (WASM agent custom tool names)
        assert_eq!(tool_icon("read"), "\u{1F4C4}");
        assert_eq!(tool_icon("write"), "\u{270F}\u{FE0F}");
        assert_eq!(tool_icon("edit"), "\u{270F}\u{FE0F}");
        assert_eq!(tool_icon("bash"), "\u{1F4BB}");
        assert_eq!(tool_icon("grep"), "\u{1F50D}");
        assert_eq!(tool_icon("glob"), "\u{1F50D}");
        assert_eq!(tool_icon("web_fetch"), "\u{1F310}");
        assert_eq!(tool_icon("web_search"), "\u{1F310}");
        assert_eq!(tool_icon("think"), "\u{1F4AD}");
        assert_eq!(tool_icon("plan"), "\u{1F4DD}");
        assert_eq!(tool_icon("switch_model"), "\u{1F504}");
        assert_eq!(tool_icon("ask_user"), "\u{2753}");
        assert_eq!(tool_icon("memory_search"), "\u{1F9E0}");
        assert_eq!(tool_icon("skill_load"), "\u{1F4E6}");
    }

    #[test]
    fn test_tool_icon_browser_tools() {
        assert_eq!(tool_icon("browser_click"), "\u{1F5B1}");
        assert_eq!(tool_icon("browser_click_by_id"), "\u{1F5B1}");
        assert_eq!(tool_icon("browser_type"), "\u{2328}\u{FE0F}");
        assert_eq!(tool_icon("browser_type_by_id"), "\u{2328}\u{FE0F}");
        assert_eq!(tool_icon("browser_fill"), "\u{2328}\u{FE0F}");
        assert_eq!(tool_icon("browser_fill_by_id"), "\u{2328}\u{FE0F}");
        assert_eq!(tool_icon("browser_screenshot"), "\u{1F4F7}");
        assert_eq!(tool_icon("browser_scroll"), "\u{2195}\u{FE0F}");
        assert_eq!(tool_icon("browser_navigate"), "\u{1F310}");
        assert_eq!(tool_icon("browser_get_content"), "\u{1F4CB}");
        assert_eq!(tool_icon("browser_get_markdown"), "\u{1F4CB}");
        assert_eq!(tool_icon("browser_wait_for"), "\u{23F1}");
        assert_eq!(tool_icon("browser_eval_js"), "\u{1F4BB}");
        assert_eq!(tool_icon("browser_get_elements"), "\u{1F50D}");
        assert_eq!(tool_icon("browser_find_elements"), "\u{1F50D}");
        assert_eq!(tool_icon("browser_element_info"), "\u{1F50D}");
    }

    #[test]
    fn test_tool_icon_subagent_tools() {
        assert_eq!(tool_icon("subagent_spawn"), "\u{1F916}");
        assert_eq!(tool_icon("subagent_status"), "\u{1F916}");
        assert_eq!(tool_icon("subagent_wait"), "\u{1F916}");
        assert_eq!(tool_icon("subagent_kill"), "\u{1F916}");
        assert_eq!(tool_icon("subagent_list"), "\u{1F916}");
    }

    #[test]
    fn test_tool_icon_dynamic_tools() {
        assert_eq!(tool_icon("tool_search"), "\u{1F50E}");
        assert_eq!(tool_icon("tool_call_dynamic"), "\u{1F50E}");
    }

    #[test]
    fn test_tool_icon_unknown_returns_default() {
        assert_eq!(tool_icon("unknown_tool"), "\u{2699}\u{FE0F}");
        assert_eq!(tool_icon("custom_mcp_tool"), "\u{2699}\u{FE0F}");
    }

    #[test]
    fn test_tool_info_new_populates_icon() {
        let info = ToolInfo::new("Bash", ToolStatus::Running, None);
        assert_eq!(info.name, "Bash");
        assert_eq!(info.icon, "\u{1F4BB}");
        assert_eq!(info.status, ToolStatus::Running);
        assert!(info.target.is_none());
    }

    #[test]
    fn test_tool_info_serialization_includes_icon() {
        let info = ToolInfo::new("browser_click", ToolStatus::Success, Some("/page".into()));
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"icon\""));
        assert!(json.contains("\u{1F5B1}"));
    }

    #[test]
    fn test_tool_info_deserialization_without_icon() {
        // Sidebar may send ToolInfo without icon field — should deserialize fine
        let json = r#"{"name":"Read","status":"running","target":null}"#;
        let info: ToolInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "Read");
        // icon defaults to empty when deserialized (skip_deserializing)
        assert_eq!(info.icon, "");
    }
}

#[cfg(test)]
mod tool_result_tests {
    use super::*;

    #[test]
    fn test_read_result_serialization_roundtrip() {
        let result = ReadResult {
            total_lines: 100,
            total_bytes: 4096,
            returned_lines: 50,
            offset: 10,
            content: "fn main() {\n    println!(\"hello\");\n}".into(),
            truncated: true,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ReadResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_lines, 100);
        assert_eq!(decoded.total_bytes, 4096);
        assert_eq!(decoded.returned_lines, 50);
        assert_eq!(decoded.offset, 10);
        assert_eq!(decoded.content, "fn main() {\n    println!(\"hello\");\n}");
        assert!(decoded.truncated);
    }

    #[test]
    fn test_read_result_empty_file() {
        let result = ReadResult {
            total_lines: 0,
            total_bytes: 0,
            returned_lines: 0,
            offset: 0,
            content: String::new(),
            truncated: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ReadResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_lines, 0);
        assert_eq!(decoded.total_bytes, 0);
        assert_eq!(decoded.returned_lines, 0);
        assert_eq!(decoded.offset, 0);
        assert_eq!(decoded.content, "");
        assert!(!decoded.truncated);
    }

    #[test]
    fn test_grep_result_serialization_roundtrip() {
        let result = GrepResult {
            total_matches: 5,
            total_files: 3,
            returned: 3,
            results: vec![
                GrepMatch {
                    file: "src/main.rs".into(),
                    line: 10,
                    content: "fn main() {".into(),
                },
                GrepMatch {
                    file: "src/lib.rs".into(),
                    line: 25,
                    content: "pub fn init() {".into(),
                },
                GrepMatch {
                    file: "tests/test.rs".into(),
                    line: 1,
                    content: "fn test_main() {".into(),
                },
            ],
            truncated: true,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: GrepResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_matches, 5);
        assert_eq!(decoded.total_files, 3);
        assert_eq!(decoded.returned, 3);
        assert_eq!(decoded.results.len(), 3);
        assert_eq!(decoded.results[0].file, "src/main.rs");
        assert_eq!(decoded.results[0].line, 10);
        assert_eq!(decoded.results[0].content, "fn main() {");
        assert_eq!(decoded.results[1].file, "src/lib.rs");
        assert_eq!(decoded.results[2].line, 1);
        assert!(decoded.truncated);
    }

    #[test]
    fn test_grep_result_no_matches() {
        let result = GrepResult {
            total_matches: 0,
            total_files: 0,
            returned: 0,
            results: vec![],
            truncated: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: GrepResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_matches, 0);
        assert_eq!(decoded.total_files, 0);
        assert_eq!(decoded.returned, 0);
        assert!(decoded.results.is_empty());
        assert!(!decoded.truncated);
    }

    #[test]
    fn test_bash_result_success() {
        let result = BashResult {
            exit_code: Some(0),
            status: BashStatus::Success,
            total_lines: 10,
            total_bytes: 256,
            returned_lines: 10,
            stdout: "Hello, world!\n".into(),
            stderr: None,
            truncated: false,
            hint: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: BashResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.exit_code, Some(0));
        assert_eq!(decoded.total_lines, 10);
        assert_eq!(decoded.total_bytes, 256);
        assert_eq!(decoded.returned_lines, 10);
        assert_eq!(decoded.stdout, "Hello, world!\n");
        assert!(decoded.stderr.is_none());
        assert!(!decoded.truncated);
        assert!(decoded.hint.is_none());
    }

    #[test]
    fn test_bash_result_timeout() {
        let result = BashResult {
            exit_code: None,
            status: BashStatus::Timeout,
            total_lines: 0,
            total_bytes: 0,
            returned_lines: 0,
            stdout: String::new(),
            stderr: Some("command timed out after 30s".into()),
            truncated: false,
            hint: Some("Command exceeded timeout limit".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: BashResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.exit_code.is_none());
        assert_eq!(
            decoded.stderr.as_deref(),
            Some("command timed out after 30s")
        );
        assert_eq!(
            decoded.hint.as_deref(),
            Some("Command exceeded timeout limit")
        );
    }

    #[test]
    fn test_bash_status_serialization() {
        // Verify serde rename_all = "snake_case" works correctly
        assert_eq!(
            serde_json::to_string(&BashStatus::Success).unwrap(),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&BashStatus::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&BashStatus::Timeout).unwrap(),
            "\"timeout\""
        );
        assert_eq!(
            serde_json::to_string(&BashStatus::Killed).unwrap(),
            "\"killed\""
        );

        // Verify deserialization
        let status: BashStatus = serde_json::from_str("\"success\"").unwrap();
        assert!(matches!(status, BashStatus::Success));
        let status: BashStatus = serde_json::from_str("\"timeout\"").unwrap();
        assert!(matches!(status, BashStatus::Timeout));
    }
}
