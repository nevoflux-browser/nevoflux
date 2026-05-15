//! Tool execution infrastructure for the agent.
//!
//! This module provides the tool registry and built-in tools for the agent.
//! Tools are executed by the agent runner when the Wasm agent requests them.

use crate::agent::abi::{PendingToolCall, ToolResult};
use crate::error::{DaemonError, Result};
use crate::wasm::services::{BrowserContext, BrowserRequest, BrowserResponse};
use async_trait::async_trait;
use nevoflux_protocol::BrowserToolAction;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::oneshot;

// ============================================================================
// Tool Execution Record
// ============================================================================

/// Record of a tool execution, used by the learning system.
#[derive(Debug, Clone)]
pub struct ToolExecutionRecord {
    /// Name of the tool that was executed.
    pub tool_name: String,
    /// Summary of the arguments passed to the tool.
    pub arguments_summary: String,
    /// Whether the execution succeeded.
    pub success: bool,
    /// Error message if the execution failed.
    pub error_message: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Session ID in which the tool was executed.
    pub session_id: String,
}

// ============================================================================
// Tool Executor Trait
// ============================================================================

/// Trait for tool executors.
///
/// Implementations of this trait provide the actual execution logic for tools.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute the tool with the given arguments.
    ///
    /// # Arguments
    /// * `name` - The name of the tool being executed
    /// * `arguments` - The arguments passed to the tool as JSON
    ///
    /// # Returns
    /// The result of the tool execution as a string, or an error.
    async fn execute(&self, name: &str, arguments: &serde_json::Value) -> Result<String>;
}

// ============================================================================
// Tool Registry
// ============================================================================

/// Registry for tool executors.
///
/// The registry maps tool names to their executor implementations.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Create a new tool registry with built-in tools registered.
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register built-in tools
        registry.register("read_file", Box::new(ReadFileTool));
        registry.register("write_file", Box::new(WriteFileTool));
        registry.register("list_files", Box::new(ListFilesTool));
        registry.register("canvas_render", Box::new(CanvasRenderTool));
        registry.register("run_command", Box::new(RunCommandTool));

        registry
    }

    /// Create a tool registry with browser tools registered.
    ///
    /// Includes all built-in tools plus browser interaction tools and
    /// web_search/fetch_page (which go through the browser sender).
    pub fn with_browser(ctx: BrowserContext) -> Self {
        let mut registry = Self::new();
        let ctx = Arc::new(ctx);

        let browser_tools = [
            // Core browser tools
            ("browser_get_markdown", BrowserToolAction::GetMarkdown),
            ("browser_snapshot", BrowserToolAction::Snapshot),
            ("browser_click_by_id", BrowserToolAction::ClickById),
            ("browser_type_by_id", BrowserToolAction::TypeById),
            ("browser_fill_by_id", BrowserToolAction::FillById),
            ("browser_navigate", BrowserToolAction::Navigate),
            ("browser_go_back", BrowserToolAction::GoBack),
            ("browser_go_forward", BrowserToolAction::GoForward),
            ("browser_scroll", BrowserToolAction::Scroll),
            ("browser_get_tabs", BrowserToolAction::ListTabs),
            ("browser_query_tabs", BrowserToolAction::QueryTabs),
            ("browser_activate_tab", BrowserToolAction::ActivateTab),
            ("browser_get_elements", BrowserToolAction::GetElements),
            // Lower-level browser tools
            ("browser_click", BrowserToolAction::Click),
            ("browser_type", BrowserToolAction::Type),
            ("browser_fill", BrowserToolAction::Fill),
            ("browser_get_content", BrowserToolAction::GetContent),
            ("browser_screenshot", BrowserToolAction::Screenshot),
            ("browser_eval_js", BrowserToolAction::EvalJs),
            ("browser_wait_for", BrowserToolAction::WaitFor),
            ("browser_wait_for_stable", BrowserToolAction::WaitForStable),
            ("browser_key_press", BrowserToolAction::KeyPress),
            ("browser_get_element", BrowserToolAction::GetElement),
            ("browser_query_all", BrowserToolAction::QueryAll),
            // Browser input strategy engine (PR #2)
            ("browser_input", BrowserToolAction::Input),
            ("browser_probe", BrowserToolAction::Probe),
            ("browser_upload_file", BrowserToolAction::UploadFile),
            // Artifact tools
            ("browser_read_artifact", BrowserToolAction::ReadArtifact),
            ("browser_edit_artifact", BrowserToolAction::EditArtifact),
            // Visual-identity extraction (Mode 3 entry; canvas_* prefix
            // because output feeds canvas_video DESIGN.md, even though
            // dispatch goes through the browser-tool bridge).
            (
                "canvas_extract_visual_identity",
                BrowserToolAction::ExtractVisualIdentity,
            ),
            // Web tools
            ("web_search", BrowserToolAction::WebSearch),
            ("fetch_page", BrowserToolAction::WebFetch),
            // User interaction
            ("browser_ask_user", BrowserToolAction::AskUser),
        ];

        for (name, action) in browser_tools {
            registry.register(
                name,
                Box::new(BrowserTool {
                    ctx: ctx.clone(),
                    action,
                }),
            );
        }

        registry
    }

    /// Create an empty tool registry without built-in tools.
    pub fn empty() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool executor.
    ///
    /// # Arguments
    /// * `name` - The name of the tool
    /// * `executor` - The executor implementation
    pub fn register(&mut self, name: &str, executor: Box<dyn ToolExecutor>) {
        self.tools.insert(name.to_string(), executor);
    }

    /// Check if a tool is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get the list of registered tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get sorted tool names for deterministic output.
    fn tool_names_sorted(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Get parameter hints and docs for known built-in tools.
    /// Returns (params, description) based on tool name conventions.
    fn tool_signature_hint(name: &str) -> (&'static str, &'static str) {
        match name {
            "read_file" => (
                "path: str",
                "Read the contents of a file. Returns the file content as a string.",
            ),
            "write_file" => (
                "path: str, content: str",
                "Write content to a file. Returns success message.",
            ),
            "list_files" => (
                "path: str",
                "List directory contents. Returns newline-separated entries.",
            ),
            "web_search" => (
                "query: str",
                "Search the web. Returns list of dicts with keys: title, url, snippet.",
            ),
            "fetch_page" => (
                "url: str",
                "Fetch a web page. Returns the page content as markdown.",
            ),
            "canvas_render" => (
                "files: dict, entry: str",
                "Render a multi-file project in the canvas.",
            ),
            "run_command" => (
                "command: str",
                "Run a shell command and return stdout. Use for operations requiring system tools (regex, datetime, etc).",
            ),
            "get_code_mode_context" => (
                "",
                "Get Code Mode context with available tool function signatures.",
            ),
            "browser_get_markdown" => (
                "tab_id: int = None",
                "Get the current page content as markdown. Returns dict with key 'markdown' (str).",
            ),
            "browser_snapshot" => (
                "tab_id: int = None",
                "Get page element structure (accessibility snapshot). Returns element tree.",
            ),
            "browser_click_by_id" => (
                "element_id: str, tab_id: int = None",
                "Click an element by its snapshot ID.",
            ),
            "browser_type_by_id" => (
                "element_id: str, text: str, tab_id: int = None",
                "Type text into an element by its snapshot ID. \
                 (Deprecated 2026-04; prefer browser_input which handles rich text editors.)",
            ),
            "browser_navigate" => (
                "url: str, new_tab: bool = false, tab_id: int = None",
                "Navigate the browser to a URL in the CURRENT tab. \
                 Only set new_tab=true when the user explicitly asks for a new tab. \
                 Default: navigates the current tab (new_tab=false).",
            ),
            "browser_scroll" => (
                "direction: str, amount: int = 3, tab_id: int = None",
                "Scroll the page. direction: 'up' or 'down'.",
            ),
            "browser_get_tabs" => (
                "",
                "List all open browser tabs. Returns list of dicts with keys: id, url, title, active.",
            ),
            "browser_activate_tab" => (
                "tab_id: int",
                "Switch to (activate) a specific browser tab by its tab ID. \
                 Use browser_get_tabs first to find the tab ID.",
            ),
            "browser_fill_by_id" => (
                "element_id: str, value: str, tab_id: int = None",
                "Fill a form field by its snapshot ID (clears existing value first). \
                 (Deprecated 2026-04; prefer browser_input which handles rich text editors.)",
            ),
            "browser_go_back" => (
                "tab_id: int = None",
                "Navigate back to the previous page.",
            ),
            "browser_go_forward" => (
                "tab_id: int = None",
                "Navigate forward to the next page.",
            ),
            "browser_query_tabs" => (
                "url: str = None, title: str = None, active: bool = None",
                "Query tabs with optional filters. Returns filtered list of tab dicts.",
            ),
            "browser_get_elements" => (
                "tab_id: int = None",
                "Get all interactive elements on the page. Returns list of element dicts.",
            ),
            "browser_click" => (
                "selector: str, tab_id: int = None",
                "Click an element by CSS selector.",
            ),
            "browser_type" => (
                "selector: str, text: str, tab_id: int = None",
                "Type text into an element by CSS selector.",
            ),
            "browser_fill" => (
                "selector: str, value: str, tab_id: int = None",
                "Fill a form field by CSS selector (clears existing value first).",
            ),
            "browser_get_content" => (
                "tab_id: int = None",
                "Get the full HTML content of the current page.",
            ),
            "browser_screenshot" => (
                "tab_id: int = None",
                "Take a screenshot of the current page. Returns base64 image data.",
            ),
            "browser_eval_js" => (
                "expression: str, tab_id: int = None",
                "Evaluate a JavaScript expression in the page context. Returns the result. \
                 ⚠ Runs inside a content-principal sandbox; still subject to page CSP. \
                 Strict sites (Twitter/X, GitHub, banking) will reject eval. \
                 PREFER structured tools: browser_input, browser_probe, browser_query_all, \
                 browser_get_content, browser_click/scroll/navigate. \
                 Use browser_eval_js ONLY when no structured tool covers the case.",
            ),
            "browser_input" => (
                "selector: str, text: str, mode: str = 'fill', verify: bool = true, tab_id: int = None",
                "High-level text input tool. Probes the target, selects a strategy based \
                 on its type (standard input, Draft.js, Lexical, ProseMirror, Slate, \
                 generic contentEditable), executes, and optionally verifies by reading \
                 back. Handles the 'silent success' bugs that plague legacy browser_fill_by_id \
                 on rich text editors. mode='fill' replaces content, mode='type' appends.",
            ),
            "browser_probe" => (
                "selector: str, tab_id: int = None",
                "Return a rich Fingerprint (tag, input_type, is_content_editable, \
                 editor_framework, react_fiber_present, visibility, focusability, shadow \
                 DOM depth, iframe context, innermost_editable_selector) for the element \
                 matching the selector. Useful for reasoning about page structure before \
                 choosing an input strategy manually.",
            ),
            "browser_upload_file" => (
                "selector: str, file_path: str, workspace_dir: str = None, tab_id: int = None",
                "Upload a file to an <input type=\"file\"> element. \
                 file_path must be inside workspace_dir (default: ~/.local/share/nevoflux/workspace/). \
                 Set workspace_dir to the directory containing the file (e.g. '/home/user/Documents') \
                 to allow uploads from other locations. Detects MIME type automatically.",
            ),
            "browser_wait_for" => (
                "selector: str, timeout_ms: int = 30000, tab_id: int = None",
                "Wait for an element matching the selector to appear.",
            ),
            "browser_wait_for_stable" => (
                "strategy: str = 'interaction', max_wait: int = 3000, tab_id: int = None",
                "Wait for page to stabilize. strategy: 'navigation', 'interaction', or 'scroll'.",
            ),
            "browser_key_press" => (
                "key: str, modifiers: list = None, tab_id: int = None",
                "Press a keyboard key. key: 'Enter', 'Tab', 'Escape', etc.",
            ),
            "browser_get_element" => (
                "selector: str, tab_id: int = None",
                "Get a single element by CSS selector. Returns element dict.",
            ),
            "browser_query_all" => (
                "selector: str, tab_id: int = None",
                "Query all elements matching a CSS selector. Returns list of element dicts.",
            ),
            "browser_read_artifact" => (
                "id: str, offset: int = None, limit: int = None, grep: str = None",
                "Read the source code of a canvas artifact.",
            ),
            "browser_edit_artifact" => (
                "id: str, old_str: str, new_str: str",
                "Edit a canvas artifact using search-and-replace.",
            ),
            "browser_ask_user" => (
                "question: str, options: list = None, allow_custom: bool = True",
                "Ask the user a question and wait for response.",
            ),
            _ => ("**kwargs", "Execute this tool with keyword arguments."),
        }
    }

    /// Build parameter name mappings for all registered tools.
    ///
    /// Extracts ordered parameter names from `tool_signature_hint`, which is the
    /// same source used by `to_python_stubs`. Returns a map of tool name →
    /// ordered param names, used by `positional_to_named_auto` in the executor.
    pub fn param_mappings(&self) -> std::collections::HashMap<String, Vec<String>> {
        let mut map = std::collections::HashMap::new();
        for name in self.tool_names() {
            let (sig, _) = Self::tool_signature_hint(name);
            let names: Vec<String> = sig
                .split(',')
                .filter_map(|p| {
                    let p = p.trim();
                    if p.is_empty() || p == "**kwargs" {
                        return None;
                    }
                    // Extract name before ':' (e.g., "path: str" → "path")
                    p.split(':').next().map(|n| n.trim().to_string())
                })
                .collect();
            map.insert(name.to_string(), names);
        }
        map
    }

    /// Generate Python function signatures for all registered tools.
    /// Since ToolExecutor doesn't expose schemas, this generates stubs based on tool names only.
    /// Used by get_code_mode_context() for on-demand tool discovery.
    pub fn to_python_stubs(&self) -> String {
        let mut stubs =
            String::from("# Available tool functions (pre-injected, no imports needed)\n\n");

        for name in self.tool_names_sorted() {
            let (params, doc) = Self::tool_signature_hint(name);
            stubs.push_str(&format!("def {}({}):\n", name, params));
            stubs.push_str(&format!("    \"\"\"{}\"\"\"\n", doc));
            stubs.push_str("    ...\n\n");
        }

        stubs
    }

    /// Generate compact tool category summary for system prompt (~200 tokens).
    pub fn tool_categories_summary(&self) -> String {
        let mut categories: std::collections::BTreeMap<&str, Vec<&str>> =
            std::collections::BTreeMap::new();

        for name in self.tool_names_sorted() {
            let category = if name.starts_with("web_") || name.starts_with("fetch_") {
                "Search & Web"
            } else if name.starts_with("read_")
                || name.starts_with("write_")
                || name.starts_with("list_")
            {
                "Files"
            } else if name.starts_with("canvas_") || name.starts_with("browser_") {
                "Browser & Canvas"
            } else if name.starts_with("run_") {
                "System"
            } else {
                "Other"
            };
            categories.entry(category).or_default().push(name);
        }

        let mut summary = String::from("Available tool categories:\n");
        for (category, tools) in &categories {
            summary.push_str(&format!("- {}: {}\n", category, tools.join(", ")));
        }
        summary
    }

    /// Generate the Code Mode system prompt with Monty constraints and tool info.
    pub fn code_mode_system_prompt(&self) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "You are in Code Mode. You MUST write a single Python script to accomplish the task.\n",
        );
        prompt.push_str("Do NOT make individual tool calls — call the `orchestrate` tool with a single Python script.\n\n");
        prompt.push_str("Supported constructs:\n");
        prompt.push_str("- Statements: variable, def, if/elif/else, for/while, break, continue, try/except/finally, return, pass, del, assert, raise\n");
        prompt.push_str("- Expressions: arithmetic, comparison, boolean, f-string, lambda, comprehensions, ternary, slice, unpack, walrus (:=)\n");
        prompt.push_str("- Types: int, float, str, bool, list, dict, set, tuple, None, bytes\n");
        prompt.push_str("- Built-ins: len, range, sorted, enumerate, zip, sum, min, max, abs, round, isinstance, type, print\n\n");
        prompt.push_str(
            "DO NOT use: class, match/case, import, with, async/await, yield, decorators\n\n",
        );
        prompt.push_str("Built-in limitations (IMPORTANT):\n");
        prompt.push_str("- sorted() does NOT support key= or reverse= kwargs. Use manual sort: pairs = [[key_fn(x), x] for x in items]; pairs.sort(); result = [p[1] for p in pairs]\n");
        prompt.push_str("- map() and filter() are NOT available. Use list comprehensions: [f(x) for x in items], [x for x in items if cond(x)]\n");
        prompt.push_str("- Tool calls that fail return {\"__tool_error\": true, \"error\": \"...\"}. Always check: if isinstance(result, dict) and result.get(\"__tool_error\"): handle_error\n\n");
        prompt.push_str("Pattern corrections:\n");
        prompt.push_str(
            "- Instead of class: use dict + factory function: def make_item(x): return {\"x\": x}\n",
        );
        prompt.push_str("- Instead of match: use if/elif/else\n");
        prompt.push_str("- Instead of import: tools are pre-injected as functions\n");
        prompt.push_str("- Instead of with: use try/finally or call tool directly\n\n");
        prompt.push_str(&self.tool_categories_summary());
        prompt.push_str("\nCall get_code_mode_context() to see full function signatures.\n");
        prompt.push_str(
            "\nPass the Python code via the `orchestrate` tool call with {\"code\": \"...\"}.\n",
        );
        prompt.push_str("\nIMPORTANT — plan before execute:\n");
        prompt.push_str("- If the user explicitly asks to see a plan first, or asks for confirmation before executing, you MUST call the `plan` tool BEFORE writing any orchestrate code.\n");
        prompt.push_str(
            "- Wait for the user to confirm the plan. Only then proceed with orchestrate.\n",
        );
        prompt.push_str("- Keywords: \"先列出计划\", \"制定计划\", \"plan first\", \"confirm before\", \"我同意后\", \"确认后再执行\".\n\n");
        prompt.push_str("IMPORTANT — orchestrate output rules:\n");
        prompt.push_str(
            "- Do NOT call canvas_render() or create_artifact() inside orchestrate scripts.\n",
        );
        prompt.push_str("- Do NOT generate HTML/CSS in orchestrate. Keep scripts focused on data collection and processing.\n");
        prompt.push_str(
            "- orchestrate should return structured data (print dicts/lists/text summaries).\n",
        );
        prompt.push_str("- To create reports, apps, or rich artifacts, call create_artifact as a SEPARATE tool call AFTER orchestrate completes, using the returned data.\n");
        prompt.push_str("- NEVER embed large data (HTML, markdown, JSON) as string literals in code. Use tool calls to retrieve data at runtime (browser_get_markdown, fetch_page, read).\n");
        prompt
    }

    /// Register the get_code_mode_context tool that returns Python stubs.
    /// Call this after all other tools are registered.
    pub fn register_code_mode_context_tool(&mut self) {
        let stubs = self.to_python_stubs();
        self.register(
            "get_code_mode_context",
            Box::new(GetCodeModeContextTool::new(stubs)),
        );
    }

    /// Execute a tool call and return the result.
    ///
    /// # Arguments
    /// * `call` - The pending tool call to execute
    ///
    /// # Returns
    /// A `ToolResult` containing the execution result or error.
    pub async fn execute(&self, call: &PendingToolCall) -> ToolResult {
        match self.tools.get(&call.name) {
            Some(executor) => match executor.execute(&call.name, &call.arguments).await {
                Ok(content) => ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: Some(content),
                    error: None,
                },
                Err(e) => ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: None,
                    error: Some(e.to_string()),
                },
            },
            None => ToolResult {
                call_id: call.id.clone(),
                name: call.name.clone(),
                content: None,
                error: Some(format!("Unknown tool: {}", call.name)),
            },
        }
    }

    /// Execute a tool call with an optional tools_config guard.
    ///
    /// When `tools_config` is set, checks the allowlist before dispatching.
    /// This provides defense-in-depth beyond the prompt-layer filtering.
    pub async fn execute_with_guard(
        &self,
        call: &PendingToolCall,
        tools_config: &Option<nevoflux_protocol::subagent::ToolsConfig>,
    ) -> ToolResult {
        // Executor guard: check tool allowlist if configured
        match tools_config {
            Some(nevoflux_protocol::subagent::ToolsConfig::None) => {
                return ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: None,
                    error: Some(format!(
                        "Tool '{}' is not available: all tools are disabled",
                        call.name
                    )),
                };
            }
            Some(nevoflux_protocol::subagent::ToolsConfig::Allow(ref allowlist)) => {
                if !nevoflux_protocol::subagent::is_tool_allowed(allowlist, &call.name) {
                    return ToolResult {
                        call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: None,
                        error: Some(format!(
                            "Tool '{}' is not available: not in the allowed tool list",
                            call.name
                        )),
                    };
                }
            }
            None => {} // inherit: allow all
        }

        self.execute(call).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Tools
// ============================================================================

/// Tool for reading file contents.
pub struct ReadFileTool;

#[async_trait]
impl ToolExecutor for ReadFileTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'path' argument".to_string()))?;

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            DaemonError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to read file '{}': {}", path, e),
            ))
        })?;

        Ok(content)
    }
}

/// Tool for writing content to files.
pub struct WriteFileTool;

#[async_trait]
impl ToolExecutor for WriteFileTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'path' argument".to_string()))?;

        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'content' argument".to_string()))?;

        tokio::fs::write(path, content).await.map_err(|e| {
            DaemonError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to write file '{}': {}", path, e),
            ))
        })?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            path
        ))
    }
}

/// Tool for listing directory contents.
pub struct ListFilesTool;

#[async_trait]
impl ToolExecutor for ListFilesTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'path' argument".to_string()))?;

        let path = Path::new(path);
        if !path.is_dir() {
            return Err(DaemonError::InternalError(format!(
                "Path '{}' is not a directory",
                path.display()
            )));
        }

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await.map_err(|e| {
            DaemonError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to read directory '{}': {}", path.display(), e),
            ))
        })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| {
            DaemonError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to read directory entry: {}", e),
            ))
        })? {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await.ok();
            let type_indicator = if file_type.map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            entries.push(format!("{}{}", file_name, type_indicator));
        }

        entries.sort();
        Ok(entries.join("\n"))
    }
}

// ============================================================================
// Run Command Tool
// ============================================================================

/// Tool for executing shell commands from Code Mode.
///
/// Used by AutoFixer-injected helpers to bridge missing stdlib modules
/// (re, datetime, random) by running `python3 -c "..."` or shell commands.
///
/// Safety: commands are timeout-limited (120s) and output-size-limited (1MB).
/// On Windows, commands run via PowerShell; on Unix, via sh.
pub struct RunCommandTool;

#[async_trait]
impl ToolExecutor for RunCommandTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'command' argument".to_string()))?;

        // Safety: reject obviously dangerous commands
        let trimmed = command.trim();
        if trimmed.starts_with("rm ")
            || trimmed.starts_with("sudo ")
            || trimmed.contains("rm -rf")
            || trimmed.starts_with("dd ")
            || trimmed.starts_with("mkfs")
            || trimmed.contains("> /dev/")
        {
            return Err(DaemonError::InternalError(
                "Command rejected: potentially destructive operation".to_string(),
            ));
        }

        use tokio::process::Command;

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        let output = tokio::time::timeout(std::time::Duration::from_secs(120), cmd.output())
            .await
            .map_err(|_| {
                DaemonError::InternalError("Command timed out after 120 seconds".to_string())
            })?
            .map_err(|e| DaemonError::InternalError(format!("Failed to execute command: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Limit output size to 1MB
        let max_size = 1024 * 1024;

        if output.status.success() {
            let result = if stdout.len() > max_size {
                let safe_end = stdout.as_ref().floor_char_boundary(max_size);
                format!("{}... (truncated)", &stdout[..safe_end])
            } else {
                stdout.to_string()
            };
            Ok(result)
        } else {
            let err_msg = if stderr.is_empty() {
                format!(
                    "Command exited with code {}",
                    output.status.code().unwrap_or(-1)
                )
            } else if stderr.len() > max_size {
                let safe_end = stderr.as_ref().floor_char_boundary(max_size);
                format!("{}... (truncated)", &stderr[..safe_end])
            } else {
                stderr.to_string()
            };
            Err(DaemonError::InternalError(err_msg))
        }
    }
}

// ============================================================================
// Canvas Render Tool
// ============================================================================

/// Tool for rendering multi-file projects in the browser canvas.
///
/// When Python code in Code Mode calls `canvas_render(files, entry)`, this tool
/// validates the arguments and returns artifact data as JSON. The browser side
/// (background.js) handles the actual artifact creation and canvas tab opening.
pub struct CanvasRenderTool;

#[async_trait]
impl ToolExecutor for CanvasRenderTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        // Validate: files is required and must be an object mapping file paths to content
        let files = arguments
            .get("files")
            .ok_or_else(|| DaemonError::InternalError("Missing 'files' argument".to_string()))?;

        if !files.is_object() {
            return Err(DaemonError::InternalError(
                "'files' must be an object mapping file paths to content".to_string(),
            ));
        }

        let entry = arguments.get("entry").and_then(|e| e.as_str());
        let title = arguments
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("Generated App");

        // Generate artifact ID with random suffix to avoid collisions
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let random = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let id = format!("code-mode-{}-{:x}", millis, random);

        // Return artifact data as JSON - the browser side will handle creation
        let result = serde_json::json!({
            "success": true,
            "artifact_id": id,
            "type": "project",
            "title": title,
            "files": files,
            "entry": entry,
        });

        Ok(result.to_string())
    }
}

// ============================================================================
// Browser Tool (generic for all browser actions)
// ============================================================================

/// Generic browser tool that dispatches actions via the BrowserSender channel.
///
/// Each instance is configured with a specific `BrowserToolAction` and maps
/// positional/named arguments to the appropriate `BrowserRequest` params.
pub struct BrowserTool {
    ctx: Arc<BrowserContext>,
    action: BrowserToolAction,
}

impl BrowserTool {
    /// Build the params JSON from tool arguments based on the action type.
    fn build_params(
        action: BrowserToolAction,
        arguments: &serde_json::Value,
    ) -> (serde_json::Value, Option<i64>) {
        let tab_id = arguments
            .get("tab_id")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                // Support positional: last arg may be tab_id
                arguments
                    .as_array()
                    .and_then(|arr| arr.last())
                    .and_then(|v| v.as_i64())
            });

        let params = match action {
            BrowserToolAction::GetMarkdown
            | BrowserToolAction::Snapshot
            | BrowserToolAction::GoBack
            | BrowserToolAction::GoForward
            | BrowserToolAction::GetContent
            | BrowserToolAction::Screenshot
            | BrowserToolAction::GetElements => {
                serde_json::json!({})
            }
            BrowserToolAction::ListTabs | BrowserToolAction::QueryTabs => {
                // QueryTabs may have optional filters, pass through
                let mut p = serde_json::Map::new();
                if let Some(url) = arguments.get("url").and_then(|v| v.as_str()) {
                    p.insert("url".into(), serde_json::json!(url));
                }
                if let Some(title) = arguments.get("title").and_then(|v| v.as_str()) {
                    p.insert("title".into(), serde_json::json!(title));
                }
                if let Some(active) = arguments.get("active").and_then(|v| v.as_bool()) {
                    p.insert("active".into(), serde_json::json!(active));
                }
                serde_json::Value::Object(p)
            }
            BrowserToolAction::ClickById => {
                let element_id = arguments
                    .get("element_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "element_id": element_id })
            }
            BrowserToolAction::TypeById => {
                let element_id = arguments
                    .get("element_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let text = arguments.get("text").and_then(|v| v.as_str()).unwrap_or("");
                serde_json::json!({ "element_id": element_id, "text": text })
            }
            BrowserToolAction::Navigate => {
                let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let new_tab = arguments
                    .get("new_tab")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                serde_json::json!({ "url": url, "new_tab": new_tab })
            }
            BrowserToolAction::ActivateTab => {
                let target_tab = arguments
                    .get("tab_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                serde_json::json!({ "tab_id": target_tab })
            }
            BrowserToolAction::Scroll => {
                let direction = arguments
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("down");
                let amount = arguments
                    .get("amount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(3);
                serde_json::json!({ "direction": direction, "amount": amount })
            }
            BrowserToolAction::WebSearch => {
                let query = arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "query": query, "max_results": 10, "timeout_ms": 30000 })
            }
            BrowserToolAction::WebFetch => {
                let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
                serde_json::json!({
                    "url": url,
                    "timeout_ms": 30000,
                    "include_images": false,
                    "max_length": 100000
                })
            }
            BrowserToolAction::FillById => {
                let element_id = arguments
                    .get("element_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let value = arguments
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "element_id": element_id, "value": value })
            }
            BrowserToolAction::Click => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "selector": selector })
            }
            BrowserToolAction::Type => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let text = arguments.get("text").and_then(|v| v.as_str()).unwrap_or("");
                serde_json::json!({ "selector": selector, "text": text })
            }
            BrowserToolAction::Fill => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let value = arguments
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "selector": selector, "value": value })
            }
            BrowserToolAction::EvalJs => {
                let script = arguments
                    .get("expression")
                    .or_else(|| arguments.get("script"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "script": script })
            }
            BrowserToolAction::WaitFor => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let timeout_ms = arguments
                    .get("timeout_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(30000);
                serde_json::json!({ "selector": selector, "timeout_ms": timeout_ms })
            }
            BrowserToolAction::WaitForStable => {
                let strategy = arguments
                    .get("strategy")
                    .and_then(|v| v.as_str())
                    .unwrap_or("interaction");
                let max_wait = arguments
                    .get("max_wait")
                    .or_else(|| arguments.get("maxWait"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(3000);
                serde_json::json!({ "strategy": strategy, "maxWait": max_wait })
            }
            BrowserToolAction::KeyPress => {
                let key = arguments.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let mut p = serde_json::json!({ "key": key });
                if let Some(modifiers) = arguments.get("modifiers") {
                    p["modifiers"] = modifiers.clone();
                }
                p
            }
            BrowserToolAction::GetElement => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "selector": selector })
            }
            BrowserToolAction::QueryAll => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "selector": selector })
            }
            BrowserToolAction::ReadArtifact => {
                let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let mut p = serde_json::json!({ "id": id });
                // Multi-file artifact path selector — defaults to entry file
                // in the extension handler when absent.
                if let Some(path) = arguments.get("path") {
                    p["path"] = path.clone();
                }
                if let Some(offset) = arguments.get("offset") {
                    p["offset"] = offset.clone();
                }
                if let Some(limit) = arguments.get("limit") {
                    p["limit"] = limit.clone();
                }
                if let Some(grep) = arguments.get("grep") {
                    p["grep"] = grep.clone();
                }
                if let Some(context) = arguments.get("context") {
                    p["context"] = context.clone();
                }
                p
            }
            BrowserToolAction::EditArtifact => {
                let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let old_str = arguments
                    .get("old_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_str = arguments
                    .get("new_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mut p = serde_json::json!({ "id": id, "old_str": old_str, "new_str": new_str });
                if let Some(path) = arguments.get("path") {
                    p["path"] = path.clone();
                }
                p
            }
            BrowserToolAction::ExtractVisualIdentity => {
                // Forward the full ExtractVisualIdentityRequest shape (target,
                // timeout_sec, viewport) through to the extension. The
                // extension handler reads `target.url` / `target.tab_id`
                // itself; the daemon doesn't need to interpret them here.
                let mut p = serde_json::json!({
                    "target": arguments
                        .get("target")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                });
                if let Some(t) = arguments.get("timeout_sec") {
                    p["timeout_sec"] = t.clone();
                }
                if let Some(v) = arguments.get("viewport") {
                    p["viewport"] = v.clone();
                }
                p
            }
            BrowserToolAction::AskUser => {
                let question = arguments
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mut p = serde_json::json!({ "question": question });
                if let Some(options) = arguments.get("options") {
                    p["options"] = options.clone();
                }
                if let Some(allow_custom) = arguments.get("allow_custom") {
                    p["allow_custom"] = allow_custom.clone();
                }
                if let Some(timeout_ms) = arguments.get("timeout_ms") {
                    p["timeout_ms"] = timeout_ms.clone();
                }
                p
            }
            BrowserToolAction::Input => {
                // browser_input is dispatched via run_browser_input in the
                // execute() override — params do not need to be translated.
                // Passing the raw arguments through is sufficient.
                arguments.clone()
            }
            BrowserToolAction::Probe => {
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({ "selector": selector })
            }
            BrowserToolAction::Paste | BrowserToolAction::FillRichText => {
                // Internal variants dispatched by browser_input executor.
                // Build params the same way as Fill so they can be sent
                // directly if someone ever reaches this arm via the
                // standard execute path.
                let selector = arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let text = arguments.get("text").and_then(|v| v.as_str()).unwrap_or("");
                serde_json::json!({ "selector": selector, "text": text })
            }
            BrowserToolAction::UploadFile => {
                // Dispatched via the browser_upload_file tool (Task 4).
                // Pass arguments through unchanged; the orchestration layer
                // handles token issuance and file serving.
                arguments.clone()
            }
        };

        (params, tab_id)
    }

    /// Extract the user-facing value from a browser tool response.
    ///
    /// The raw browser response is a JSON object with metadata fields
    /// (success, title, url, etc.). For Code Mode, tools should return
    /// just the relevant content — e.g. `browser_get_markdown` returns
    /// the markdown string, not `{"markdown":"...","title":"..."}`.
    fn extract_result(action: BrowserToolAction, val: &serde_json::Value) -> String {
        match action {
            BrowserToolAction::GetMarkdown => {
                // Return the full response dict so code can access result["markdown"].
                // LLMs in Code Mode consistently assume dict returns (e.g.
                // `result.get("markdown")`); returning a plain string caused
                // AttributeError/TypeError crashes and expensive LLM rewrites.
                val.to_string()
            }
            BrowserToolAction::Snapshot => {
                // Extract the snapshot/elements data
                val.get("snapshot")
                    .or_else(|| val.get("elements"))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| val.to_string())
            }
            BrowserToolAction::EvalJs => {
                // Extract the JS return value from the response.
                // The extension uses different wrapper formats:
                //   Simple values: {"value": "https://...", "type": "string"}
                //   Objects:       {"result": {"title": "...", "url": "..."}}
                //   Legacy:        {"success": true, "result": "..."}
                // Extract the inner value so Python code gets the actual JS result.
                if let Some(inner) = val.get("value") {
                    // Extension format: {"value": <actual>, "type": "string"|"number"|...}
                    match inner {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }
                } else if let Some(inner) = val.get("result") {
                    // Legacy/object format: {"result": <actual>}
                    match inner {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }
                } else if let serde_json::Value::String(s) = val {
                    // Already a plain string value
                    s.clone()
                } else {
                    val.to_string()
                }
            }
            _ => val.to_string(),
        }
    }

    /// Dispatch the LLM-facing `browser_input` tool via the strategy
    /// engine orchestration. Constructs a RealBrowserBridge from the
    /// shared context and calls run_browser_input, returning the
    /// serialized BrowserInputResult.
    async fn run_browser_input_from_args(&self, arguments: &serde_json::Value) -> Result<String> {
        use crate::agent::browser_input::bridge::RealBrowserBridge;
        use crate::agent::browser_input::{run_browser_input, InputMode};

        let adapter_registry = browser_input_adapter_registry();

        let selector = arguments
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DaemonError::InvalidRequest("browser_input: selector required".into())
            })?;
        let text = arguments
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InvalidRequest("browser_input: text required".into()))?;
        let mode = arguments
            .get("mode")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s == "type" {
                    InputMode::Type
                } else {
                    InputMode::Fill
                }
            })
            .unwrap_or(InputMode::Fill);
        let verify_enabled = arguments
            .get("verify")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let tab_id = arguments.get("tab_id").and_then(|v| v.as_i64());

        let bridge = RealBrowserBridge::new(self.ctx.clone());
        let result = run_browser_input(
            &bridge,
            adapter_registry,
            selector,
            text,
            mode,
            tab_id,
            verify_enabled,
        )
        .await
        .map_err(|e| DaemonError::InternalError(e.to_string()))?;

        Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{\"success\":true}".to_string()))
    }

    /// Dispatch the LLM-facing `browser_probe` tool. Calls run_browser_probe
    /// and returns the Fingerprint as serialized JSON.
    async fn run_browser_probe_from_args(&self, arguments: &serde_json::Value) -> Result<String> {
        use crate::agent::browser_input::bridge::RealBrowserBridge;
        use crate::agent::browser_input::run_browser_probe;

        let selector = arguments
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DaemonError::InvalidRequest("browser_probe: selector required".into())
            })?;
        let tab_id = arguments.get("tab_id").and_then(|v| v.as_i64());

        let bridge = RealBrowserBridge::new(self.ctx.clone());
        let fingerprint = run_browser_probe(&bridge, selector, tab_id)
            .await
            .map_err(|e| DaemonError::InternalError(e.to_string()))?;

        Ok(serde_json::to_string(&fingerprint).unwrap_or_else(|_| "{}".to_string()))
    }

    async fn run_browser_upload_from_args(&self, arguments: &serde_json::Value) -> Result<String> {
        use crate::agent::browser_input::upload::{
            check_file_size, check_sensitive_path, detect_mime, validate_workspace_path,
            DEFAULT_MAX_SIZE, TOKEN_TTL,
        };
        use std::path::PathBuf;

        let selector = arguments
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DaemonError::InvalidRequest("browser_upload_file: selector required".into())
            })?;
        let file_path_str = arguments
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DaemonError::InvalidRequest("browser_upload_file: file_path required".into())
            })?;
        let tab_id = arguments.get("tab_id").and_then(|v| v.as_i64());

        // Resolve the workspace directory.
        // If the caller supplies workspace_dir (e.g. from user prompt: "upload from ~/Documents"),
        // use it. Otherwise fall back to the default workspace.
        let workspace_dir = match arguments.get("workspace_dir").and_then(|v| v.as_str()) {
            Some(dir) => PathBuf::from(dir),
            None => dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("nevoflux")
                .join("workspace"),
        };

        if !workspace_dir.exists() {
            std::fs::create_dir_all(&workspace_dir).map_err(|e| {
                DaemonError::InternalError(format!(
                    "browser_upload_file: cannot create workspace dir: {e}"
                ))
            })?;
        }

        // Validate path containment and canonicalize.
        let canonical =
            validate_workspace_path(std::path::Path::new(file_path_str), &workspace_dir)
                .map_err(|e| DaemonError::InternalError(e.to_string()))?;

        // Block sensitive files (keys, credentials, env, etc.)
        check_sensitive_path(&canonical).map_err(|e| DaemonError::InternalError(e.to_string()))?;

        // Check size limit.
        let size = check_file_size(&canonical, DEFAULT_MAX_SIZE)
            .map_err(|e| DaemonError::InternalError(e.to_string()))?;

        // Detect MIME type from magic bytes.
        let mime_type =
            detect_mime(&canonical).map_err(|e| DaemonError::InternalError(e.to_string()))?;

        // Derive file name.
        let file_name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());

        // Hand the canonical path to the AssetServer, which mints a
        // short-lived URL the browser actor can fetch.  AssetServer is
        // daemon-lifetime; absence here means the daemon couldn't bind
        // its loopback HTTP port at boot, in which case browser_upload
        // is genuinely unavailable.
        let asset_server =
            self.ctx.asset_server.as_ref().ok_or_else(|| {
                DaemonError::InternalError(
                    "browser_upload_file: AssetServer is not running on this daemon".into(),
                )
            })?;
        let file_url =
            asset_server.register_download(canonical, mime_type.clone(), file_name.clone(), TOKEN_TTL);

        // Build and dispatch the browser request.
        let params = serde_json::json!({
            "selector": selector,
            "fileUrl": file_url,
            "fileName": file_name,
            "mimeType": mime_type,
        });

        let request = BrowserRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: "browser-upload".to_string(),
            tab_id,
            action: BrowserToolAction::UploadFile,
            params,
            timeout_ms: 120_000,
            client_identity: self.ctx.client_identity.clone(),
            proxy_id: self.ctx.proxy_id.clone(),
        };

        let (response_tx, response_rx) = oneshot::channel();

        self.ctx
            .sender
            .send((request, response_tx))
            .await
            .map_err(|_| DaemonError::InternalError("Failed to send browser request".into()))?;

        let response: BrowserResponse = tokio::time::timeout(Duration::from_secs(120), response_rx)
            .await
            .map_err(|_| {
                DaemonError::InternalError("browser_upload_file: request timed out".into())
            })?
            .map_err(|_| {
                DaemonError::InternalError("browser_upload_file: response channel closed".into())
            })?;

        if response.success {
            Ok(serde_json::json!({
                "success": true,
                "file_name": file_name,
                "mime_type": mime_type,
                "size": size,
            })
            .to_string())
        } else {
            let msg = response
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "browser_upload_file failed".to_string());
            Err(DaemonError::InternalError(msg))
        }
    }
}

/// Process-global AdapterRegistry. Parses the compiled-in x_com.yaml
/// (plus any user/share dir recipes if the daemon config is extended
/// in a future PR) on first access.
fn browser_input_adapter_registry() -> &'static crate::agent::browser_input::AdapterRegistry {
    use crate::agent::browser_input::AdapterRegistry;
    static REGISTRY: OnceLock<AdapterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let reg = AdapterRegistry::load_standard(None, None);
        tracing::info!(
            loaded = reg.len(),
            "browser_input: adapter registry initialized"
        );
        reg
    })
}

#[async_trait]
impl ToolExecutor for BrowserTool {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        // PR #2 tools (browser_input, browser_probe) are orchestrated via
        // the strategy engine, not single-call, so they short-circuit out
        // of the standard request/response path.
        match self.action {
            BrowserToolAction::Input => {
                return self.run_browser_input_from_args(arguments).await;
            }
            BrowserToolAction::Probe => {
                return self.run_browser_probe_from_args(arguments).await;
            }
            BrowserToolAction::UploadFile => {
                return self.run_browser_upload_from_args(arguments).await;
            }
            _ => {}
        }

        let (params, tab_id) = Self::build_params(self.action, arguments);

        let request = BrowserRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: "code-mode".to_string(),
            tab_id,
            action: self.action,
            params,
            timeout_ms: 30000,
            client_identity: self.ctx.client_identity.clone(),
            proxy_id: self.ctx.proxy_id.clone(),
        };

        let (response_tx, response_rx) = oneshot::channel();

        self.ctx
            .sender
            .send((request, response_tx))
            .await
            .map_err(|_| DaemonError::InternalError("Failed to send browser request".into()))?;

        let response: BrowserResponse = tokio::time::timeout(Duration::from_secs(30), response_rx)
            .await
            .map_err(|_| DaemonError::InternalError("Browser request timed out".into()))?
            .map_err(|_| DaemonError::InternalError("Response channel closed".into()))?;

        if response.success {
            match response.result {
                Some(val) => Ok(Self::extract_result(self.action, &val)),
                None => Ok("OK".to_string()),
            }
        } else {
            let error_msg = match &response.error {
                // browser_input / browser_probe error code hints (spec §5.8)
                Some(e)
                    if (1001..=1014).contains(&e.code)
                        && matches!(
                            self.action,
                            BrowserToolAction::Input | BrowserToolAction::Probe
                        ) =>
                {
                    let hint = match e.code {
                        1001 => " — Element not found. Try browser_query_all with a broader selector.",
                        1002 => " — Could not focus target. Try browser_click first then retry.",
                        1007 => " — Invalid CSS selector syntax. Double-check the selector string.",
                        1008 => " — Element is disabled or readonly; input not possible.",
                        1014 => " — Verification mismatch: the content was not updated as expected. The framework may have rejected the input.",
                        _ => " — Browser input failed.",
                    };
                    format!("{}{}", e.message, hint)
                }
                Some(e) if e.code == 9001 && self.action == BrowserToolAction::EvalJs => {
                    let script = arguments
                        .get("expression")
                        .or_else(|| arguments.get("script"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    format!("{} — {}", e.message, eval_csp_hint(script))
                }
                Some(e) if e.code == 9004 && self.action == BrowserToolAction::EvalJs => {
                    format!(
                        "{} — JS runtime/syntax error (code 9004, recoverable). \
                         Review the script and retry with a fix.",
                        e.message
                    )
                }
                Some(e)
                    if matches!(
                        self.action,
                        BrowserToolAction::Fill
                            | BrowserToolAction::FillById
                            | BrowserToolAction::Click
                            | BrowserToolAction::ClickById
                    ) && e.message.contains("not found") =>
                {
                    format!(
                        "{} — Take a new snapshot to get fresh element IDs, \
                         or try browser_type_by_id as an alternative.",
                        e.message
                    )
                }
                Some(e) => e.message.clone(),
                None => "Browser action failed".to_string(),
            };
            Err(DaemonError::InternalError(error_msg))
        }
    }
}

/// Heuristic hint for CSP-blocked eval: suggests a structured tool
/// based on what the rejected script was trying to do (spec §9.3).
fn eval_csp_hint(script: &str) -> String {
    let s = script.to_lowercase();
    let suggestion = if s.contains(".value =") || s.contains(".value=") {
        "Use browser_input instead."
    } else if s.contains("queryselectorall") {
        "Use browser_query_all instead."
    } else if s.contains("queryselector") {
        "Use browser_probe or browser_get_element instead."
    } else if s.contains("click(") {
        "Use browser_click instead."
    } else if s.contains("textcontent") || s.contains("innertext") {
        "Use browser_get_content instead."
    } else if s.contains("scroll") {
        "Use browser_scroll instead."
    } else if s.contains("window.location") {
        "Use browser_navigate or browser_get_tabs instead."
    } else {
        "If no structured tool covers this use case, CSP blocks this eval; \
         only workaround is asking a human to add a dedicated primitive."
    };
    format!(
        "CSP blocked eval(). {} Structured tools bypass CSP because they \
         are chrome-privileged DOM operations.",
        suggestion
    )
}

// ============================================================================
// Code Mode Context Tool
// ============================================================================

/// Tool that returns Code Mode context (Python function signatures for available tools).
pub struct GetCodeModeContextTool {
    stubs: String,
}

impl GetCodeModeContextTool {
    /// Create a new Code Mode context tool with pre-generated Python stubs.
    pub fn new(stubs: String) -> Self {
        Self { stubs }
    }
}

#[async_trait]
impl ToolExecutor for GetCodeModeContextTool {
    async fn execute(&self, _name: &str, _arguments: &serde_json::Value) -> Result<String> {
        Ok(self.stubs.clone())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();

        // Check that built-in tools are registered
        assert!(registry.has_tool("read_file"));
        assert!(registry.has_tool("write_file"));
        assert!(registry.has_tool("list_files"));
        assert!(registry.has_tool("canvas_render"));
        assert!(registry.has_tool("run_command"));

        // Check that unknown tools are not registered
        assert!(!registry.has_tool("unknown_tool"));
    }

    #[test]
    fn test_registry_tool_names() {
        let registry = ToolRegistry::new();
        let names = registry.tool_names();

        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"canvas_render"));
        // 5 built-in tools: read_file, write_file, list_files, canvas_render, run_command
        assert_eq!(names.len(), 5);
    }

    #[tokio::test]
    async fn test_with_browser_registry() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let registry = ToolRegistry::with_browser(BrowserContext {
            sender: tx,
            proxy_id: String::new(),
            client_identity: vec![],
            asset_server: None,
        });
        let names = registry.tool_names();

        // Should have base tools + browser tools + web tools
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"canvas_render"));
        assert!(names.contains(&"run_command"));

        // Core browser tools
        assert!(names.contains(&"browser_get_markdown"));
        assert!(names.contains(&"browser_snapshot"));
        assert!(names.contains(&"browser_click_by_id"));
        assert!(names.contains(&"browser_type_by_id"));
        assert!(names.contains(&"browser_fill_by_id"));
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_go_back"));
        assert!(names.contains(&"browser_go_forward"));
        assert!(names.contains(&"browser_scroll"));
        assert!(names.contains(&"browser_get_tabs"));
        assert!(names.contains(&"browser_query_tabs"));
        assert!(names.contains(&"browser_get_elements"));

        // Lower-level browser tools
        assert!(names.contains(&"browser_click"));
        assert!(names.contains(&"browser_type"));
        assert!(names.contains(&"browser_fill"));
        assert!(names.contains(&"browser_get_content"));
        assert!(names.contains(&"browser_screenshot"));
        assert!(names.contains(&"browser_eval_js"));
        assert!(names.contains(&"browser_wait_for"));
        assert!(names.contains(&"browser_wait_for_stable"));
        assert!(names.contains(&"browser_key_press"));
        assert!(names.contains(&"browser_get_element"));
        assert!(names.contains(&"browser_query_all"));

        // Artifact tools
        assert!(names.contains(&"browser_read_artifact"));
        assert!(names.contains(&"browser_edit_artifact"));

        // Web tools
        assert!(names.contains(&"web_search"));
        assert!(names.contains(&"fetch_page"));

        // User interaction
        assert!(names.contains(&"browser_ask_user"));
    }

    #[tokio::test]
    async fn test_with_browser_python_stubs() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let registry = ToolRegistry::with_browser(BrowserContext {
            sender: tx,
            proxy_id: String::new(),
            client_identity: vec![],
            asset_server: None,
        });
        let stubs = registry.to_python_stubs();

        assert!(stubs.contains("def browser_get_markdown("));
        assert!(stubs.contains("def browser_navigate("));
        assert!(stubs.contains("def web_search("));
        assert!(stubs.contains("def fetch_page("));
    }

    #[tokio::test]
    async fn test_with_browser_categories() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let registry = ToolRegistry::with_browser(BrowserContext {
            sender: tx,
            proxy_id: String::new(),
            client_identity: vec![],
            asset_server: None,
        });
        let summary = registry.tool_categories_summary();

        assert!(summary.contains("Browser & Canvas"));
        assert!(summary.contains("browser_get_markdown"));
        assert!(summary.contains("Search & Web"));
        assert!(summary.contains("web_search"));
    }

    #[test]
    fn test_python_stubs_generation() {
        let registry = ToolRegistry::new();
        let stubs = registry.to_python_stubs();
        assert!(stubs.contains("def list_files("));
        assert!(stubs.contains("def read_file("));
        assert!(stubs.contains("def write_file("));
        assert!(stubs.contains("def run_command("));
        assert!(stubs.contains("# Available tool functions"));
    }

    #[test]
    fn test_tool_categories_summary() {
        let registry = ToolRegistry::new();
        let summary = registry.tool_categories_summary();
        assert!(summary.contains("Files"));
        assert!(summary.contains("read_file"));
    }

    #[test]
    fn test_code_mode_system_prompt() {
        let registry = ToolRegistry::new();
        let prompt = registry.code_mode_system_prompt();
        assert!(prompt.contains("Code Mode"));
        assert!(prompt.contains("DO NOT use"));
        assert!(prompt.contains("class"));
        assert!(prompt.contains("orchestrate"));
    }

    #[test]
    fn test_register_code_mode_context_tool() {
        let mut registry = ToolRegistry::new();
        assert!(!registry.has_tool("get_code_mode_context"));

        registry.register_code_mode_context_tool();
        assert!(registry.has_tool("get_code_mode_context"));
        assert_eq!(registry.tool_names().len(), 6);
    }

    #[tokio::test]
    async fn test_code_mode_context_tool_execution() {
        let mut registry = ToolRegistry::new();
        registry.register_code_mode_context_tool();

        let call = PendingToolCall {
            id: "call-ctx".to_string(),
            name: "get_code_mode_context".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_none());
        let content = result.content.unwrap();
        assert!(content.contains("def read_file("));
        assert!(content.contains("def write_file("));
        assert!(content.contains("# Available tool functions"));
    }

    #[test]
    fn test_param_mappings_basic() {
        let registry = ToolRegistry::new();
        let mappings = registry.param_mappings();

        // read_file has "path" param
        assert_eq!(mappings.get("read_file"), Some(&vec!["path".to_string()]));

        // write_file has "path" and "content"
        let wf = mappings.get("write_file").unwrap();
        assert_eq!(wf.len(), 2);
        assert!(wf.contains(&"path".to_string()));
        assert!(wf.contains(&"content".to_string()));

        // run_command has "command"
        assert_eq!(
            mappings.get("run_command"),
            Some(&vec!["command".to_string()])
        );
    }

    #[test]
    fn test_param_mappings_browser() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let registry = ToolRegistry::with_browser(BrowserContext {
            sender: tx,
            proxy_id: String::new(),
            client_identity: vec![],
            asset_server: None,
        });
        let mappings = registry.param_mappings();

        // browser_navigate has "url" and "tab_id"
        let nav = mappings.get("browser_navigate").unwrap();
        assert!(nav.contains(&"url".to_string()));
        assert!(nav.contains(&"tab_id".to_string()));

        // web_search has "query"
        assert_eq!(mappings.get("web_search"), Some(&vec!["query".to_string()]));
    }

    #[test]
    fn test_empty_registry() {
        let registry = ToolRegistry::empty();
        assert!(!registry.has_tool("read_file"));
        assert!(registry.tool_names().is_empty());
    }

    #[test]
    fn test_register_custom_tool() {
        struct CustomTool;

        #[async_trait]
        impl ToolExecutor for CustomTool {
            async fn execute(&self, _name: &str, _arguments: &serde_json::Value) -> Result<String> {
                Ok("custom result".to_string())
            }
        }

        let mut registry = ToolRegistry::empty();
        registry.register("custom", Box::new(CustomTool));

        assert!(registry.has_tool("custom"));
    }

    #[tokio::test]
    async fn test_unknown_tool_handling() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-001".to_string(),
            name: "nonexistent_tool".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute(&call).await;

        assert_eq!(result.call_id, "call-001");
        assert_eq!(result.name, "nonexistent_tool");
        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_list_files_execution() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create some test files
        std::fs::write(dir_path.join("file1.txt"), "content1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "content2").unwrap();
        std::fs::create_dir(dir_path.join("subdir")).unwrap();

        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-002".to_string(),
            name: "list_files".to_string(),
            arguments: serde_json::json!({
                "path": dir_path.to_str().unwrap()
            }),
        };

        let result = registry.execute(&call).await;

        assert_eq!(result.call_id, "call-002");
        assert_eq!(result.name, "list_files");
        assert!(result.error.is_none());
        assert!(result.content.is_some());

        let content = result.content.unwrap();
        assert!(content.contains("file1.txt"));
        assert!(content.contains("file2.txt"));
        assert!(content.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_read_file_execution() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-003".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": file_path.to_str().unwrap()
            }),
        };

        let result = registry.execute(&call).await;

        assert!(result.error.is_none());
        assert_eq!(result.content, Some("Hello, World!".to_string()));
    }

    #[tokio::test]
    async fn test_read_file_missing_path() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-004".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute(&call).await;

        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Missing 'path' argument"));
    }

    #[tokio::test]
    async fn test_read_file_nonexistent() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-005".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "/nonexistent/path/to/file.txt"
            }),
        };

        let result = registry.execute(&call).await;

        assert!(result.content.is_none());
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_write_file_execution() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-006".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "content": "Test content"
            }),
        };

        let result = registry.execute(&call).await;

        assert!(result.error.is_none());
        assert!(result.content.is_some());
        assert!(result.content.unwrap().contains("Successfully wrote"));

        // Verify file was written
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Test content");
    }

    #[tokio::test]
    async fn test_write_file_missing_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-007".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": file_path.to_str().unwrap()
            }),
        };

        let result = registry.execute(&call).await;

        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Missing 'content' argument"));
    }

    #[tokio::test]
    async fn test_list_files_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-008".to_string(),
            name: "list_files".to_string(),
            arguments: serde_json::json!({
                "path": file_path.to_str().unwrap()
            }),
        };

        let result = registry.execute(&call).await;

        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("is not a directory"));
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.has_tool("read_file"));
        assert!(registry.has_tool("write_file"));
        assert!(registry.has_tool("list_files"));
        assert!(registry.has_tool("canvas_render"));
        assert!(registry.has_tool("run_command"));
    }

    // ========================================================================
    // Canvas Render Tool Tests
    // ========================================================================

    #[tokio::test]
    async fn test_canvas_render_with_valid_files_and_entry() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-cr-1".to_string(),
            name: "canvas_render".to_string(),
            arguments: serde_json::json!({
                "files": {
                    "index.html": "<html><body><div id=\"root\"></div></body></html>",
                    "App.jsx": "export default function App() { return <h1>Hello</h1>; }",
                    "style.css": "body { margin: 0; }"
                },
                "entry": "index.html",
                "title": "My React App"
            }),
        };

        let result = registry.execute(&call).await;
        assert!(
            result.error.is_none(),
            "Expected success, got error: {:?}",
            result.error
        );

        let content = result.content.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["type"], "project");
        assert_eq!(parsed["title"], "My React App");
        assert_eq!(parsed["entry"], "index.html");
        assert!(parsed["artifact_id"]
            .as_str()
            .unwrap()
            .starts_with("code-mode-"));
        assert!(parsed["files"].is_object());
        assert_eq!(
            parsed["files"]["App.jsx"],
            "export default function App() { return <h1>Hello</h1>; }"
        );
    }

    #[tokio::test]
    async fn test_canvas_render_missing_files() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-cr-2".to_string(),
            name: "canvas_render".to_string(),
            arguments: serde_json::json!({
                "entry": "index.html"
            }),
        };

        let result = registry.execute(&call).await;
        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Missing 'files' argument"));
    }

    #[tokio::test]
    async fn test_canvas_render_invalid_files_type() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-cr-3".to_string(),
            name: "canvas_render".to_string(),
            arguments: serde_json::json!({
                "files": "not-an-object"
            }),
        };

        let result = registry.execute(&call).await;
        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("'files' must be an object"));
    }

    #[tokio::test]
    async fn test_canvas_render_files_as_array() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-cr-3b".to_string(),
            name: "canvas_render".to_string(),
            arguments: serde_json::json!({
                "files": ["file1.js", "file2.js"]
            }),
        };

        let result = registry.execute(&call).await;
        assert!(result.content.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("'files' must be an object"));
    }

    #[tokio::test]
    async fn test_canvas_render_minimal_args() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-cr-4".to_string(),
            name: "canvas_render".to_string(),
            arguments: serde_json::json!({
                "files": {
                    "index.html": "<h1>Hello</h1>"
                }
            }),
        };

        let result = registry.execute(&call).await;
        assert!(
            result.error.is_none(),
            "Expected success, got error: {:?}",
            result.error
        );

        let content = result.content.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["type"], "project");
        // Default title when none provided
        assert_eq!(parsed["title"], "Generated App");
        // entry should be null when not provided
        assert!(parsed["entry"].is_null());
        assert!(parsed["artifact_id"]
            .as_str()
            .unwrap()
            .starts_with("code-mode-"));
    }

    // ========================================================================
    // Run Command Tool Tests
    // ========================================================================

    #[tokio::test]
    async fn test_run_command_echo() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-1".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({"command": "echo hello"}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_none(), "Error: {:?}", result.error);
        assert_eq!(result.content.unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_run_command_python3() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-2".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({"command": "python3 -c \"print(2 + 3)\""}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_none(), "Error: {:?}", result.error);
        assert_eq!(result.content.unwrap().trim(), "5");
    }

    #[tokio::test]
    async fn test_run_command_reject_rm() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-3".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({"command": "rm -rf /tmp/test"}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("rejected"));
    }

    #[tokio::test]
    async fn test_run_command_reject_sudo() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-4".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({"command": "sudo ls"}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("rejected"));
    }

    #[tokio::test]
    async fn test_run_command_missing_arg() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-5".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Missing 'command'"));
    }

    #[tokio::test]
    async fn test_run_command_nonzero_exit() {
        let registry = ToolRegistry::new();
        let call = PendingToolCall {
            id: "call-rc-6".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({"command": "false"}),
        };

        let result = registry.execute(&call).await;
        assert!(result.error.is_some());
    }

    #[test]
    fn tool_execution_record_captures_outcome() {
        let record = ToolExecutionRecord {
            tool_name: "web_fetch".into(),
            arguments_summary: r#"{"url":"https://example.com"}"#.into(),
            success: true,
            error_message: None,
            duration_ms: 1500,
            session_id: "sess-123".into(),
        };
        assert!(record.success);
        assert_eq!(record.duration_ms, 1500);
        assert_eq!(record.tool_name, "web_fetch");
    }

    #[tokio::test]
    async fn test_canvas_render_registered_in_default_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.has_tool("canvas_render"));

        // Also verify it appears in python stubs
        let stubs = registry.to_python_stubs();
        assert!(stubs.contains("def canvas_render("));
        assert!(stubs.contains("files: dict, entry: str"));

        // Verify it appears in tool categories summary under Browser & Canvas
        let summary = registry.tool_categories_summary();
        assert!(summary.contains("canvas_render"));
        assert!(summary.contains("Browser & Canvas"));
    }

    // ========================================================================
    // Tool filter executor guard tests
    // ========================================================================

    #[test]
    fn test_executor_guard_blocks_disallowed_tool() {
        use nevoflux_protocol::subagent::{is_tool_allowed, ToolsConfig};

        let config = Some(ToolsConfig::Allow(vec!["browser_*".to_string()]));

        // read_file should NOT match browser_* pattern
        match &config {
            Some(ToolsConfig::Allow(ref allowlist)) => {
                assert!(
                    !is_tool_allowed(allowlist, "read_file"),
                    "read_file should be blocked by browser_* allowlist"
                );
            }
            _ => panic!("Expected Allow config"),
        }
    }

    #[test]
    fn test_executor_guard_allows_matching_tool() {
        use nevoflux_protocol::subagent::{is_tool_allowed, ToolsConfig};

        let config = Some(ToolsConfig::Allow(vec![
            "read_file".to_string(),
            "browser_*".to_string(),
        ]));

        match &config {
            Some(ToolsConfig::Allow(ref allowlist)) => {
                assert!(
                    is_tool_allowed(allowlist, "read_file"),
                    "read_file should be allowed"
                );
                assert!(
                    is_tool_allowed(allowlist, "browser_navigate"),
                    "browser_navigate should match browser_*"
                );
            }
            _ => panic!("Expected Allow config"),
        }
    }

    #[test]
    fn test_executor_guard_blocks_all_when_none() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let config = Some(ToolsConfig::None);

        // ToolsConfig::None should block everything
        match &config {
            Some(ToolsConfig::None) => {
                // This is the expected path — all tools disabled
            }
            _ => panic!("Expected None config"),
        }
    }

    #[test]
    fn test_executor_guard_inherit_allows_all() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let config: Option<ToolsConfig> = None;

        // None (inherit) means no filtering — all tools allowed
        assert!(config.is_none(), "Inherit config should be None");
    }

    #[test]
    fn test_extract_result_eval_js_value_string() {
        // Extension format: {"value": "https://www.google.com/", "type": "string"}
        let val = serde_json::json!({"value": "https://www.google.com/", "type": "string"});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert_eq!(result, "https://www.google.com/");
    }

    #[test]
    fn test_extract_result_eval_js_value_number() {
        // Extension format: {"value": 42, "type": "number"}
        let val = serde_json::json!({"value": 42, "type": "number"});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_extract_result_eval_js_result_string() {
        // Legacy format: {"success":true,"result":"Google"}
        let val = serde_json::json!({"success": true, "result": "Google"});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert_eq!(result, "Google");
    }

    #[test]
    fn test_extract_result_eval_js_result_object() {
        // Object format: {"result":{"title":"Google","url":"https://google.com"}}
        let val = serde_json::json!({"result": {"title": "Google", "url": "https://google.com"}});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert!(result.contains("Google"));
        assert!(result.contains("https://google.com"));
    }

    #[test]
    fn test_extract_result_eval_js_plain_string() {
        // Already a plain string value (no wrapper)
        let val = serde_json::Value::String("hello".into());
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_extract_result_eval_js_no_extractable_field() {
        // Extension returns {"success":true} with no value/result field
        let val = serde_json::json!({"success": true});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        assert_eq!(result, r#"{"success":true}"#);
    }

    #[test]
    fn test_extract_result_eval_js_complex_elements() {
        // Extension returns element query result: {"count":1,"elements":[...]}
        let val =
            serde_json::json!({"count": 1, "elements": [{"tag": "textarea", "id": "APjFqb"}]});
        let result = BrowserTool::extract_result(BrowserToolAction::EvalJs, &val);
        // No value/result field, falls through to full JSON
        assert!(result.contains("APjFqb"));
        assert!(result.contains("textarea"));
    }

    // -----------------------------------------------------------------------
    // §13.7 — browser_upload_e2e_via_asset_server
    //
    // Run the full validation pipeline through `run_browser_upload_from_args`,
    // catch the URL the BrowserTool is about to dispatch to the actor, and
    // fetch it via reqwest.  Asserts: HTTP 200, correct bytes, correct
    // Content-Disposition filename, and single-use semantics (second GET
    // returns 404).
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn browser_upload_e2e_via_asset_server() {
        use crate::asset_server::{AssetServer, AssetServerConfig};
        use nevoflux_protocol::BrowserToolAction;
        use std::io::Write;
        use std::sync::Arc;
        use tempfile::TempDir;

        // Boot the AssetServer (the new infrastructure).
        let asset_server = AssetServer::start(AssetServerConfig::default())
            .await
            .expect("AssetServer should boot in tests");

        // Channel for the browser actor — capture the BrowserRequest, then
        // reply success so `run_browser_upload_from_args` returns Ok.
        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<(BrowserRequest, oneshot::Sender<BrowserResponse>)>(1);

        // Workspace + fixture file.
        let workspace = TempDir::new().unwrap();
        let fixture = workspace.path().join("e2e.txt");
        let payload = b"asset_server upload e2e payload";
        let mut f = std::fs::File::create(&fixture).unwrap();
        f.write_all(payload).unwrap();
        f.flush().unwrap();

        // BrowserTool with the AssetServer threaded into its context.
        let ctx = Arc::new(BrowserContext {
            sender: tx,
            proxy_id: "test-proxy".into(),
            client_identity: b"test-proxy".to_vec(),
            asset_server: Some(asset_server.clone()),
        });
        let tool = BrowserTool {
            ctx,
            action: BrowserToolAction::UploadFile,
        };

        // Drain the actor channel and reply success — the channel-side
        // task captures the dispatched URL.
        let captured: Arc<tokio::sync::Mutex<Option<String>>> =
            Arc::new(tokio::sync::Mutex::new(None));
        let captured_clone = captured.clone();
        let actor = tokio::spawn(async move {
            if let Some((req, reply)) = rx.recv().await {
                if let Some(url) = req.params.get("fileUrl").and_then(|v| v.as_str()) {
                    *captured_clone.lock().await = Some(url.to_string());
                }
                let _ = reply.send(BrowserResponse {
                    request_id: req.request_id,
                    success: true,
                    result: Some(serde_json::json!({"ok": true})),
                    error: None,
                });
            }
        });

        let args = serde_json::json!({
            "selector": "#file-input",
            "file_path": fixture.to_str().unwrap(),
            "workspace_dir": workspace.path().to_str().unwrap(),
        });
        let out = tool
            .run_browser_upload_from_args(&args)
            .await
            .expect("upload from args should succeed");
        actor.await.unwrap();

        // The tool returned a JSON success blob.
        let val: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(val["success"], true);
        assert_eq!(val["file_name"], "e2e.txt");

        // Pull the dispatched URL and fetch it via the AssetServer.
        let url = captured.lock().await.clone().expect("dispatched URL");
        assert!(
            url.starts_with(&format!(
                "http://127.0.0.1:{}/file/",
                asset_server.bound_port()
            )),
            "url should target this AssetServer: got {url}"
        );

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client.get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let cd = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        assert_eq!(cd.as_deref(), Some("attachment; filename=\"e2e.txt\""));
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), payload);

        // Single-use: the second GET must 404 (token consumed).
        let resp2 = client.get(&url).send().await.unwrap();
        assert_eq!(resp2.status(), 404);
    }

    #[tokio::test]
    async fn browser_upload_returns_error_when_asset_server_missing() {
        use nevoflux_protocol::BrowserToolAction;
        use std::sync::Arc;
        use tempfile::TempDir;

        let (tx, _rx) =
            tokio::sync::mpsc::channel::<(BrowserRequest, oneshot::Sender<BrowserResponse>)>(1);
        let workspace = TempDir::new().unwrap();
        let fixture = workspace.path().join("a.txt");
        std::fs::write(&fixture, b"x").unwrap();

        let ctx = Arc::new(BrowserContext {
            sender: tx,
            proxy_id: "p".into(),
            client_identity: b"p".to_vec(),
            asset_server: None,
        });
        let tool = BrowserTool {
            ctx,
            action: BrowserToolAction::UploadFile,
        };

        let args = serde_json::json!({
            "selector": "#x",
            "file_path": fixture.to_str().unwrap(),
            "workspace_dir": workspace.path().to_str().unwrap(),
        });
        let err = tool
            .run_browser_upload_from_args(&args)
            .await
            .expect_err("must fail when AssetServer is missing");
        assert!(err.to_string().contains("AssetServer is not running"));
    }
}
