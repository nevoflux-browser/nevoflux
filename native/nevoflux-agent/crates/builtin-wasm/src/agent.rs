//! Agent loop implementation.
//!
//! This module contains the core agent logic that:
//! - Constructs prompts based on mode
//! - Calls the LLM
//! - Executes tool calls
//! - Manages the conversation loop

use crate::host::{HostFunctions, HostResult};
use crate::types::*;
use nevoflux_protocol::{Artifact, LocalFileRef, PlanProposal, PlanStep};
use std::cell::{Cell, RefCell};

/// Format local file references for injection into user message.
fn format_local_files(files: &[LocalFileRef]) -> String {
    if files.is_empty() {
        return String::new();
    }

    let mut result = String::from("用户附加了以下本地文件/目录：\n");

    for file in files {
        let type_str = if file.is_directory {
            "目录"
        } else {
            "文件"
        };
        let size_str = file.size.map(format_file_size).unwrap_or_default();

        if file.is_directory {
            result.push_str(&format!("- {} ({})\n", file.path, type_str));
        } else {
            result.push_str(&format!("- {} ({}, {})\n", file.path, type_str, size_str));
        }
    }

    result.push_str("\n如需查看内容，请使用 read_file 或 list_directory 工具。\n\n");
    result
}

/// Format file size in human-readable form.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a `ReadResult` into model-readable text with line numbers and metadata.
fn format_read_result(result: &ReadResult, path: &str) -> String {
    let mut output = String::new();
    let end_line = result.offset + result.returned_lines;
    let size_str = if result.total_bytes >= 1024 * 1024 {
        format!("{:.1}MB", result.total_bytes as f64 / (1024.0 * 1024.0))
    } else if result.total_bytes >= 1024 {
        format!("{:.0}KB", result.total_bytes as f64 / 1024.0)
    } else {
        format!("{}B", result.total_bytes)
    };

    output.push_str(&format!(
        "[File: {} | Lines: {}-{} of {} | {}]\n",
        path,
        result.offset + 1,
        end_line,
        result.total_lines,
        size_str
    ));

    for (i, line) in result.content.lines().enumerate() {
        let line_num = result.offset + i as u64 + 1;
        output.push_str(&format!("{:>4}|{}\n", line_num, line));
    }

    if result.truncated {
        let remaining = result.total_lines - end_line;
        output.push_str(&format!(
            "[Truncated: {} lines remaining. Use offset={} to continue.]",
            remaining, end_line
        ));
    }

    output
}

/// Format a `GrepResult` into model-readable text with match locations.
fn format_grep_result(result: &GrepResult, pattern: &str) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "[Search: \"{}\" | {} matches in {} files | showing {}]\n",
        pattern, result.total_matches, result.total_files, result.returned
    ));

    for m in &result.results {
        output.push_str(&format!("{}:{}: {}\n", m.file, m.line, m.content));
    }

    if result.truncated {
        let remaining = result.total_matches - result.returned;
        output.push_str(&format!(
            "[Truncated: {} more matches. Narrow your pattern or use max_results.]",
            remaining
        ));
    }

    output
}

/// Format a `BashResult` into model-readable text with status and output.
fn format_bash_result(result: &BashResult) -> String {
    let mut output = String::new();

    let status_str = match result.status {
        BashStatus::Success => format!("exit={}", result.exit_code.unwrap_or(0)),
        BashStatus::Error => format!("exit={} | error", result.exit_code.unwrap_or(-1)),
        BashStatus::Timeout => "timeout".into(),
        BashStatus::Killed => "killed".into(),
    };

    output.push_str(&format!(
        "[Bash: {} | {} lines]\n",
        status_str, result.returned_lines
    ));

    if !result.stdout.is_empty() {
        if result.stderr.is_some() {
            output.push_str("STDOUT:\n");
        }
        output.push_str(&result.stdout);
        if !result.stdout.ends_with('\n') {
            output.push('\n');
        }
    }

    if let Some(stderr) = &result.stderr {
        if !stderr.is_empty() {
            output.push_str("STDERR:\n");
            output.push_str(stderr);
            if !stderr.ends_with('\n') {
                output.push('\n');
            }
        }
    }

    if result.truncated {
        output.push_str(&format!(
            "[Truncated: showing {} of {} lines]",
            result.returned_lines, result.total_lines
        ));
    }

    if let Some(hint) = &result.hint {
        output.push_str(&format!("[Hint: {}]", hint));
    }

    output
}

/// Maximum iterations in the agent loop to prevent infinite loops.
const MAX_ITERATIONS: usize = 100;

/// Agent configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum iterations before stopping.
    pub max_iterations: usize,
    /// Whether to use streaming.
    pub use_streaming: bool,
    /// Suppress streaming output (for sub-agents that only return final result).
    ///
    /// When true, intermediate results are not sent to the host.
    /// This is useful for sub-agents where only the final result matters.
    pub suppress_streaming: bool,
    /// Whether this agent is running as a sub-agent (restricts to read-only browser tools).
    pub is_subagent: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: MAX_ITERATIONS,
            use_streaming: true,
            suppress_streaming: false,
            is_subagent: false,
        }
    }
}

impl AgentConfig {
    /// Create a new config for a sub-agent with streaming suppressed.
    pub fn for_subagent() -> Self {
        Self {
            max_iterations: MAX_ITERATIONS,
            use_streaming: false,
            suppress_streaming: true,
            is_subagent: true,
        }
    }

    /// Set whether to suppress streaming output.
    pub fn with_suppress_streaming(mut self, suppress: bool) -> Self {
        self.suppress_streaming = suppress;
        self
    }
}

/// Bounding rectangle for a cached element.
#[derive(Debug, Clone)]
struct ElementRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Screen-absolute bounding rectangle from the browser extension.
/// Used for `computer_click` fallback when JS clicks fail.
#[derive(Debug, Clone, serde::Deserialize)]
struct ScreenBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// A cached element from browser_get_elements.
#[derive(Debug, Clone)]
struct CachedElement {
    id: String,
    role: String,
    name: String,
    selector: String,
    rect: Option<ElementRect>,
    /// Preserve all original fields for browser_element_info.
    raw: serde_json::Value,
}

/// Cache of elements from the last browser_get_elements call.
struct ElementsCache {
    elements: Vec<CachedElement>,
}

/// Interactive ARIA roles that represent actionable UI elements.
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "menuitem",
    "tab",
    "slider",
    "switch",
    "spinbutton",
    "searchbox",
];

/// Parse element data from browser_get_elements into a cache.
///
/// Supports two formats:
/// 1. `refs` map format (from browser sidebar): `{ "refs": { "e10": {...}, ... } }`
/// 2. `elements` array format: `{ "elements": [{ "id": "e10", ... }, ...] }`
fn parse_elements_from_data(data: &serde_json::Value) -> Option<ElementsCache> {
    let mut elements = Vec::new();

    if let Some(refs) = data.get("refs").and_then(|r| r.as_object()) {
        // Format 1: refs map { "e10": { "name": "...", "role": "...", ... } }
        for (id, elem) in refs {
            let cached = parse_single_element(id, elem);
            elements.push(cached);
        }
    } else if let Some(elems) = data.get("elements").and_then(|e| e.as_array()) {
        // Format 2: elements array [{ "id": "e10", "name": "...", ... }]
        for elem in elems {
            let id = elem
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cached = parse_single_element(&id, elem);
            elements.push(cached);
        }
    } else {
        return None;
    }

    if elements.is_empty() {
        return None;
    }

    Some(ElementsCache { elements })
}

/// Extract the best CSS selector from an element's selectors array or legacy selector field.
fn extract_best_selector(elem: &serde_json::Value) -> String {
    // New format: "selectors" array of {type, strategy, value}
    if let Some(selectors) = elem.get("selectors").and_then(|v| v.as_array()) {
        // Prefer CSS selectors (skip a11y: locators)
        for s in selectors {
            if s.get("type").and_then(|t| t.as_str()) == Some("css") {
                if let Some(val) = s.get("value").and_then(|v| v.as_str()) {
                    return val.to_string();
                }
            }
        }
        // Fallback: first selector of any type
        if let Some(first) = selectors.first() {
            if let Some(val) = first.get("value").and_then(|v| v.as_str()) {
                return val.to_string();
            }
        }
    }
    // Legacy format: "selector" string
    elem.get("selector")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse a single element value into a CachedElement.
fn parse_single_element(id: &str, elem: &serde_json::Value) -> CachedElement {
    let role = elem
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = elem
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let selector = extract_best_selector(elem);
    let rect = elem.get("rect").and_then(|r| {
        Some(ElementRect {
            x: r.get("x")?.as_f64()?,
            y: r.get("y")?.as_f64()?,
            width: r.get("width")?.as_f64()?,
            height: r.get("height")?.as_f64()?,
        })
    });

    CachedElement {
        id: id.to_string(),
        role,
        name,
        selector,
        rect,
        raw: elem.clone(),
    }
}

/// Build a compact summary of cached elements for the LLM.
fn build_elements_summary(elements: &[CachedElement]) -> String {
    use std::collections::HashMap;

    // Count elements by role
    let mut role_counts: HashMap<&str, usize> = HashMap::new();
    for elem in elements {
        *role_counts.entry(&elem.role).or_insert(0) += 1;
    }

    // Collect interactive elements with non-empty names (max 20)
    let interactive_elements: Vec<serde_json::Value> = elements
        .iter()
        .filter(|e| {
            !e.name.is_empty()
                && INTERACTIVE_ROLES
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(&e.role))
        })
        .take(20)
        .map(|e| {
            let mut obj = serde_json::json!({
                "id": e.id,
                "role": e.role,
                "name": e.name,
            });
            if let Some(ref rect) = e.rect {
                obj["rect"] = serde_json::json!({
                    "x": rect.x as i64,
                    "y": rect.y as i64,
                    "width": rect.width as i64,
                    "height": rect.height as i64,
                });
            }
            obj
        })
        .collect();

    // Collect unnamed interactive elements (all of them)
    let unnamed_interactive: Vec<serde_json::Value> = elements
        .iter()
        .filter(|e| {
            e.name.is_empty()
                && INTERACTIVE_ROLES
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(&e.role))
        })
        .map(|e| {
            let mut obj = serde_json::json!({
                "id": e.id,
                "role": e.role,
                "name": "",
            });
            if let Some(ref rect) = e.rect {
                obj["rect"] = serde_json::json!({
                    "x": rect.x as i64,
                    "y": rect.y as i64,
                    "width": rect.width as i64,
                    "height": rect.height as i64,
                });
            }
            obj
        })
        .collect();

    let unnamed_count = unnamed_interactive.len();

    // Build role counts map sorted by count descending
    let mut sorted_roles: Vec<(&str, usize)> = role_counts.into_iter().collect();
    sorted_roles.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut roles_map = serde_json::Map::new();
    for (role, count) in sorted_roles {
        roles_map.insert(role.to_string(), serde_json::json!(count));
    }

    let summary = serde_json::json!({
        "success": true,
        "element_count": elements.len(),
        "roles": roles_map,
        "interactive_elements": interactive_elements,
        "unnamed_interactive_count": unnamed_count,
        "unnamed_interactive_elements": unnamed_interactive,
        "hint": "IMPORTANT: Do NOT summarize this data in natural language. You MUST follow this workflow: (1) Call browser_screenshot to capture the current page. (2) For unnamed elements, cross-reference their rect coordinates with the screenshot to determine their visual meaning/label. (3) Use browser_find_elements to search by role/name/selector/position. (4) Use browser_element_info(id) for full element details. (5) Output structured data, not a prose summary."
    });

    summary.to_string()
}

/// The built-in agent.
pub struct Agent<H: HostFunctions> {
    /// Host functions interface.
    host: H,
    /// Configuration.
    config: AgentConfig,
    /// Cached elements from browser_get_elements (replaced on each call).
    elements_cache: RefCell<Option<ElementsCache>>,
    /// Cached screenshot base64 from browser_screenshot (consumed once per call).
    screenshot_cache: RefCell<Option<String>>,
    /// Pending plan proposal from the plan tool (consumed by run_loop to break out).
    pending_plan: RefCell<Option<PlanProposal>>,
    /// Pending artifact from the create_artifact tool (consumed by run_loop to break out).
    pending_artifact: RefCell<Option<Artifact>>,
    /// Monotonic counter for generating unique artifact IDs.
    artifact_counter: Cell<u32>,
    /// Whether computer use has been triggered (for progressive prompt injection).
    computer_use_triggered: Cell<bool>,
    /// Current keywords extracted from user message and LLM context, used for auto-snapshots.
    current_keywords: RefCell<Vec<String>>,
    /// Skills that have been loaded in this session (prevent redundant re-loading).
    loaded_skills: RefCell<std::collections::HashSet<String>>,
}

// Static base prompts, compiled into the binary
const CHAT_PROMPT: &str = include_str!("../prompts/chat.md");
const BROWSER_PROMPT: &str = include_str!("../prompts/browser.md");
const AGENT_PROMPT: &str = include_str!("../prompts/agent.md");
const SUBAGENT_BROWSER_PROMPT: &str = include_str!("../prompts/subagent_browser.md");
const SUBAGENT_AGENT_PROMPT: &str = include_str!("../prompts/subagent_agent.md");

// Computer use prompt layers
const COMPUTER_USE_OVERVIEW: &str = include_str!("../prompts/computer_use_overview.md");
const COMPUTER_USE_GUIDE: &str = include_str!("../prompts/computer_use_guide.md");
const COMPUTER_USE_EXAMPLES: &str = include_str!("../prompts/computer_use_examples.md");

/// Controls which layers of the computer use prompt are injected.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputerUseFlags {
    pub inject_overview: bool,
    pub inject_guide: bool,
    pub inject_examples: bool,
}
/// Tools safe for parallel execution (rendered as `async def` in orchestrate).
/// All other tools default to sequential (`def`). This is fail-safe:
/// a new tool not in this list defaults to sequential (slower but correct).
///
/// This is the canonical list — `daemon/code_mode/signature.rs` imports it.
pub const ASYNC_SAFE_TOOLS: &[&str] = &[
    // Browser read-only
    "browser_screenshot",
    "browser_get_content",
    "browser_get_elements",
    "browser_get_element",
    "browser_query_all",
    "browser_get_markdown",
    "browser_eval_js",
    "browser_read_artifact",
    "browser_get_tabs",
    "browser_query_tabs",
    "browser_find_elements",
    "browser_element_info",
    // Network
    "web_search",
    "web_fetch",
    "fetch_page",
    // File read-only
    "read_file",
    "read",
    "glob",
    "grep",
    "list_files",
    // Memory
    "memory_search",
    "memory_create",
    "memory_update",
    "memory_delete",
    "memory_view",
    // MCP
    "mcp_list_tools",
    "mcp_call",
    "mcp_read_resource",
    "tool_search",
    "tool_call_dynamic",
    // Meta
    "think",
    "switch_model",
];

/// Check if a tool should be marked sequential in orchestrate signatures.
///
/// Tools *not* in the ASYNC_SAFE_TOOLS list are sequential — they modify browser
/// state (navigation, clicks, etc.) and must be called one at a time.
fn is_orchestrate_sequential(name: &str) -> bool {
    !ASYNC_SAFE_TOOLS.contains(&name)
}

/// Compact JSON Schema -> Python type (just primitives, no TypedDict).
fn schema_to_py_type_compact(schema: &serde_json::Value) -> &'static str {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => "str",
        Some("integer") => "int",
        Some("number") => "float",
        Some("boolean") => "bool",
        Some("array") => "list",
        Some("object") => "dict",
        _ => "Any",
    }
}

/// Extract a compact parameter string from a tool's JSON Schema.
fn extract_params_compact(schema: &serde_json::Value) -> String {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return String::new(),
    };
    let required: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut req_params = Vec::new();
    let mut opt_params = Vec::new();

    for (name, prop_schema) in props {
        let py_type = schema_to_py_type_compact(prop_schema);
        if required.contains(name.as_str()) {
            req_params.push(format!("{}: {}", name, py_type));
        } else {
            let default = prop_schema
                .get("default")
                .map(|d| match d {
                    serde_json::Value::Null => "None".to_string(),
                    serde_json::Value::Bool(true) => "True".to_string(),
                    serde_json::Value::Bool(false) => "False".to_string(),
                    serde_json::Value::String(s) => format!("\"{}\"", s),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "None".to_string(),
                })
                .unwrap_or_else(|| "None".to_string());
            opt_params.push(format!("{}: {} = {}", name, py_type, default));
        }
    }

    req_params.extend(opt_params);
    req_params.join(", ")
}

impl<H: HostFunctions> Agent<H> {
    /// Create a new agent with the given host functions.
    pub fn new(host: H) -> Self {
        Self {
            host,
            config: AgentConfig::default(),
            elements_cache: RefCell::new(None),
            screenshot_cache: RefCell::new(None),
            pending_plan: RefCell::new(None),
            pending_artifact: RefCell::new(None),
            artifact_counter: Cell::new(0),
            computer_use_triggered: Cell::new(false),
            current_keywords: RefCell::new(Vec::new()),
            loaded_skills: RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Create a new agent with custom configuration.
    pub fn with_config(host: H, config: AgentConfig) -> Self {
        Self {
            host,
            config,
            elements_cache: RefCell::new(None),
            screenshot_cache: RefCell::new(None),
            pending_plan: RefCell::new(None),
            pending_artifact: RefCell::new(None),
            artifact_counter: Cell::new(0),
            computer_use_triggered: Cell::new(false),
            current_keywords: RefCell::new(Vec::new()),
            loaded_skills: RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Run the agent for a single turn.
    pub fn run(&self, input: &AgentInput) -> HostResult<AgentOutput> {
        // Check for keyword triggers in user message before building prompt
        if !self.computer_use_triggered.get() && should_trigger_computer_use(&input.user_message) {
            self.computer_use_triggered.set(true);
        }

        let mode = input.mode;

        // Use custom system prompt if provided, otherwise use mode-based prompt
        let base_prompt = match &input.custom_system_prompt {
            Some(custom) => custom.clone(),
            None => {
                let skills = self.host.skill_list().unwrap_or_default();
                let cu_flags = self.computer_use_flags(mode);
                Self::build_system_prompt(
                    mode,
                    &skills,
                    &input.available_models,
                    cu_flags,
                    input.os_platform.as_deref(),
                )
            }
        };

        // Append soul document context if available
        let base_prompt = if let Some(ref soul) = input.soul_context {
            format!("{}\n\n{}", base_prompt, soul)
        } else {
            base_prompt
        };

        // Prepend skill context with high priority if present
        let system_prompt = match &input.skill_context {
            Some(skill) => {
                let files_section = if !skill.available_files.is_empty() {
                    let file_list: String = skill
                        .available_files
                        .iter()
                        .map(|f| format!("- {}", f))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!(
                        r#"

<available_files base_path="{}">
{}
</available_files>
To read files listed above, use the `read` tool with just the filename (e.g., `resume.md`). Do NOT fabricate absolute paths."#,
                        skill.base_path, file_list
                    )
                } else {
                    String::new()
                };

                format!(
                    r#"<CRITICAL_INSTRUCTIONS priority="highest">
The following skill instructions MUST be followed exactly. These instructions take absolute priority over all other guidance.

<skill name="{}" base_path="{}">
{}
</skill>{}
</CRITICAL_INSTRUCTIONS>

{}"#,
                    skill.name, skill.base_path, skill.content, files_section, base_prompt
                )
            }
            None => base_prompt,
        };

        let mut tools = if self.config.is_subagent {
            self.get_subagent_tools_for_mode(mode)
        } else {
            self.get_tools_for_mode(mode)
        };

        // Apply tool filtering based on tools_config
        tools = self.filter_tools(tools, &input.tools_config);

        // When user attached specific tabs, update browser tool tab_id descriptions
        // to guide the LLM toward the attached tabs instead of defaulting to current_tab.
        if !input.tab_ids.is_empty() {
            let attached: Vec<String> = input
                .tab_ids
                .iter()
                .map(|t| format!("{} (\"{}\")", t.tab_id, t.tab_title))
                .collect();
            let hint = format!(
                "Tab ID. The user attached tabs: {}. Unless the user explicitly asks for the current tab, use the attached tab's ID.",
                attached.join(", ")
            );
            for tool in &mut tools {
                if matches!(
                    tool.name.as_str(),
                    "browser_get_markdown" | "browser_get_content" | "browser_screenshot"
                ) {
                    if let Some(props) = tool
                        .input_schema
                        .get_mut("properties")
                        .and_then(|p| p.as_object_mut())
                    {
                        if let Some(tab_id_prop) = props.get_mut("tab_id") {
                            tab_id_prop["description"] = serde_json::Value::String(hint.clone());
                        }
                    }
                }
            }
        }

        self.run_loop(input, &system_prompt, &tools)
    }

    /// Get tools for a specific mode. Public so daemon-side callers (e.g.
    /// the /loop iteration executor) can enumerate the canonical tool list
    /// for a given mode without going through `Agent::run`.
    pub fn get_tools_for_mode(&self, mode: AgentMode) -> Vec<ToolDefinition> {
        match mode {
            AgentMode::Chat => self.get_chat_tools(),
            AgentMode::Browser => self.get_browser_tools(),
            AgentMode::Agent => self.get_agent_tools(),
            AgentMode::Code => self.get_agent_tools(),
        }
    }

    /// Filter tools based on the tools_config.
    ///
    /// - `None` (Option::None / inherit): returns the full tool set unchanged
    /// - `Some(ToolsConfig::None)`: returns an empty vec (no tools)
    /// - `Some(ToolsConfig::Allow(list))`: keeps only tools matching the allowlist
    fn filter_tools(
        &self,
        tools: Vec<ToolDefinition>,
        tools_config: &Option<nevoflux_protocol::subagent::ToolsConfig>,
    ) -> Vec<ToolDefinition> {
        match tools_config {
            None => tools, // inherit: full tool set
            Some(nevoflux_protocol::subagent::ToolsConfig::None) => Vec::new(),
            Some(nevoflux_protocol::subagent::ToolsConfig::Allow(ref allowlist)) => tools
                .into_iter()
                .filter(|t| nevoflux_protocol::subagent::is_tool_allowed(allowlist, &t.name))
                .collect(),
        }
    }

    /// Get the static base prompt for a mode.
    fn base_prompt_for_mode(mode: AgentMode) -> &'static str {
        match mode {
            AgentMode::Chat => CHAT_PROMPT,
            AgentMode::Browser => BROWSER_PROMPT,
            AgentMode::Agent => AGENT_PROMPT,
            AgentMode::Code => AGENT_PROMPT,
        }
    }

    /// Get the static subagent prompt for a mode.
    pub fn subagent_prompt_for_mode(mode: AgentMode) -> &'static str {
        match mode {
            // Chat mode doesn't expose subagent tools,
            // but fallback to browser-level if somehow called
            AgentMode::Chat => SUBAGENT_BROWSER_PROMPT,
            AgentMode::Browser => SUBAGENT_BROWSER_PROMPT,
            AgentMode::Agent => SUBAGENT_AGENT_PROMPT,
            AgentMode::Code => SUBAGENT_AGENT_PROMPT,
        }
    }

    /// Build the full system prompt with dynamic sections appended.
    /// Called once per session; result should be cached.
    fn build_system_prompt(
        mode: AgentMode,
        skills: &[SkillSummary],
        models: &[(String, String)],
        computer_use: ComputerUseFlags,
        os_platform: Option<&str>,
    ) -> String {
        let mut prompt = Self::base_prompt_for_mode(mode).to_string();

        // Inject platform info so LLM uses correct shell syntax
        if let Some(os) = os_platform {
            let shell_hint = match os {
                "windows" => "Windows (PowerShell). Use PowerShell syntax for commands.",
                "macos" => "macOS (zsh/bash). Use POSIX shell syntax for commands.",
                _ => "Linux (bash). Use POSIX shell syntax for commands.",
            };
            prompt.push_str(&format!(
                "\n\n# System Environment\n\nOperating System: {}\n",
                shell_hint
            ));
        }

        // Append computer use prompt layers based on flags
        if computer_use.inject_overview {
            prompt.push_str("\n\n");
            prompt.push_str(COMPUTER_USE_OVERVIEW);
        }
        if computer_use.inject_guide {
            prompt.push_str("\n\n");
            prompt.push_str(COMPUTER_USE_GUIDE);
        }
        if computer_use.inject_examples {
            prompt.push_str("\n\n");
            prompt.push_str(COMPUTER_USE_EXAMPLES);
        }

        if !models.is_empty() {
            prompt.push_str("\n\n# Available models\n\n");
            for (provider, model) in models {
                prompt.push_str(&format!("- {}: {}\n", provider, model));
            }
        }

        if !skills.is_empty() {
            prompt.push_str("\n\n# Skills\n\n");
            prompt.push_str("The following skills are available. When a user's request matches a skill's description, you MUST use `skill_load(name)` to load the skill's full instructions BEFORE responding. Skills provide specialized workflows that produce better results than generic responses. Even a partial match (e.g., user asks to \"build a dashboard\" and a skill handles web apps) means you should load the skill.\n\n");
            prompt.push_str(&format_skill_summaries(skills));
            prompt.push_str("\n\nUsers can also invoke skills explicitly with `/skill_name`. If the user's message starts with `/`, treat the first word as a skill name.");
        }

        prompt
    }

    /// Determine computer use flags based on mode and trigger state.
    fn computer_use_flags(&self, mode: AgentMode) -> ComputerUseFlags {
        let triggered = self.computer_use_triggered.get();
        match mode {
            AgentMode::Chat => ComputerUseFlags::default(),
            AgentMode::Browser => {
                if triggered {
                    ComputerUseFlags {
                        inject_overview: true,
                        inject_guide: true,
                        inject_examples: true,
                    }
                } else {
                    ComputerUseFlags {
                        inject_overview: true,
                        inject_guide: false,
                        inject_examples: false,
                    }
                }
            }
            AgentMode::Agent | AgentMode::Code => {
                if triggered {
                    ComputerUseFlags {
                        inject_overview: true,
                        inject_guide: true,
                        inject_examples: true,
                    }
                } else {
                    ComputerUseFlags {
                        inject_overview: true,
                        inject_guide: true,
                        inject_examples: false,
                    }
                }
            }
        }
    }

    /// Get tools for a subagent based on mode (read-only browser access).
    fn get_subagent_tools_for_mode(&self, mode: AgentMode) -> Vec<ToolDefinition> {
        match mode {
            AgentMode::Chat => self.get_chat_tools(),
            AgentMode::Browser => self.get_subagent_browser_read_tools(),
            AgentMode::Agent | AgentMode::Code => {
                let mut tools = self.get_subagent_browser_read_tools();
                tools.extend(self.get_file_tools());
                tools
            }
        }
    }

    /// Get read-only browser tools for subagents.
    ///
    /// Includes chat tools + browser_get_content, browser_get_markdown, browser_screenshot.
    /// Does NOT include interaction tools (click, type, fill, scroll, navigate, etc.).
    fn get_subagent_browser_read_tools(&self) -> Vec<ToolDefinition> {
        // Start with chat tools (think, plan, switch_model, web_search, web_fetch, ask_user,
        // memory_search, skill_load, browser_get_content, browser_get_markdown, browser_screenshot)
        // Chat tools already include browser_get_content, browser_get_markdown, browser_screenshot
        self.get_chat_tools()
    }

    /// Get file/system tools (read, write, edit, bash, glob, grep, tool_search, tool_call_dynamic).
    fn get_file_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "read".into(),
                description: "Read file contents. Returns partial content with metadata (total_lines, total_bytes). Default: first 200 lines. Use offset/limit to paginate.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The absolute path to read"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line offset to start reading from"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum lines to read"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolDefinition {
                name: "write".into(),
                description: "Write content to a file".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The absolute path to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write"
                        }
                    },
                    "required": ["file_path", "content"]
                }),
            },
            ToolDefinition {
                name: "edit".into(),
                description: "Edit a file using search and replace".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The text to find"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement text"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "Replace all occurrences"
                        }
                    },
                    "required": ["file_path", "old_string", "new_string"]
                }),
            },
            ToolDefinition {
                name: "bash".into(),
                description: "Execute a shell command. Default timeout: 30s. Output capped at 200 lines.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Timeout in milliseconds"
                        }
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "glob".into(),
                description: "Find files matching a pattern".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern like '**/*.rs'"
                        },
                        "path": {
                            "type": "string",
                            "description": "Base directory"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "grep".into(),
                description: "Search file contents with regex. Returns structured matches with counts.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern (regex)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search"
                        },
                        "type": {
                            "type": "string",
                            "description": "File type filter (e.g., 'rs', 'py', 'js')"
                        },
                        "case_insensitive": {
                            "type": "boolean",
                            "description": "Case insensitive search (default: false)"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum results to return (default: 50)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        ]
    }

    /// Core agent loop.
    fn run_loop(
        &self,
        input: &AgentInput,
        system_prompt: &str,
        tools: &[ToolDefinition],
    ) -> HostResult<AgentOutput> {
        let mut messages = vec![Message::system(system_prompt)];
        messages.extend(input.history.clone());

        // Build context prefixes for user message
        let local_files_prefix = format_local_files(&input.local_files);
        // Build active_tab TabInfo from tab_id if we don't have it in tab_ids
        let active_tab_info = input.tab_id.map(|id| {
            input
                .tab_ids
                .iter()
                .find(|t| t.tab_id == id)
                .cloned()
                .unwrap_or(TabInfo {
                    tab_id: id,
                    tab_title: String::new(),
                    url: String::new(),
                    space: String::new(),
                })
        });
        let tab_context_prefix = Self::format_tab_context(active_tab_info.as_ref(), &input.tab_ids);

        // For browser/agent mode: extract keywords from user message and take initial viewport snapshot
        let initial_snapshot = if matches!(input.mode, AgentMode::Browser | AgentMode::Agent) {
            let keywords = Self::extract_keywords_from_text(&input.user_message);
            if !keywords.is_empty() {
                eprintln!("[AGENT] Initial keywords from user message: {:?}", keywords);
            }
            let kw_arg = if keywords.is_empty() {
                None
            } else {
                Some(keywords.clone())
            };
            *self.current_keywords.borrow_mut() = keywords;
            let snapshot_text = self.get_viewport_snapshot_text(input.tab_id, kw_arg);
            if !snapshot_text.is_empty() {
                format!("\n\nCurrent page state:\n{}", snapshot_text)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Combine prefixes with user message
        let user_content = match (local_files_prefix.is_empty(), tab_context_prefix.is_empty()) {
            (true, true) => input.user_message.clone(),
            (false, true) => format!("{}{}", local_files_prefix, input.user_message),
            (true, false) => format!("{}\n\n{}", tab_context_prefix, input.user_message),
            (false, false) => format!(
                "{}{}\n\n{}",
                local_files_prefix, tab_context_prefix, input.user_message
            ),
        };
        let user_content = format!("{}{}", user_content, initial_snapshot);

        // Create user message with optional attachments
        if input.attachments.is_empty() {
            messages.push(Message::user(&user_content));
        } else {
            messages.push(Message::user_with_attachments(
                &user_content,
                input.attachments.clone(),
            ));
        }

        let mut iterations = 0;
        let mut final_text = String::new();
        let mut all_tool_calls = Vec::new();

        loop {
            iterations += 1;
            let _ = self.host.set_iteration(iterations as u32);
            if iterations > self.config.max_iterations {
                break;
            }

            // Check for interrupt signal from sidebar
            if self.host.is_interrupted()? {
                break;
            }

            // Use streaming or non-streaming LLM based on config
            let response = if self.config.use_streaming && !self.config.suppress_streaming {
                self.call_llm_streaming(&messages, tools)?
            } else {
                // Call LLM non-streaming
                let request = LlmRequest {
                    messages: messages.clone(),
                    tools: tools.to_vec(),
                    stream: false,
                };
                self.host.llm_chat(&request)?
            };

            // If no tool calls, we're done
            if response.tool_calls.is_empty() {
                final_text = response.text;
                break;
            }

            // When tools are disabled (empty tools vec), ignore any tool_use blocks
            // and treat the response as a final text response.
            if tools.is_empty() {
                final_text = response.text;
                break;
            }

            // Execute tool calls - must include tool_calls in the assistant message
            let tool_calls = response.tool_calls;
            messages.push(Message::assistant_with_tool_calls_and_reasoning(
                &response.text,
                tool_calls.clone(),
                response.reasoning.clone(),
            ));

            // Extract keywords from LLM reasoning text once (invariant across tool calls)
            let llm_kws = Self::extract_keywords_from_text(&response.text);

            for tool_call in &tool_calls {
                // Update keywords from pre-extracted LLM keywords + tool args
                self.update_keywords_from_tool_context(&llm_kws, tool_call);

                eprintln!(
                    "[AGENT] Executing tool: name={}, id={}, call_id={:?}, args={}",
                    tool_call.name, tool_call.id, tool_call.call_id, tool_call.arguments
                );
                let result = match self.execute_tool(tool_call) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "[AGENT] Tool execution failed: name={}, error={}",
                            tool_call.name, e.message
                        );
                        ToolResult {
                            tool_call_id: tool_call.call_id.clone().unwrap_or(tool_call.id.clone()),
                            content: format!("Error: {}", e.message),
                            success: false,
                        }
                    }
                };
                eprintln!(
                    "[AGENT] Tool result will use tool_call_id={}",
                    result.tool_call_id
                );
                // Find safe UTF-8 boundary for preview (handles multi-byte chars like Chinese)
                let preview = truncate_string_safe(&result.content, 200);
                eprintln!(
                    "[AGENT] Tool result: success={}, content_len={}, content={:?}",
                    result.success,
                    result.content.len(),
                    preview
                );

                // Dynamic truncation based on current message size
                let content = truncate_tool_result_if_needed(&messages, &result.content);

                // Check if there's a cached screenshot to attach (base64 stays out of content)
                let attachments = if tool_call.name == "browser_screenshot" {
                    self.screenshot_cache
                        .borrow_mut()
                        .take()
                        .map(|base64| {
                            // Detect actual image format from base64 magic bytes
                            let mime_type: String = if base64.starts_with("/9j/") {
                                "image/jpeg"
                            } else {
                                "image/png" // PNG or default fallback
                            }
                            .into();
                            vec![Attachment {
                                name: "screenshot.png".into(),
                                mime_type,
                                data: base64,
                            }]
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                // Use call_id (from result.tool_call_id) for the tool message
                // This was set in execute_tool to use call_id when available
                messages.push(Message {
                    role: MessageRole::Tool,
                    content,
                    tool_call_id: Some(result.tool_call_id.clone()),
                    tool_calls: vec![],
                    attachments,
                    reasoning: None,
                });

                // Check interrupt after each tool execution
                if self.host.is_interrupted()? {
                    break;
                }
            }

            // Move tool calls into the accumulator (avoids a second clone)
            all_tool_calls.extend(tool_calls);

            // Check if we should exit the outer loop due to interrupt
            if self.host.is_interrupted()? {
                break;
            }

            // Check if a plan was proposed — break out and return to runner
            if let Some(proposal) = self.pending_plan.borrow_mut().take() {
                // Signal end of stream if streaming was enabled
                if self.config.use_streaming && !self.config.suppress_streaming {
                    let _ = self.host.stream_end();
                }

                return Ok(AgentOutput {
                    text: final_text,
                    tool_calls: all_tool_calls,
                    continue_loop: false,
                    plan_proposal: Some(proposal),
                    artifact: None,
                });
            }

            // Check if an artifact was created — break out and return to runner
            if let Some(artifact) = self.pending_artifact.borrow_mut().take() {
                if self.config.use_streaming && !self.config.suppress_streaming {
                    let _ = self.host.stream_end();
                }

                return Ok(AgentOutput {
                    text: final_text,
                    tool_calls: all_tool_calls,
                    continue_loop: false,
                    plan_proposal: None,
                    artifact: Some(artifact),
                });
            }
        }

        // Signal end of stream if streaming was enabled
        if self.config.use_streaming && !self.config.suppress_streaming {
            let _ = self.host.stream_end();
        }

        Ok(AgentOutput {
            text: final_text,
            tool_calls: all_tool_calls,
            continue_loop: false,
            plan_proposal: None,
            artifact: None,
        })
    }

    /// Call LLM with streaming support.
    ///
    /// This method starts a stream, emits chunks to the sidebar, and returns
    /// the accumulated response.
    fn call_llm_streaming(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> HostResult<LlmResponse> {
        let request = LlmRequest {
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            stream: true,
        };

        // Start the stream
        let stream_id = self.host.llm_stream_start(&request)?;

        let mut accumulated_text = String::new();
        let mut accumulated_reasoning = String::new();
        // Use a HashMap to deduplicate tool calls by id, preferring those with call_id set
        let mut tool_calls_map: std::collections::HashMap<String, ToolCall> =
            std::collections::HashMap::new();

        // Buffering state for text-based <tool_call> XML that may span multiple chunks
        let mut tool_call_buf = String::new();
        let mut in_tool_call = false;
        // Buffer for partial <tool_call> tag prefix split at chunk boundary
        let mut tag_buf = String::new();

        // Read chunks until done
        loop {
            // Check for interrupt
            if self.host.is_interrupted()? {
                self.host.llm_stream_close(stream_id)?;
                break;
            }

            match self.host.llm_stream_next(stream_id)? {
                Some(chunk) => {
                    // Accumulate text, detecting <tool_call> XML that some providers
                    // emit as plain text instead of structured tool_use blocks.
                    // The opening tag may be split across chunks (e.g. "<tool_call"
                    // in one chunk and ">" in the next), so we buffer partial prefixes.
                    if let Some(ref chunk_text) = chunk.text {
                        if !chunk_text.is_empty() {
                            // Prepend any partial tag prefix from previous chunk
                            let text = if !tag_buf.is_empty() {
                                std::mem::take(&mut tag_buf) + chunk_text
                            } else {
                                chunk_text.to_string()
                            };
                            let text = text.as_str();

                            if in_tool_call {
                                // Continue buffering a multi-chunk tool call
                                tool_call_buf.push_str(text);
                                // Check accumulated buffer (not just current chunk)
                                // so split </tool_call> tags are handled correctly
                                if tool_call_buf.contains("</tool_call>") {
                                    in_tool_call = false;
                                    let complete = std::mem::take(&mut tool_call_buf);
                                    let (clean, extracted) = parse_tool_calls_from_text(&complete);
                                    for tc in extracted {
                                        tool_calls_map.insert(tc.id.clone(), tc);
                                    }
                                    if !clean.is_empty() {
                                        accumulated_text.push_str(&clean);
                                        self.host.stream_emit(&clean)?;
                                    }
                                }
                            } else if text.contains("<tool_call>") {
                                if text.contains("</tool_call>") {
                                    // Complete tool call in a single chunk
                                    let (clean, extracted) = parse_tool_calls_from_text(text);
                                    for tc in extracted {
                                        tool_calls_map.insert(tc.id.clone(), tc);
                                    }
                                    if !clean.is_empty() {
                                        accumulated_text.push_str(&clean);
                                        self.host.stream_emit(&clean)?;
                                    }
                                } else {
                                    // Starts here, doesn't end — buffer
                                    let idx = text.find("<tool_call>").unwrap();
                                    let before = &text[..idx];
                                    if !before.trim().is_empty() {
                                        accumulated_text.push_str(before);
                                        self.host.stream_emit(before)?;
                                    }
                                    tool_call_buf = text[idx..].to_string();
                                    in_tool_call = true;
                                }
                            } else if let Some(split) = find_tool_call_tag_prefix_at_end(text) {
                                // Text ends with a partial <tool_call> prefix —
                                // hold it back until the next chunk confirms
                                let (emit, hold) = text.split_at(split);
                                tag_buf = hold.to_string();
                                if !emit.is_empty() {
                                    accumulated_text.push_str(emit);
                                    self.host.stream_emit(emit)?;
                                }
                            } else {
                                // Normal text — no tool call markers
                                accumulated_text.push_str(text);
                                self.host.stream_emit(text)?;
                            }
                        }
                    }

                    // Handle generated images: emit as inline data URL markdown
                    if !chunk.images.is_empty() {
                        for img in &chunk.images {
                            let md = format!(
                                "\n\n![Generated Image](data:{};base64,{})\n",
                                img.media_type, img.data
                            );
                            accumulated_text.push_str(&md);
                            self.host.stream_emit(&md)?;
                        }
                    }

                    // Accumulate reasoning/thinking content deltas
                    if let Some(reasoning_delta) = chunk.reasoning.as_deref() {
                        if !reasoning_delta.is_empty() {
                            accumulated_reasoning.push_str(reasoning_delta);
                        }
                    }

                    // Accumulate tool calls, preferring those with call_id set
                    // This handles the case where OpenAI Responses API sends both
                    // delta-accumulated tool calls (without call_id) and complete
                    // tool calls (with call_id) for the same id
                    for tc in chunk.tool_calls {
                        let should_insert = match tool_calls_map.get(&tc.id) {
                            None => true,
                            Some(existing) => {
                                // Prefer the one with call_id, or the newer one if both have it
                                tc.call_id.is_some() || existing.call_id.is_none()
                            }
                        };
                        if should_insert {
                            tool_calls_map.insert(tc.id.clone(), tc);
                        }
                    }

                    if chunk.done {
                        // Flush incomplete tag prefix buffer as plain text
                        if !tag_buf.is_empty() {
                            let buf = std::mem::take(&mut tag_buf);
                            accumulated_text.push_str(&buf);
                            self.host.stream_emit(&buf)?;
                        }
                        // Handle incomplete tool call buffer — the model
                        // truncated output before sending </tool_call>.
                        // Don't dump raw XML to the sidebar; try to extract
                        // the tool name for a helpful warning instead.
                        if in_tool_call {
                            let buf = std::mem::take(&mut tool_call_buf);
                            // Try to identify which tool was being called
                            let tool_name = buf
                                .find("\"name\"")
                                .and_then(|i| {
                                    let after = &buf[i + 6..];
                                    let colon = after.find(':')?;
                                    let after_colon = after[colon + 1..].trim_start();
                                    let quote_start = after_colon.strip_prefix('"')?;
                                    let end = quote_start.find('"')?;
                                    Some(&quote_start[..end])
                                })
                                .unwrap_or("unknown");
                            let warning =
                                format!("\n[Tool call `{}` was truncated by the model]", tool_name);
                            accumulated_text.push_str(&warning);
                            self.host.stream_emit(&warning)?;
                        }
                        break;
                    }
                }
                None => {
                    // No more chunks available, wait a bit and try again
                    // In WASM context, we might need to yield
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        // Close the stream
        self.host.llm_stream_close(stream_id)?;

        Ok(LlmResponse {
            text: accumulated_text,
            tool_calls: tool_calls_map.into_values().collect(),
            reasoning: if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(accumulated_reasoning)
            },
        })
    }

    /// Normalize PascalCase tool names from Claude Code conventions to snake_case.
    /// e.g. "ToolSearch" → "tool_search", "WebSearch" → "web_search"
    fn normalize_tool_name(name: &str) -> &str {
        match name {
            "ToolSearch" => "tool_search",
            "WebSearch" => "web_search",
            "WebFetch" => "web_fetch",
            "Skill" => "skill_load",
            _ => name,
        }
    }

    /// Execute a single tool call.
    fn execute_tool(&self, tool_call: &ToolCall) -> HostResult<ToolResult> {
        let normalized_name = Self::normalize_tool_name(&tool_call.name);
        let content = match normalized_name {
            "think" => {
                // Think tool: no side effects, just returns acknowledgment.
                // The thought content is recorded in trace via the tool_call arguments.
                r#"{"status":"ok"}"#.to_string()
            }
            "plan" => {
                let summary = tool_call.arguments["summary"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let steps: Vec<PlanStep> = tool_call.arguments["steps"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|s| PlanStep {
                                description: s["description"].as_str().unwrap_or("").to_string(),
                                model: s["model"].as_str().map(|m| m.to_string()),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                *self.pending_plan.borrow_mut() = Some(PlanProposal { summary, steps });

                "Plan submitted for user review.".to_string()
            }
            "create_artifact" => {
                let title = tool_call.arguments["title"]
                    .as_str()
                    .unwrap_or("Untitled")
                    .to_string();
                let content_type = tool_call.arguments["content_type"]
                    .as_str()
                    .unwrap_or("text/html")
                    .to_string();
                let content = tool_call.arguments["content"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                // Extract optional multi-file project fields.
                // Accept both object (Anthropic) and JSON string (OpenAI strict mode).
                let files: Option<std::collections::HashMap<String, String>> =
                    tool_call.arguments.get("files").and_then(|f| {
                        // Try as object first (Anthropic/non-strict providers)
                        if let Some(obj) = f.as_object() {
                            Some(
                                obj.iter()
                                    .filter_map(|(k, v)| {
                                        v.as_str().map(|s| (k.clone(), s.to_string()))
                                    })
                                    .collect(),
                            )
                        } else if let Some(s) = f.as_str() {
                            // Try as JSON string (OpenAI strict mode)
                            serde_json::from_str(s).ok()
                        } else {
                            None
                        }
                    });
                let entry = tool_call
                    .arguments
                    .get("entry")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string());
                let description = tool_call
                    .arguments
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                // Generate a unique artifact ID using timestamp + counter.
                // LLM-generated tool call IDs (e.g. "call_1") reset each turn
                // and would cause collisions, so we use server-side generation.
                let seq = self.artifact_counter.get() + 1;
                self.artifact_counter.set(seq);
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let id = format!("art-{}-{}", ts, seq);

                *self.pending_artifact.borrow_mut() = Some(Artifact {
                    id: id.clone(),
                    title,
                    content_type,
                    description,
                    content,
                    files,
                    entry,
                    is_persistent: false,
                });

                format!("Artifact created and sent to canvas: {}", id)
            }
            "switch_model" => {
                let provider = tool_call.arguments["provider"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let model = tool_call.arguments["model"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();

                if provider.is_empty() || model.is_empty() {
                    r#"{"error":"provider and model are required"}"#.to_string()
                } else {
                    match self.host.set_model_override(&provider, &model) {
                        Ok(()) => format!(
                            r#"{{"status":"ok","provider":"{}","model":"{}"}}"#,
                            provider, model
                        ),
                        Err(e) => format!(r#"{{"error":"{}"}}"#, e.message),
                    }
                }
            }
            "web_search" => {
                let query = tool_call.arguments["query"].as_str().unwrap_or("");
                self.host.tool_web_search(query)?
            }
            "web_fetch" => {
                let url = tool_call.arguments["url"].as_str().unwrap_or("");
                let prompt = tool_call.arguments["prompt"]
                    .as_str()
                    .unwrap_or("Extract the main content");
                self.host.tool_web_fetch(url, prompt)?
            }
            "ask_user" => {
                let question = tool_call.arguments["question"].as_str().unwrap_or("");
                let options: Vec<String> = tool_call.arguments["options"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                self.host.tool_ask_user(question, &options)?
            }
            "read" => {
                let path = tool_call.arguments["file_path"].as_str().unwrap_or("");
                let offset = tool_call.arguments["offset"].as_u64();
                let limit = tool_call.arguments["limit"].as_u64();
                let result = self.host.tool_read(path, offset, limit)?;
                format_read_result(&result, path)
            }
            "write" => {
                let path = tool_call.arguments["file_path"].as_str().unwrap_or("");
                let content = tool_call.arguments["content"].as_str().unwrap_or("");
                self.host.tool_write(path, content)?;
                "File written successfully.".to_string()
            }
            "edit" => {
                let path = tool_call.arguments["file_path"].as_str().unwrap_or("");
                let old_string = tool_call.arguments["old_string"].as_str().unwrap_or("");
                let new_string = tool_call.arguments["new_string"].as_str().unwrap_or("");
                let replace_all = tool_call.arguments["replace_all"]
                    .as_bool()
                    .unwrap_or(false);
                self.host
                    .tool_edit(path, old_string, new_string, replace_all)?;
                "File edited successfully.".to_string()
            }
            "bash" => {
                let command = tool_call.arguments["command"].as_str().unwrap_or("");
                let timeout = tool_call.arguments["timeout"].as_u64();
                let result = self.host.tool_bash(command, timeout)?;
                format_bash_result(&result)
            }
            "glob" => {
                let pattern = tool_call.arguments["pattern"].as_str().unwrap_or("*");
                let path = tool_call.arguments["path"].as_str();
                let files = self.host.tool_glob(pattern, path)?;
                files.join("\n")
            }
            "grep" => {
                let pattern = tool_call.arguments["pattern"].as_str().unwrap_or("");
                let path = tool_call.arguments["path"].as_str();
                let file_type = tool_call.arguments["type"].as_str();
                let case_insensitive = tool_call.arguments["case_insensitive"].as_bool();
                let max_results = tool_call.arguments["max_results"].as_u64();
                let result =
                    self.host
                        .tool_grep(pattern, path, file_type, case_insensitive, max_results)?;
                format_grep_result(&result, pattern)
            }
            "memory_search" => {
                let query = tool_call.arguments["query"].as_str().unwrap_or("");
                let limit = tool_call.arguments["limit"].as_u64().unwrap_or(10) as usize;
                let chunks = self.host.memory_search(query, limit)?;
                serde_json::to_string_pretty(&chunks).unwrap_or_default()
            }
            "memory_create" => {
                let content = tool_call.arguments["content"].as_str().unwrap_or("");
                let mut metadata = tool_call
                    .arguments
                    .get("metadata")
                    .and_then(|v| {
                        if v.is_object() {
                            Some(v.clone())
                        } else {
                            v.as_str()
                                .filter(|s| !s.is_empty())
                                .and_then(|s| serde_json::from_str(s).ok())
                        }
                    })
                    .unwrap_or(serde_json::json!({}));

                // Merge explicit category/domain args into metadata so
                // host.memory_create() can read them uniformly.
                if let Some(cat) = tool_call.arguments["category"].as_str() {
                    metadata["category"] = serde_json::json!(cat);
                }
                if let Some(dom) = tool_call.arguments["domain"].as_str() {
                    metadata["domain"] = serde_json::json!(dom);
                }

                // Delegate to host.memory_create() which handles:
                // - knowledge_teach (create + validate + mark hot)
                // - mark_manual_create (suppress auto-extraction this turn)
                let id = self.host.memory_create(content, &metadata)?;
                serde_json::json!({"id": id, "status": "created"}).to_string()
            }
            "memory_update" => {
                let id = tool_call.arguments["id"].as_str().unwrap_or("");
                let content = tool_call.arguments["content"].as_str().unwrap_or("");
                self.host.memory_update(id, content)?;
                serde_json::json!({"id": id, "status": "updated"}).to_string()
            }
            "memory_delete" => {
                let id = tool_call.arguments["id"].as_str().unwrap_or("");
                self.host.memory_delete(id)?;
                serde_json::json!({"id": id, "status": "deleted"}).to_string()
            }
            "memory_view" => {
                let limit = tool_call.arguments["limit"].as_u64().unwrap_or(20) as usize;
                let entries = self.host.memory_view(limit)?;
                serde_json::to_string_pretty(&entries).unwrap_or_default()
            }
            "skill_load" => {
                let name = tool_call.arguments["name"].as_str().unwrap_or("");
                // Prevent redundant re-loading of already loaded skills.
                // The full skill content (including API specs) is already in
                // conversation history from the first load.
                if self.loaded_skills.borrow().contains(name) {
                    format!(
                        "Skill '{}' is already loaded in this session. \
                         Refer to the earlier skill_load result in conversation history \
                         for the full instructions and API specifications. \
                         Do NOT create a new artifact unless the user explicitly asks for it.",
                        name
                    )
                } else {
                    let content = self.host.skill_load(name)?;
                    self.loaded_skills.borrow_mut().insert(name.to_string());
                    content
                }
            }
            "tool_search" => {
                let query = tool_call.arguments["query"].as_str().unwrap_or("");
                let max_results = tool_call.arguments["max_results"].as_u64().unwrap_or(5) as usize;
                let results = self.host.tool_search(query, max_results)?;
                serde_json::to_string_pretty(&results).unwrap_or_default()
            }
            "tool_call_dynamic" => {
                let tool_name = tool_call.arguments["tool_name"].as_str().unwrap_or("");
                let arguments = match &tool_call.arguments["arguments"] {
                    serde_json::Value::Object(_) => tool_call.arguments["arguments"].clone(),
                    serde_json::Value::String(s) => {
                        nevoflux_protocol::json_repair::parse_tool_arguments_json(s)
                    }
                    _ => serde_json::json!({}),
                };
                // If LLM routes a built-in tool through tool_call_dynamic,
                // redirect to execute_tool instead of MCP dispatch.
                if tool_name != "tool_call_dynamic" && self.is_builtin_tool(tool_name) {
                    let inner = ToolCall {
                        id: tool_call.id.clone(),
                        call_id: tool_call.call_id.clone(),
                        name: tool_name.to_string(),
                        arguments,
                        signature: None,
                    };
                    return self.execute_tool(&inner);
                }
                self.host.tool_call_dynamic(tool_name, &arguments)?
            }
            // Computer tools
            "computer_screenshot" => {
                let monitor = tool_call.arguments.get("monitor").and_then(|v| v.as_i64());
                self.host.computer_screenshot(monitor)?
            }
            "computer_mouse_move" => {
                let x = tool_call.arguments["x"].as_i64().unwrap_or(0);
                let y = tool_call.arguments["y"].as_i64().unwrap_or(0);
                self.host.computer_mouse_move(x, y)?
            }
            "computer_click" => {
                let x = tool_call.arguments["x"].as_i64().unwrap_or(0);
                let y = tool_call.arguments["y"].as_i64().unwrap_or(0);
                let button = tool_call.arguments.get("button").and_then(|v| v.as_str());
                let click_type = tool_call
                    .arguments
                    .get("click_type")
                    .and_then(|v| v.as_str());
                self.host.computer_click(x, y, button, click_type)?
            }
            "computer_type_text" => {
                let text = tool_call.arguments["text"].as_str().unwrap_or("");
                let delay_ms = tool_call.arguments.get("delay_ms").and_then(|v| v.as_u64());
                self.host.computer_type_text(text, delay_ms)?
            }
            "computer_key" => {
                let key = tool_call.arguments["key"].as_str().unwrap_or("");
                let modifiers = parse_modifiers_arg(&tool_call.arguments);
                let repeat = tool_call.arguments.get("repeat").and_then(|v| v.as_u64());
                self.host.computer_key(key, &modifiers, repeat)?
            }
            "computer_scroll" => {
                let x = tool_call.arguments["x"].as_i64().unwrap_or(0);
                let y = tool_call.arguments["y"].as_i64().unwrap_or(0);
                let direction = tool_call.arguments["direction"].as_str().unwrap_or("down");
                let amount = tool_call.arguments.get("amount").and_then(|v| v.as_u64());
                self.host.computer_scroll(x, y, direction, amount)?
            }
            "computer_drag" => {
                let start_x = tool_call.arguments["start_x"].as_i64().unwrap_or(0);
                let start_y = tool_call.arguments["start_y"].as_i64().unwrap_or(0);
                let end_x = tool_call.arguments["end_x"].as_i64().unwrap_or(0);
                let end_y = tool_call.arguments["end_y"].as_i64().unwrap_or(0);
                let button = tool_call.arguments.get("button").and_then(|v| v.as_str());
                self.host
                    .computer_drag(start_x, start_y, end_x, end_y, button)?
            }
            "computer_cursor_position" => self.host.computer_cursor_position()?,
            "computer_mouse_down" => {
                let x = tool_call.arguments["x"].as_i64().unwrap_or(0);
                let y = tool_call.arguments["y"].as_i64().unwrap_or(0);
                let button = tool_call.arguments.get("button").and_then(|v| v.as_str());
                self.host.computer_mouse_down(x, y, button)?
            }
            "computer_mouse_up" => {
                let x = tool_call.arguments["x"].as_i64().unwrap_or(0);
                let y = tool_call.arguments["y"].as_i64().unwrap_or(0);
                let button = tool_call.arguments.get("button").and_then(|v| v.as_str());
                self.host.computer_mouse_up(x, y, button)?
            }
            "computer_hold_key" => {
                let key = tool_call.arguments["key"].as_str().unwrap_or("");
                let duration_ms = tool_call.arguments["duration_ms"].as_u64().unwrap_or(500);
                let modifiers = parse_modifiers_arg(&tool_call.arguments);
                self.host.computer_hold_key(key, duration_ms, &modifiers)?
            }
            "computer_wait" => {
                let ms = tool_call.arguments["ms"].as_u64().unwrap_or(1000);
                self.host.computer_wait(ms)?
            }
            // Browser tools
            "browser_navigate" => {
                let url = tool_call.arguments["url"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let new_tab = tool_call.arguments["new_tab"].as_bool().unwrap_or(false);
                let result_str = if new_tab {
                    // new_tab mode: use tool_call_dynamic so the new_tab param
                    // reaches background.js via the standard BrowserRequest path.
                    self.host
                        .tool_call_dynamic("browser_navigate", &tool_call.arguments)?
                } else {
                    let result = self.host.browser_navigate(url, tab_id)?;
                    serde_json::to_string(&result).unwrap_or_default()
                };
                self.auto_snapshot_after_action(&result_str, "navigation", tab_id)
            }
            "browser_activate_tab" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result_str = self
                    .host
                    .tool_call_dynamic("browser_activate_tab", &tool_call.arguments)?;
                self.auto_snapshot_after_action(&result_str, "navigation", tab_id)
            }
            "browser_go_back" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_go_back(tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "navigation", tab_id)
            }
            "browser_go_forward" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_go_forward(tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "navigation", tab_id)
            }
            "browser_click" => {
                let selector = tool_call.arguments["selector"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_click(selector, tab_id)?;
                self.click_result_with_fallback(&result, tab_id)
            }
            "browser_click_by_id" => {
                let element_id = tool_call.arguments["element_id"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_click_by_id(element_id, tab_id)?;
                self.click_result_with_fallback(&result, tab_id)
            }
            "browser_type" => {
                let selector = tool_call.arguments["selector"].as_str().unwrap_or("");
                let text = tool_call.arguments["text"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_type(selector, text, tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            "browser_type_by_id" => {
                let element_id = tool_call.arguments["element_id"].as_str().unwrap_or("");
                let text = tool_call.arguments["text"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_type_by_id(element_id, text, tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            "browser_fill" => {
                let selector = tool_call.arguments["selector"].as_str().unwrap_or("");
                let value = tool_call.arguments["value"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_fill(selector, value, tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            "browser_fill_by_id" => {
                let element_id = tool_call.arguments["element_id"].as_str().unwrap_or("");
                let value = tool_call.arguments["value"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_fill_by_id(element_id, value, tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            // PR #2 / #2.5: browser input strategy engine tools.
            //
            // These dispatch through tool_call_dynamic (generic host function)
            // instead of a typed host method. The daemon's mcp_tool_executor
            // intercepts Input/Probe actions and runs the orchestration pipeline
            // (probe → decide → execute → verify) via run_browser_input.
            //
            // browser_input is an interaction tool → auto_snapshot is desirable.
            // browser_probe is read-only → no auto_snapshot needed.
            "browser_input" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result_str = self
                    .host
                    .tool_call_dynamic("browser_input", &tool_call.arguments)?;
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            "browser_probe" => self
                .host
                .tool_call_dynamic("browser_probe", &tool_call.arguments)?,
            "browser_upload_file" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result_str = self
                    .host
                    .tool_call_dynamic("browser_upload_file", &tool_call.arguments)?;
                self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
            }
            "browser_get_content" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_get_content(tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            "browser_get_markdown" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_get_markdown(tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            "browser_screenshot" => {
                let full_page = tool_call.arguments["full_page"].as_bool().unwrap_or(false);
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_screenshot(full_page, tab_id)?;
                if let Some(base64_data) = &result.screenshot {
                    // Cache the screenshot for attachment (stripped from content)
                    *self.screenshot_cache.borrow_mut() = Some(base64_data.clone());
                    r#"{"success":true,"screenshot_available":true}"#.to_string()
                } else {
                    serde_json::to_string(&result).unwrap_or_default()
                }
            }
            "browser_eval_js" => {
                let script = tool_call.arguments["script"].as_str().unwrap_or("");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_eval_js(script, tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            "browser_scroll" => {
                let direction = tool_call.arguments["direction"].as_str().unwrap_or("down");
                let amount = tool_call.arguments["amount"].as_str().unwrap_or("page");
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_scroll(direction, amount, tab_id)?;
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                self.auto_snapshot_after_action(&result_str, "scroll", tab_id)
            }
            "browser_wait_for" => {
                let selector = tool_call.arguments["selector"].as_str().unwrap_or("");
                let timeout_ms = tool_call.arguments["timeout_ms"].as_u64().unwrap_or(10000);
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let result = self.host.browser_wait_for(selector, timeout_ms, tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            "browser_get_elements" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let keywords: Option<Vec<String>> = tool_call
                    .arguments
                    .get("keywords")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    });
                let result = self.host.browser_get_elements(tab_id, keywords)?;
                // Parse and cache elements, return compact summary
                if let Some(data) = &result.data {
                    if let Some(cache) = parse_elements_from_data(data) {
                        let summary = build_elements_summary(&cache.elements);
                        *self.elements_cache.borrow_mut() = Some(cache);
                        summary
                    } else {
                        // Parse failed — fallback to original behavior
                        serde_json::to_string(&result).unwrap_or_default()
                    }
                } else {
                    serde_json::to_string(&result).unwrap_or_default()
                }
            }
            "browser_find_elements" => {
                let cache = self.elements_cache.borrow();
                match cache.as_ref() {
                    None => r#"{"success":false,"error":"No elements cached. Call browser_get_elements first."}"#.to_string(),
                    Some(cache) => {
                        let role_filter = tool_call.arguments.get("role").and_then(|v| v.as_str());
                        let name_filter = tool_call.arguments.get("name").and_then(|v| v.as_str());
                        let selector_filter = tool_call.arguments.get("selector").and_then(|v| v.as_str());
                        let near_x = tool_call.arguments.get("near_x").and_then(|v| v.as_f64());
                        let near_y = tool_call.arguments.get("near_y").and_then(|v| v.as_f64());
                        let radius = tool_call.arguments.get("radius").and_then(|v| v.as_f64()).unwrap_or(50.0);
                        let unnamed_only = tool_call.arguments.get("unnamed_only").and_then(|v| v.as_bool()).unwrap_or(false);
                        let limit = tool_call.arguments.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

                        let results: Vec<serde_json::Value> = cache
                            .elements
                            .iter()
                            .filter(|e| {
                                if let Some(role) = role_filter {
                                    if !e.role.eq_ignore_ascii_case(role) {
                                        return false;
                                    }
                                }
                                if let Some(name) = name_filter {
                                    if !e.name.to_lowercase().contains(&name.to_lowercase()) {
                                        return false;
                                    }
                                }
                                if let Some(sel) = selector_filter {
                                    if !e.selector.contains(sel) {
                                        return false;
                                    }
                                }
                                if unnamed_only && !e.name.is_empty() {
                                    return false;
                                }
                                if let (Some(nx), Some(ny)) = (near_x, near_y) {
                                    if let Some(ref rect) = e.rect {
                                        let cx = rect.x + rect.width / 2.0;
                                        let cy = rect.y + rect.height / 2.0;
                                        let dist = ((cx - nx).powi(2) + (cy - ny).powi(2)).sqrt();
                                        if dist > radius {
                                            return false;
                                        }
                                    } else {
                                        return false;
                                    }
                                }
                                true
                            })
                            .take(limit)
                            .map(|e| {
                                let mut obj = serde_json::json!({
                                    "id": e.id,
                                    "role": e.role,
                                    "name": e.name,
                                    "selector": e.selector,
                                });
                                if let Some(ref rect) = e.rect {
                                    obj["rect"] = serde_json::json!({
                                        "x": rect.x as i64,
                                        "y": rect.y as i64,
                                        "width": rect.width as i64,
                                        "height": rect.height as i64,
                                    });
                                }
                                obj
                            })
                            .collect();

                        serde_json::json!({
                            "success": true,
                            "count": results.len(),
                            "elements": results,
                        })
                        .to_string()
                    }
                }
            }
            "browser_element_info" => {
                let id = tool_call
                    .arguments
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let cache = self.elements_cache.borrow();
                match cache.as_ref() {
                    None => r#"{"success":false,"error":"No elements cached. Call browser_get_elements first."}"#.to_string(),
                    Some(cache) => {
                        match cache.elements.iter().find(|e| e.id == id) {
                            Some(el) => serde_json::json!({
                                "success": true,
                                "element": el.raw,
                            })
                            .to_string(),
                            None => format!(r#"{{"success":false,"error":"Element '{}' not found"}}"#, id),
                        }
                    }
                }
            }
            // Meta-tool: load computer use tools and trigger full prompt injection
            "load_computer_use_tools" => {
                self.computer_use_triggered.set(true);
                r#"{"success":true,"message":"Computer use tools are now available. On the next turn, you will receive the full computer use guide. Available tools: computer_screenshot, computer_mouse_move, computer_click, computer_type_text, computer_key, computer_scroll, computer_drag, computer_cursor_position, computer_mouse_down, computer_mouse_up, computer_hold_key, computer_wait."}"#.to_string()
            }
            // Artifact editing tools
            "browser_read_artifact" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let params = serde_json::json!({
                    "id": tool_call.arguments.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "offset": tool_call.arguments.get("offset"),
                    "limit": tool_call.arguments.get("limit"),
                    "grep": tool_call.arguments.get("grep"),
                    "context": tool_call.arguments.get("context"),
                });
                let result = self.host.browser_read_artifact(&params, tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            "browser_edit_artifact" => {
                let tab_id = tool_call.arguments["tab_id"].as_i64();
                let params = serde_json::json!({
                    "id": tool_call.arguments.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "old_str": tool_call.arguments.get("old_str").and_then(|v| v.as_str()).unwrap_or(""),
                    "new_str": tool_call.arguments.get("new_str").and_then(|v| v.as_str()).unwrap_or(""),
                });
                let result = self.host.browser_edit_artifact(&params, tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            // Subagent tools
            "subagent_spawn" => {
                // If args contain role-system fields, use new JSON config path
                if tool_call.arguments.get("role").is_some()
                    || tool_call.arguments.get("tools").is_some()
                    || tool_call.arguments.get("provider").is_some()
                    || tool_call.arguments.get("system_prompt").is_some()
                {
                    // New path: serialize entire args as SpawnSubagentConfig JSON
                    // Rename "task" -> "prompt" for SpawnSubagentConfig compatibility
                    let mut config_obj = tool_call.arguments.clone();
                    // Parse "tools" from string to JSON value if needed (OpenAI strict mode sends strings)
                    if let Some(tools_val) = config_obj.get("tools").cloned() {
                        if let Some(s) = tools_val.as_str() {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                                config_obj
                                    .as_object_mut()
                                    .map(|m| m.insert("tools".to_string(), parsed));
                            }
                        }
                    }
                    if let Some(task_val) = config_obj.get("task").cloned() {
                        config_obj
                            .as_object_mut()
                            .map(|m| m.insert("prompt".to_string(), task_val));
                        config_obj.as_object_mut().map(|m| m.remove("task"));
                    }
                    let config_json = serde_json::to_string(&config_obj).unwrap_or_default();
                    let id = self.host.subagent_spawn(&config_json, "agent", None)?;
                    format!("Spawned sub-agent with ID: {}", id)
                } else {
                    // Legacy path: extract task, mode, tab_id as before
                    let task = tool_call.arguments["task"].as_str().unwrap_or("");
                    let mode = tool_call.arguments["mode"].as_str().unwrap_or("agent");
                    let tab_id = tool_call.arguments["tab_id"].as_i64();
                    let id = self.host.subagent_spawn(task, mode, tab_id)?;
                    format!("Spawned sub-agent with ID: {}", id)
                }
            }
            "subagent_wait_all" => {
                let ids: Vec<u64> = tool_call.arguments["ids"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();
                self.host.subagent_wait_all(&ids)?
            }
            "subagent_status" => {
                let id = tool_call.arguments["id"].as_u64().unwrap_or(0);
                let status = self.host.subagent_status(id)?;
                format!("Sub-agent {} status: {}", id, status)
            }
            "subagent_wait" => {
                let id = tool_call.arguments["id"].as_u64().unwrap_or(0);
                self.host.subagent_wait(id)?
            }
            "subagent_kill" => {
                let id = tool_call.arguments["id"].as_u64().unwrap_or(0);
                let killed = self.host.subagent_kill(id)?;
                if killed {
                    format!("Sub-agent {} was terminated", id)
                } else {
                    format!("Sub-agent {} had already completed", id)
                }
            }
            "subagent_list" => {
                let list = self.host.subagent_list()?;
                serde_json::to_string_pretty(&list).unwrap_or_default()
            }
            "list_agents" => self.host.list_agents()?,
            // orchestrate: execute a Python script in sandboxed Monty interpreter
            // to orchestrate multiple tool calls in a single script.
            "orchestrate" => {
                match self
                    .host
                    .tool_call_dynamic("orchestrate", &tool_call.arguments)
                {
                    Ok(output) => output,
                    Err(e) => format!("orchestrate error: {}", e.message),
                }
            }
            "canvas_create_composition" => {
                let resp = self
                    .host
                    .canvas_video_create_composition(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_render_video" => {
                let resp = self.host.canvas_video_render_start(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_lint_composition" => {
                let resp = self
                    .host
                    .canvas_video_lint_composition(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_apply_design_md" => {
                let resp = self
                    .host
                    .canvas_video_apply_design_md(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_create_from_visual_identity" => {
                let resp = self
                    .host
                    .canvas_video_create_from_visual_identity(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_attach_asset" => {
                let resp = self.host.canvas_video_attach_asset(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_inspect_layout" => {
                let resp = self
                    .host
                    .canvas_video_inspect_layout(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "tts_synthesize_api" => {
                let resp = self.host.tts_synthesize_api(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "tts_synthesize_local" => {
                let resp = self.host.tts_synthesize_local(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "tts_transcribe" => {
                let resp = self.host.tts_transcribe(&tool_call.arguments)?;
                serde_json::to_string(&resp)
                    .unwrap_or_else(|e| format!(r#"{{"error":"serialize failed: {}"}}"#, e))
            }
            "canvas_extract_visual_identity" => {
                // Mode-3 entry point: route through the standard browser-tool
                // dispatch (BrowserToolAction::ExtractVisualIdentity → daemon
                // BrowserRequest → extension extraction handler → BrowserToolResult).
                // Intentionally goes through `host.browser_extract_visual_identity`
                // so the direct-API path matches mcp_tool_executor's MCP/ACP path —
                // both ultimately produce a BrowserRequest with action=ExtractVisualIdentity.
                let tab_id = tool_call
                    .arguments
                    .get("target")
                    .and_then(|t| t.get("tab_id"))
                    .and_then(|v| v.as_i64());
                let result = self
                    .host
                    .browser_extract_visual_identity(&tool_call.arguments, tab_id)?;
                serde_json::to_string(&result).unwrap_or_default()
            }
            // /loop skill tools — direct-API dispatch (Anthropic / OpenAI direct
            // providers). The MCP/ACP path goes through
            // `mcp_tool_executor::execute_mcp_tool::loop.*`.
            "loop.create" => self
                .host
                .tool_loop_create(&serde_json::to_string(&tool_call.arguments).unwrap_or_default())?,
            "loop.list" => self.host.tool_loop_list()?,
            "loop.cancel" => {
                let loop_id = tool_call.arguments["loop_id"].as_str().unwrap_or("");
                self.host.tool_loop_cancel(loop_id)?
            }
            "loop.scratchpad.get" => self.host.tool_loop_scratchpad_get(
                &serde_json::to_string(&tool_call.arguments).unwrap_or_default(),
            )?,
            "loop.scratchpad.set" => self.host.tool_loop_scratchpad_set(
                &serde_json::to_string(&tool_call.arguments).unwrap_or_default(),
            )?,
            _ => {
                format!("Unknown tool: {}", tool_call.name)
            }
        };

        // Auto-trigger computer use if a browser tool result suggests native interaction needed
        if !self.computer_use_triggered.get()
            && tool_call.name.starts_with("browser_")
            && is_browser_failure_suggesting_computer_use(&content)
        {
            self.computer_use_triggered.set(true);
        }

        // Use call_id if available, otherwise fall back to id
        // OpenAI Responses API requires call_id to match tool results
        let tool_call_id = tool_call
            .call_id
            .clone()
            .unwrap_or_else(|| tool_call.id.clone());
        Ok(ToolResult {
            tool_call_id,
            content,
            success: true,
        })
    }

    /// Check if a tool name is handled by execute_tool (not an MCP tool).
    fn is_builtin_tool(&self, name: &str) -> bool {
        let name = Self::normalize_tool_name(name);
        matches!(
            name,
            "think"
                | "plan"
                | "create_artifact"
                | "switch_model"
                | "web_search"
                | "web_fetch"
                | "ask_user"
                | "read"
                | "write"
                | "edit"
                | "bash"
                | "glob"
                | "grep"
                | "memory_search"
                | "memory_create"
                | "memory_update"
                | "memory_delete"
                | "skill_load"
                | "tool_search"
                | "tool_call_dynamic"
                | "orchestrate"
                | "load_computer_use_tools"
                | "subagent_spawn"
                | "subagent_wait_all"
                | "subagent_status"
                | "subagent_wait"
                | "subagent_kill"
                | "subagent_list"
                | "list_agents"
                | "canvas_create_composition"
                | "canvas_render_video"
                | "canvas_lint_composition"
                | "canvas_apply_design_md"
                | "canvas_extract_visual_identity"
                | "canvas_create_from_visual_identity"
                | "canvas_attach_asset"
                | "canvas_inspect_layout"
                | "tts_synthesize_api"
                | "tts_synthesize_local"
                | "tts_transcribe"
                | "loop.create"
                | "loop.list"
                | "loop.cancel"
                | "loop.scratchpad.get"
                | "loop.scratchpad.set"
        ) || name.starts_with("computer_")
            || name.starts_with("browser_")
    }

    /// Attempt a native OS-level click fallback when all JS click tiers failed.
    ///
    /// Returns a replacement JSON result string if the fallback fired successfully,
    /// or `None` to let the original result flow through.
    fn try_computer_click_fallback(&self, result: &BrowserToolResult) -> Option<String> {
        let data = result.data.as_ref()?;

        // Only fire when every programmatic click tier was attempted and none was effective
        let click_method = data.get("clickMethod").and_then(|v| v.as_str())?;
        if click_method != "all_tiers_exhausted" {
            return None;
        }
        let effective = data
            .get("effective")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if effective {
            return None;
        }

        // Extension must provide screen-absolute bounds for the element
        let bounds: ScreenBounds =
            serde_json::from_value(data.get("screenBounds")?.clone()).ok()?;
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return None;
        }

        let cx = (bounds.x + bounds.width / 2.0).round() as i64;
        let cy = (bounds.y + bounds.height / 2.0).round() as i64;

        match self.host.computer_click(cx, cy, None, None) {
            Ok(click_result) => {
                self.computer_use_triggered.set(true);
                let click_value = serde_json::from_str::<serde_json::Value>(&click_result)
                    .unwrap_or(serde_json::Value::String(click_result));
                Some(
                    serde_json::json!({
                        "clickMethod": "computer_click_fallback",
                        "originalMethod": "all_tiers_exhausted",
                        "fallbackCoordinates": { "x": cx, "y": cy },
                        "computerClickResult": click_value,
                    })
                    .to_string(),
                )
            }
            Err(_) => None,
        }
    }

    /// Serialize a browser click result, applying the computer_click fallback if needed.
    fn click_result_with_fallback(
        &self,
        result: &BrowserToolResult,
        tab_id: Option<i64>,
    ) -> String {
        let result_str = if let Some(fallback) = self.try_computer_click_fallback(result) {
            fallback
        } else {
            serde_json::to_string(result).unwrap_or_default()
        };
        self.auto_snapshot_after_action(&result_str, "interaction", tab_id)
    }

    /// Execute wait-for-stable + viewport snapshot and append to action result.
    fn auto_snapshot_after_action(
        &self,
        action_result: &str,
        wait_strategy: &str,
        tab_id: Option<i64>,
    ) -> String {
        // 1. Wait for page stability (ignore errors — best effort)
        let _ = self
            .host
            .browser_wait_for_stable(wait_strategy, 3000, tab_id);

        // 2. Take viewport snapshot with current keywords
        let kws = self.current_keywords.borrow().clone();
        let kw_arg = if kws.is_empty() { None } else { Some(kws) };
        let snapshot_text = self.get_viewport_snapshot_text(tab_id, kw_arg);

        if snapshot_text.is_empty() {
            action_result.to_string()
        } else {
            format!(
                "{}\n\nCurrent page state:\n{}",
                action_result, snapshot_text
            )
        }
    }

    /// Extract keywords from text using rule-based heuristics.
    ///
    /// Extracts: CJK sequences, quoted strings, capitalized words, known action verbs.
    /// Deduplicates and limits to 10 keywords.
    fn extract_keywords_from_text(text: &str) -> Vec<String> {
        let mut keywords = Vec::new();

        // CJK stop words to filter out (2+ chars only — single-char sequences
        // are already excluded by the >= 2 character threshold below)
        const CJK_STOPS: &[&str] = &[
            "一个", "没有", "自己", "这个", "那个", "什么", "怎么", "可以", "已经", "因为", "所以",
            "但是", "如果", "虽然", "还是", "就是",
        ];

        // Known action verbs (Chinese + English)
        const ACTION_VERBS: &[&str] = &[
            "登录", "login", "sign in", "signin", "search", "click", "submit", "注册", "搜索",
            "点击", "提交", "打开", "关闭", "输入", "选择", "register", "signup", "sign up",
            "open", "close", "enter", "select",
        ];

        // 1. Extract quoted strings
        let mut in_quote = false;
        let mut quote_char = ' ';
        let mut current_quoted = String::new();
        for ch in text.chars() {
            if !in_quote && (ch == '"' || ch == '\'' || ch == '\u{201C}' || ch == '\u{300C}') {
                in_quote = true;
                quote_char = ch;
                current_quoted.clear();
            } else if in_quote
                && ((quote_char == '"' && ch == '"')
                    || (quote_char == '\'' && ch == '\'')
                    || (quote_char == '\u{201C}' && ch == '\u{201D}')
                    || (quote_char == '\u{300C}' && ch == '\u{300D}'))
            {
                in_quote = false;
                let trimmed = current_quoted.trim().to_string();
                if !trimmed.is_empty() && trimmed.len() <= 50 {
                    keywords.push(trimmed);
                }
            } else if in_quote {
                current_quoted.push(ch);
            }
        }

        // 2. Extract CJK sequences (2+ consecutive CJK chars, minus stop words)
        let mut cjk_buf = String::new();
        for ch in text.chars() {
            if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
                || ('\u{3400}'..='\u{4DBF}').contains(&ch)
                || ('\u{F900}'..='\u{FAFF}').contains(&ch)
            {
                cjk_buf.push(ch);
            } else {
                if cjk_buf.chars().count() >= 2 && !CJK_STOPS.contains(&cjk_buf.as_str()) {
                    keywords.push(cjk_buf.clone());
                }
                cjk_buf.clear();
            }
        }
        if cjk_buf.chars().count() >= 2 && !CJK_STOPS.contains(&cjk_buf.as_str()) {
            keywords.push(cjk_buf);
        }

        // 3. Extract capitalized words / proper nouns (2+ chars starting with uppercase)
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
            let trimmed = word.trim();
            if trimmed.len() >= 2
                && trimmed
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_ascii_uppercase())
                && ![
                    "The", "This", "That", "What", "When", "Where", "How", "Why", "And", "But",
                    "For", "Not", "You", "Are", "Was", "Were", "Can", "Could", "Will", "Would",
                    "Should", "May", "Might", "Has", "Have", "Had", "Does", "Did", "Its", "All",
                ]
                .contains(&trimmed)
            {
                keywords.push(trimmed.to_string());
            }
        }

        // 4. Extract known action verbs found in the text
        let lower = text.to_lowercase();
        for verb in ACTION_VERBS {
            if lower.contains(verb) {
                keywords.push(verb.to_string());
            }
        }

        // Deduplicate (case-insensitive) and limit to 10
        let mut seen = std::collections::HashSet::new();
        keywords.retain(|kw| {
            let key = kw.to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });
        keywords.truncate(10);
        keywords
    }

    /// Update stored keywords from pre-extracted LLM keywords and tool call arguments.
    fn update_keywords_from_tool_context(&self, llm_kws: &[String], tool_call: &ToolCall) {
        let mut kws = self.current_keywords.borrow_mut();

        // Merge pre-extracted LLM reasoning keywords
        for kw in llm_kws {
            if !kws.iter().any(|existing| existing.eq_ignore_ascii_case(kw)) {
                kws.push(kw.clone());
            }
        }

        // Extract from tool call arguments (element_id, text, value, selector, query, url)
        let args = &tool_call.arguments;
        for key in &["text", "value", "selector", "query", "url", "element_id"] {
            if let Some(val) = args.get(key).and_then(|v| v.as_str()) {
                let arg_kws = Self::extract_keywords_from_text(val);
                for kw in arg_kws {
                    if !kws
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(&kw))
                    {
                        kws.push(kw);
                    }
                }
            }
        }

        // Keep latest 10
        if kws.len() > 10 {
            let excess = kws.len() - 10;
            kws.drain(..excess);
        }
    }

    /// Get viewport snapshot text for initial context.
    fn get_viewport_snapshot_text(
        &self,
        tab_id: Option<i64>,
        keywords: Option<Vec<String>>,
    ) -> String {
        match self.host.browser_viewport_snapshot(tab_id, keywords) {
            Ok(result) => {
                if let Some(data) = &result.data {
                    if let Some(result_obj) = data.get("result") {
                        result_obj
                            .get("tree")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string()
                    } else if let Some(tree) = data.get("tree") {
                        tree.as_str().unwrap_or("").to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
            Err(_) => String::new(),
        }
    }

    /// Format tab context as pure data for injection into user messages.
    fn format_tab_context(active_tab: Option<&TabInfo>, extra_tabs: &[TabInfo]) -> String {
        if extra_tabs.is_empty() && active_tab.is_none() {
            return String::new();
        }

        let mut ctx = String::from("\n\n## Active Tabs\n");

        if !extra_tabs.is_empty() {
            // When the user attaches specific tabs, those are the PRIMARY targets
            // of their request (e.g. "summarize this page" refers to the attached
            // tabs, not necessarily the sidebar's current_tab).
            ctx.push_str("IMPORTANT: The user has explicitly attached the tabs listed below. Unless the user explicitly asks to read the \"current tab\", you should use browser_get_markdown with the attached tab's tab_id to read their content. current_tab only indicates which tab the sidebar panel is open on.\n\n");
            let mut by_space: std::collections::BTreeMap<&str, Vec<&TabInfo>> =
                std::collections::BTreeMap::new();
            for tab in extra_tabs {
                let space = if tab.space.is_empty() {
                    "Default"
                } else {
                    &tab.space
                };
                by_space.entry(space).or_default().push(tab);
            }
            for (space, tabs) in &by_space {
                ctx.push_str(&format!("[{}]\n", space));
                for tab in tabs {
                    ctx.push_str(&format!(
                        "- {}: \"{}\" | {}\n",
                        tab.tab_id, tab.tab_title, tab.url
                    ));
                }
            }
            if let Some(tab) = active_tab {
                ctx.push_str(&format!("current_tab: {}\n", tab.tab_id));
            }
        } else if let Some(tab) = active_tab {
            ctx.push_str(&format!(
                "current_tab: {} | \"{}\" | {}\n",
                tab.tab_id, tab.tab_title, tab.url
            ));
        }

        ctx
    }

    /// Get available tools for chat mode.
    fn get_chat_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "think".into(),
                description: "Use this tool to think through problems step by step before acting. Analyze the situation, reason about the best approach, reflect on results, or plan your next move. This tool has no side effects - it simply records your thought process.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "thought": {
                            "type": "string",
                            "description": "Your thought process, reasoning, or analysis"
                        }
                    },
                    "required": ["thought"]
                }),
            },
            ToolDefinition {
                name: "plan".into(),
                description: "Create an execution plan for the user to review and approve. The plan will be displayed in the sidebar. The user can provide feedback via chat to request changes, and you should revise and call plan() again. Use this for multi-step tasks.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "Brief overview of what the plan accomplishes"
                        },
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "description": {
                                        "type": "string",
                                        "description": "What this step does"
                                    },
                                    "model": {
                                        "type": "string",
                                        "description": "Optional: suggested model for this step"
                                    }
                                },
                                "required": ["description"]
                            },
                            "description": "Ordered list of steps"
                        }
                    },
                    "required": ["summary", "steps"]
                }),
            },
            ToolDefinition {
                name: "create_artifact".into(),
                description: "Create a rich artifact that opens in the browser canvas. For single-file content (HTML page, document), provide 'content'. For multi-file projects (React/Vue/Svelte apps), provide 'files' and 'entry' with content_type 'project'. The artifact opens in a dedicated canvas tab.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Human-readable title for the artifact"
                        },
                        "content_type": {
                            "type": "string",
                            "enum": ["text/html", "text/markdown", "text/plain", "application/json", "text/css", "text/javascript", "project"],
                            "description": "MIME type of the content. Use 'project' for multi-file React/Vue/Svelte apps."
                        },
                        "description": {
                            "type": "string",
                            "description": "Brief description of what this artifact contains (1-2 sentences)"
                        },
                        "content": {
                            "type": "string",
                            "description": "The full artifact content (for single-file artifacts: HTML, Markdown, code). Optional when using 'files'."
                        },
                        "files": {
                            "type": "string",
                            "description": "Multi-file project: a JSON-encoded map of file paths to file contents. Example: \"{\\\"src/App.jsx\\\": \\\"export default function App() {...}\\\", \\\"src/index.jsx\\\": \\\"import App from './App'; ...\\\"}\". Use with content_type 'project'."
                        },
                        "entry": {
                            "type": "string",
                            "description": "Entry point file path for multi-file projects (e.g. 'src/index.jsx'). Required when 'files' is provided."
                        }
                    },
                    "required": ["title"]
                }),
            },
            ToolDefinition {
                name: "switch_model".into(),
                description: "Switch the active LLM provider and model for subsequent operations. Use this when executing plan steps that specify a different model.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "provider": {
                            "type": "string",
                            "description": "Provider name (e.g., 'anthropic', 'openai', 'deepseek')"
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name (e.g., 'claude-sonnet-4-20250514', 'gpt-4o')"
                        }
                    },
                    "required": ["provider", "model"]
                }),
            },
            ToolDefinition {
                name: "web_search".into(),
                description: "Search the web for information. Use for time-sensitive or unknown topics.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "web_fetch".into(),
                description: "Fetch full content of a specific URL and analyze it with a prompt.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "What to extract from the page"
                        }
                    },
                    "required": ["url", "prompt"]
                }),
            },
            ToolDefinition {
                name: "ask_user".into(),
                description: "Ask the user a question".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to ask"
                        },
                        "options": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Optional choices for the user"
                        }
                    },
                    "required": ["question"]
                }),
            },
            ToolDefinition {
                name: "memory_search".into(),
                description: "Search your memory for relevant information".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What to search for"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results to return"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "memory_create".into(),
                description: "Save information to memory for future conversations. Stored knowledge is automatically available in all subsequent conversations. Use for: user preferences, important facts, behavioral rules, site-specific knowledge, or anything worth remembering long-term.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The information to remember"
                        },
                        "category": {
                            "type": "string",
                            "enum": ["user_preference", "site_interaction", "tool_optimization"],
                            "description": "Knowledge category (default: user_preference)"
                        },
                        "domain": {
                            "type": "string",
                            "description": "Associated domain if applicable (e.g., github.com)"
                        },
                        "metadata": {
                            "type": "string",
                            "description": "Optional JSON metadata"
                        }
                    },
                    "required": ["content"]
                }),
            },
            ToolDefinition {
                name: "memory_update".into(),
                description: "Update an existing memory chunk with new content. Use the id from memory_search results.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The memory chunk ID to update"
                        },
                        "content": {
                            "type": "string",
                            "description": "The new content to replace the existing content"
                        }
                    },
                    "required": ["id", "content"]
                }),
            },
            ToolDefinition {
                name: "memory_delete".into(),
                description: "Delete a memory chunk by ID. Use the id from memory_search results.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The memory chunk ID to delete"
                        }
                    },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "memory_view".into(),
                description: "View all saved memories. Returns a list of all active knowledge entries that are automatically included in conversations.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum entries to return (default 20)"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "skill_load".into(),
                description: "Load a skill's full instructions by name. MUST be called before responding when the user's request matches any skill listed in the system prompt. The loaded content becomes your primary instructions for the task.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The skill name (from the Skills section in system prompt)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            // Browser content tools (also available in chat mode for tab context)
            ToolDefinition {
                name: "browser_get_content".into(),
                description: "Get full raw HTML of the page. High token cost. Use ONLY when user needs actual HTML/CSS code structure (e.g., 'build a page like this'). For content reading, use browser_get_markdown instead.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tab_id": {
                            "type": "integer",
                            "description": "Optional tab ID (uses active tab if not specified)"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "browser_get_markdown".into(),
                description: "Get page content as clean markdown. Low token cost. DEFAULT tool for reading pages. Use for: summarize, translate, analyze, extract info, Q&A. Prefer over browser_get_content unless user needs actual HTML/CSS structure.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tab_id": {
                            "type": "integer",
                            "description": "Optional tab ID (uses active tab if not specified)"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "browser_screenshot".into(),
                description: "Capture the current tab's visible viewport as an image. Use for: visual layout questions, design analysis, canvas/image content, or when get_markdown returns empty. Do NOT use for reading text content.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "full_page": {
                            "type": "boolean",
                            "description": "Whether to capture the full page (default: false)",
                            "default": false
                        },
                        "tab_id": {
                            "type": "integer",
                            "description": "Optional tab ID (uses active tab if not specified)"
                        }
                    }
                }),
            },
            // Artifact editing tools (available in all modes)
            ToolDefinition {
                name: "browser_read_artifact".into(),
                description: "Read the source code of a canvas artifact. Returns full content by default (with line numbers). Use offset/limit for large artifacts, or grep to search for specific code sections. For multi-file artifacts (e.g. canvas_video compositions: index.html + DESIGN.md + composition.meta.json), pass `path` to read a non-entry file; omit `path` to read the entry file (typically index.html). Only use when [Active Canvas] hint is present.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Artifact ID (from the [Active Canvas] context hint)"
                        },
                        "path": {
                            "type": "string",
                            "description": "(Multi-file artifacts only) File path within the artifact, e.g. 'DESIGN.md' or 'composition.meta.json'. Defaults to the entry file (typically 'index.html') when omitted."
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Start from line N (1-based)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Read N lines"
                        },
                        "grep": {
                            "type": "string",
                            "description": "Search keyword to find specific code sections"
                        },
                        "context": {
                            "type": "integer",
                            "description": "Lines of context around grep matches (default 5)"
                        }
                    },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "browser_edit_artifact".into(),
                description: "Edit a canvas artifact using search-and-replace. The old_str must match exactly one location in the targeted file. Include surrounding lines for uniqueness. The canvas updates in real-time after each edit. For multi-file artifacts (e.g. canvas_video compositions), pass `path` to edit a non-entry file (e.g. 'DESIGN.md'); omit `path` to edit the entry file (typically index.html). Only use when [Active Canvas] hint is present.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Artifact ID (from the [Active Canvas] context hint)"
                        },
                        "path": {
                            "type": "string",
                            "description": "(Multi-file artifacts only) File path within the artifact to edit, e.g. 'DESIGN.md'. Defaults to the entry file (typically 'index.html') when omitted."
                        },
                        "old_str": {
                            "type": "string",
                            "description": "Exact string to find in the targeted file"
                        },
                        "new_str": {
                            "type": "string",
                            "description": "Replacement string"
                        }
                    },
                    "required": ["id", "old_str", "new_str"]
                }),
            },
            ToolDefinition {
                name: "canvas_create_composition".into(),
                description: "Create a composition artifact for video rendering. Returns \
                              {artifact_id} immediately. Stores a multi-file artifact \
                              (index.html + DESIGN.md + composition.meta.json).\n\n\
                              **TEMPLATE-VS-CUSTOM DECISION (read first):**\n\
                              The seven shipped templates are PRE-BAKED scenes for specific \
                              creative briefs. Pick `template` ONLY when the user's request \
                              cleanly matches one — e.g. \"3-second TikTok hook\" → \
                              tiktok-hook; \"product feature reel\" → product-intro-*; \
                              \"website screenshot promo\" → website-promo-16x9; \"3D logo \
                              reveal\" → logo-3d-reveal.\n\
                              When the user says \"make a video of <THIS IMAGE>\" / \"animate \
                              this picture\" / \"video about X\" without naming a template \
                              vibe, USE `html` (custom layout) — DO NOT default to a template. \
                              Templates have their own opinionated copy slots, scene timing \
                              and decoratives that will fight the user's actual content. \
                              Defaulting to a template is a known failure mode; the resulting \
                              video looks like the template, not what the user asked for.\n\n\
                              **IMAGE INPUT — call canvas_attach_asset FIRST:**\n\
                              If the user provided an image (uploaded / URL / clipboard) that \
                              should appear in the video, you MUST call canvas_attach_asset \
                              BEFORE referencing it. The asset goes into the composition's \
                              files map at `assets/<name>` and the renderer auto-inlines it. \
                              Putting `<img src=\"https://...\">` directly in HTML or pasting \
                              base64 inline both fail.\n\n\
                              THREE LAYERS — separate concerns, edit independently:\n\
                              1. STRUCTURE (`template` OR `html`): layout, scene rhythm, GSAP \
                                 timeline. Pick at create time. Shipped templates: \
                                 website-promo-16x9, product-intro-16x9, product-intro-9x16, \
                                 tiktok-hook, video-overlay, logo-3d-reveal, product-3d-spin. \
                                 When both are supplied, `html` wins.\n\
                              2. BRAND (`design_md`): colors, typography, spacing, motion \
                                 easings. Optional YAML+markdown following Google design.md \
                                 spec + NevoFlux video extension. Daemon parses the YAML \
                                 frontmatter and injects a `<style data-nf-design-tokens>` \
                                 block at the top of the composition's <head>.\n\
                              3. CONTENT (post-create): use browser_edit_artifact on the \
                                 returned artifact_id to replace text placeholders, edit \
                                 copy, and add `<img src=\"assets/...\">` references after \
                                 canvas_attach_asset. After editing DESIGN.md separately, \
                                 call canvas_apply_design_md to refresh the brand layer.\n\n\
                              Arguments: title (str); width (int 1-1920); height (int 1-1920); \
                              duration_sec (number 0.5-60); fps (24|25|30); bg (optional CSS \
                              color string); plus exactly one of `template` or `html`; plus \
                              optional `design_md` (string).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title":        { "type": "string" },
                        "width":        { "type": "integer", "minimum": 1, "maximum": 1920 },
                        "height":       { "type": "integer", "minimum": 1, "maximum": 1920 },
                        "duration_sec": { "type": "number",  "minimum": 0.5, "maximum": 60 },
                        "fps":          { "type": "integer", "enum": [24, 25, 30] },
                        "bg":           { "type": ["string", "null"] },
                        "template":     {
                            "type": "string",
                            "enum": [
                                "website-promo-16x9",
                                "product-intro-16x9",
                                "product-intro-9x16",
                                "tiktok-hook",
                                "video-overlay",
                                "logo-3d-reveal",
                                "product-3d-spin"
                            ],
                            "description": "Skill template name. ONLY pick when the user's \
                                            request matches a template's specific creative \
                                            brief (e.g. they say 'TikTok hook' or 'product \
                                            intro'). For generic / image-driven requests \
                                            ('make a video of this picture'), use `html` \
                                            instead — defaulting to a template makes the \
                                            output look like the template, not the user's \
                                            content."
                        },
                        "html": {
                            "type": "string",
                            "description": "Raw composition HTML body. Use this when the user \
                                            didn't request a specific template style, when \
                                            their main content is a user-supplied image / \
                                            text, or when no shipped template's creative brief \
                                            fits. Overrides `template` when both are supplied."
                        },
                        "design_md": {
                            "type": "string",
                            "description": "Brand identity (Google design.md + NevoFlux video \
                                            extension). YAML frontmatter must include colors \
                                            (primary/secondary/accent/background/foreground), \
                                            typography (hero/body), spacing. The daemon parses \
                                            and injects --color-* / --typography-* / --spacing-* \
                                            CSS variables into the composition's <head>. Omit \
                                            to use the template's own default brand defaults."
                        }
                    },
                    "required": ["title", "width", "height", "duration_sec", "fps"]
                }),
            },
            ToolDefinition {
                name: "canvas_render_video".into(),
                description: "Kick off a non-blocking video render for the given composition. Returns {job_id} IMMEDIATELY — the render continues in the background for up to 2 minutes at 1080p. Progress and completion are displayed live in the sidebar (the user will see a progress card under this tool call with an in-flight progress bar and a cancel button; on completion the card shows the MP4 path). Do NOT wait for completion — after calling this tool, inform the user that rendering started and where to watch.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "composition_id": { "type": "string" }
                    },
                    "required": ["composition_id"]
                }),
            },
            ToolDefinition {
                name: "canvas_lint_composition".into(),
                description: "Lint an existing composition artifact. Returns a LintReport { errors, warnings, infos, elapsed_ms } where each issue has { severity, rule_id, message, line?, col?, fix_hint? }. Use BEFORE canvas_render_video to catch invalid HTML/animation patterns that would cause render failure. Arguments: composition_id (string).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "composition_id": { "type": "string" }
                    },
                    "required": ["composition_id"]
                }),
            },
            ToolDefinition {
                name: "canvas_apply_design_md".into(),
                description: "Re-apply the composition's stored DESIGN.md to its rendered HTML. \
                              Reads `artifact.files['DESIGN.md']`, parses the YAML frontmatter, \
                              and replaces the `<style data-nf-design-tokens>:root { ... }</style>` \
                              block at the top of `index.html`. Idempotent and non-destructive: \
                              only the marked block changes, so any text/copy/CSS edits the \
                              user (or you) made elsewhere in `index.html` survive untouched.\n\n\
                              Use this AFTER the user edits DESIGN.md (in the Canvas Editor or \
                              via any other mechanism) to refresh the brand layer without \
                              regenerating the composition. Returns { composition_id }. \
                              Arguments: composition_id (string).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "composition_id": { "type": "string" }
                    },
                    "required": ["composition_id"]
                }),
            },
            ToolDefinition {
                name: "canvas_inspect_layout".into(),
                description: "Run a visual layout + WCAG contrast audit on a composition. \
                              The daemon broadcasts the request to the extension, which loads \
                              the composition into a sandbox iframe, seeks the timeline at N \
                              evenly-spaced timestamps (plus any hero frames you specify in \
                              `at`), collects bounding boxes for every `[data-track-index]` / \
                              `.clip` element, and runs a WCAG AA contrast check on every text \
                              element.\n\n\
                              Returns issues of these kinds (`kind` field):\n\
                              - `overflow_x` / `overflow_y` — element extends past stage edges\n\
                              - `off_stage` — element entirely outside the stage rect\n\
                              - `zero_size` — `[data-track-index]` element has 0×0 bbox during \
                                its `data-start..+data-duration` window (visibility/opacity \
                                misuse)\n\
                              - `contrast` — text contrast ratio below 4.5:1 (3:1 for large \
                                text); includes `fg`, `bg`, `ratio`, `required` fields\n\n\
                              Run AFTER `canvas_lint_composition` succeeds — lint catches \
                              static rules, inspect catches runtime/visual issues. Iterate: \
                              tweak DESIGN.md / scene padding / max-width, re-inspect, until \
                              `issues` is empty (or only known-acceptable items remain).\n\n\
                              Default frames=8 (good for most compositions). Bump to 15 for \
                              dense videos with rapid scene changes. Use `at` to additionally \
                              check exact hero-frame timestamps you suspect.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "composition_id": { "type": "string" },
                        "frames":         { "type": "integer", "minimum": 1, "maximum": 30, "description": "Evenly-spaced sample count; defaults to 8." },
                        "at":             { "type": "array", "items": { "type": "number" }, "description": "Optional explicit timestamps to additionally check." }
                    },
                    "required": ["composition_id"]
                }),
            },
            ToolDefinition {
                name: "canvas_attach_asset".into(),
                description: "Attach an image / video / audio / font / arbitrary file to a \
                              composition under `assets/<name>.<ext>`. The renderer auto-inlines \
                              every `<img src=\"assets/X\">`, `<video src=\"assets/X\">`, CSS \
                              `url(assets/X)`, etc. into a `data:` URI at render time, so the \
                              agent can write template-style references and trust they'll show up.\n\n\
                              **Use this whenever the user provides an image** (drag-drop, URL, \
                              clipboard, local file). DO NOT paste base64 data URIs directly \
                              into composition HTML — that bloats the artifact, breaks lint, \
                              and can't be re-used across compositions. Call canvas_attach_asset, \
                              then reference the returned `path`.\n\n\
                              Provide EXACTLY ONE source:\n\
                              - `local_path`: absolute filesystem path (e.g.\n\
                                `/tmp/Generated_Image_xyz.png`). **Use this when the path is \
                                visible in your local_files context** (the user uploaded a \
                                file to chat). The daemon reads the bytes server-side, so \
                                multi-megabyte images don't blow tool-arg size limits.\n\
                              - `data_b64`:   base64-encoded bytes (good for ≤ 1 MB only — \
                                tool-arg size limit). For bigger files use `local_path` or \
                                `url` instead.\n\
                              - `url`:        http(s) URL the daemon fetches (10s timeout).\n\
                              - `from_tab`:   not yet wired; use data_b64 with screenshot bytes.\n\n\
                              **Decision tree for user-attached files (most common case):**\n\
                              1. Look for the path in your local_files context (the daemon \
                                 surfaces it as a path string near your user message).\n\
                              2. Call `canvas_attach_asset({ composition_id, local_path })`.\n\
                              3. Reference the returned `path` as `<img src=\"assets/...\">`.\n\
                              DO NOT glob /tmp/, DO NOT search the page for path keywords, \
                              DO NOT ask the user for the path again — it's already in your \
                              context.\n\n\
                              Returns `{ path, mime_type, size_bytes }`. Use `path` (e.g. \
                              \"assets/hero.png\") in HTML — NEVER reference the URL, the \
                              base64, or the local_path again.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "composition_id": { "type": "string", "description": "Target composition artifact id." },
                        "name":           { "type": "string", "description": "Optional filename with extension (e.g. 'hero.png'). Inferred from URL / local_path basename / MIME otherwise." },
                        "mime_type":      { "type": "string", "description": "Optional explicit MIME type. Inferred from name / path / URL response otherwise." },
                        "data_b64":       { "type": "string", "description": "Inline base64 payload (mutually exclusive). Limit: ≤ 1 MB before base64 — for larger use local_path / url." },
                        "url":            { "type": "string", "description": "Public http(s) URL the daemon fetches (mutually exclusive)." },
                        "local_path":     { "type": "string", "description": "Absolute filesystem path the daemon reads (mutually exclusive). USE THIS when the path appears in your local_files context — it bypasses tool-arg size limits." },
                        "from_tab":       { "type": "integer", "description": "Tab id to screenshot. NOT YET WIRED — error today; use data_b64 instead." },
                        "role":           { "type": "string", "description": "Optional advisory hint: 'hero' / 'logo' / 'background' / 'decorative'. Informational only today." }
                    },
                    "required": ["composition_id"]
                }),
            },
            ToolDefinition {
                name: "tts_synthesize_api".into(),
                description: "Synthesize speech via the ElevenLabs HTTP API and return the \
                              audio bytes (base64 MP3). Requires `[tts.elevenlabs] api_key` \
                              configured in `~/.config/nevoflux/config.toml`; returns a clear \
                              ConfigMissing error otherwise.\n\n\
                              Usage in /video Mode 3 narrated flow:\n\
                              1. After creating a composition, call this tool with \
                              `composition_id` set — daemon writes the MP3 directly into the \
                              artifact's files map as `narration.mp3`.\n\
                              2. Edit the composition's `index.html` to add an \
                              `<audio src=\"narration.mp3\" data-start=\"0\" data-duration=\"<sec>\"/>` \
                              element on a track-index ≥ 100.\n\
                              3. Render — the audio is muxed into the output MP4 (P5b-final).\n\n\
                              Limits: text ≤ 600 chars (~60s of speech). Returns audio_b64 \
                              (always), wrote_to_files (only when composition_id supplied), \
                              voice_id (echoes which voice was used after default-fallback), \
                              duration_sec (estimated, ~2.5 chars/s for English).\n\n\
                              Voice IDs: ElevenLabs catalog (e.g. `21m00Tcm4TlvDq8ikWAM` = \
                              Rachel, `pNInz6obpgDQGcFmaJgB` = Adam). Defaults to Rachel \
                              (en-US female) if voice_id and config default both omitted.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to speak. Max 600 chars." },
                        "voice_id": { "type": "string", "description": "ElevenLabs voice ID (optional, falls back to config default or Rachel)." },
                        "model_id": { "type": "string", "description": "ElevenLabs model ID (optional, e.g. 'eleven_multilingual_v2')." },
                        "composition_id": { "type": "string", "description": "If set, the synthesized MP3 is written into this artifact's files map as 'narration.mp3'." }
                    },
                    "required": ["text"]
                }),
            },
            ToolDefinition {
                name: "tts_synthesize_local".into(),
                description: "Synthesize speech via local Kokoro-82M ONNX inference (no API \
                              key required, no network). Returns base64 WAV audio. \
                              \n\n\
                              STATUS: This tool is REGISTERED but the ONNX runtime \
                              integration is the next nevoflux-tts crate milestone. Calling \
                              it today returns a clear ConfigMissing error pointing at the \
                              setup steps; PREFER `tts_synthesize_api` (ElevenLabs) for \
                              narration that needs to ship now.\n\n\
                              When wired up, voice tags follow Kokoro convention: `af` (American \
                              female), `am` (American male), `bf` (British female), `bm` \
                              (British male), `zf` / `zm` (Mandarin female / male). The \
                              composition_id contract matches `tts_synthesize_api`: when \
                              provided, the audio lands in `narration.wav` inside the \
                              artifact's files map.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text":           { "type": "string", "description": "Text to speak. Max 600 chars." },
                        "voice_id":       { "type": "string", "description": "Kokoro voice tag: af / am / bf / bm / zf / zm. Defaults to config." },
                        "composition_id": { "type": "string", "description": "If set, the synthesized WAV is written into this artifact's files map as 'narration.wav'." }
                    },
                    "required": ["text"]
                }),
            },
            ToolDefinition {
                name: "tts_transcribe".into(),
                description: "Transcribe audio to text + per-segment timestamps via local \
                              Whisper ONNX. Used by P5c auto-captions to drive caption \
                              tracks from a composition's narration.mp3.\n\n\
                              STATUS: REGISTERED but inference not yet wired (ships with \
                              Kokoro in the nevoflux-tts crate). Returns ConfigMissing today.\n\n\
                              Provide EXACTLY ONE input source:\n\
                              - `audio_b64`: raw audio bytes (MP3/WAV/etc.), OR\n\
                              - `composition_id` + `file_path`: read the audio from an \
                                artifact's files map (e.g. file_path=\"narration.mp3\").\n\n\
                              Returns { text, segments: [{start_ms, end_ms, text}, ...] } — \
                              the segments stream straight into a caption track in the \
                              composition's HTML.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "audio_b64":      { "type": "string", "description": "Base64-encoded audio bytes." },
                        "composition_id": { "type": "string", "description": "Read audio from this artifact's files map (use with file_path)." },
                        "file_path":      { "type": "string", "description": "Path inside the artifact's files map (e.g. 'narration.mp3')." },
                        "model_size":     { "type": "string", "enum": ["tiny", "base", "small", "medium"], "description": "Whisper model size; defaults to config or 'base'." }
                    }
                }),
            },
            ToolDefinition {
                name: "canvas_create_from_visual_identity".into(),
                description: "Create a composition with DESIGN.md auto-derived from a \
                              VisualIdentity blob (typically the output of \
                              `canvas_extract_visual_identity`). Mode-3 (website-to-video) \
                              entry point — replaces the two-step \"extract → manually render \
                              DESIGN.md → create_composition\" flow with one deterministic \
                              call.\n\n\
                              The daemon serializes VI fields → DESIGN.md frontmatter \
                              (colors picked by role_hint; fonts by source label; defaults \
                              for spacing / motion / rounded). After this call the agent \
                              can use `browser_edit_artifact` on the composition's \
                              DESIGN.md if the user wants further tweaks (e.g. \"make \
                              background black\") and then `canvas_apply_design_md` to \
                              refresh the brand layer.\n\n\
                              Pass the VI JSON exactly as returned by \
                              canvas_extract_visual_identity; do NOT re-render it.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title":         { "type": "string" },
                        "width":         { "type": "integer", "minimum": 1, "maximum": 1920 },
                        "height":        { "type": "integer", "minimum": 1, "maximum": 1920 },
                        "duration_sec":  { "type": "number",  "minimum": 0.5, "maximum": 60 },
                        "fps":           { "type": "integer", "enum": [24, 25, 30] },
                        "bg":            { "type": ["string", "null"] },
                        "template":      {
                            "type": "string",
                            "enum": [
                                "website-promo-16x9",
                                "product-intro-16x9",
                                "product-intro-9x16",
                                "tiktok-hook",
                                "video-overlay",
                                "logo-3d-reveal",
                                "product-3d-spin"
                            ]
                        },
                        "visual_identity": {
                            "type": "object",
                            "description": "VisualIdentity object as returned by canvas_extract_visual_identity. Must include `url`; other fields are optional but inform DESIGN.md output."
                        }
                    },
                    "required": ["title", "width", "height", "duration_sec", "fps", "template", "visual_identity"]
                }),
            },
            ToolDefinition {
                name: "canvas_extract_visual_identity".into(),
                description: "Extract a brand's visual identity (name, tagline, primary URL, \
                              hero screenshot, and — once Slice B lands — colors / fonts / \
                              logo / key value-prop items) from a URL or an existing tab. \
                              Used by Mode 3 (website-to-video): given a URL, the agent \
                              calls this first to populate a composition's DESIGN.md.\n\n\
                              Tab handling:\n\
                              - URL mode: opens a background tab, runs extraction, closes it.\n\
                              - Tab mode: reuses the existing tab, leaves it open.\n\
                              Provide EXACTLY ONE of `target.url` or `target.tab_id`.\n\n\
                              Slice A returns { name, tagline, url, hero_screenshot_b64, \
                              extracted_at, warnings }. Color / font / logo / key_assets \
                              fields are present but empty until Slice B; consumers should \
                              treat them as optional.\n\n\
                              Returns the full `VisualIdentity` JSON; pass through to \
                              `browser_edit_artifact` on a composition's DESIGN.md to wire \
                              into the Mode 3 workflow.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "object",
                            "properties": {
                                "url":    { "type": "string", "description": "URL to open in a background tab" },
                                "tab_id": { "type": "integer", "description": "WebExtension tab id to reuse" }
                            }
                        },
                        "timeout_sec": {
                            "type": "integer",
                            "minimum": 5,
                            "maximum": 60,
                            "description": "Extraction wall-clock budget in seconds. Default 20."
                        },
                        "viewport": {
                            "type": "array",
                            "items": { "type": "integer" },
                            "minItems": 2,
                            "maxItems": 2,
                            "description": "[width, height] for the screenshot. Default [1920, 1080]."
                        }
                    },
                    "required": ["target"]
                }),
            },
            // /loop skill tools (spec §10).
            //
            // NOTE: These ToolDefinition entries make the tools VISIBLE to the LLM,
            // but the actual dispatch lives in `mcp_tool_executor::execute_mcp_tool`
            // (Phase 9.3). The builtin-wasm `Agent::execute_tool` match arm has no
            // direct access to `LoopManager` or `Database` (the `HostFunctions`
            // trait does not surface them), so a direct-API provider that reaches
            // builtin-wasm without going through mcp_tool_executor will fall through
            // to the generic "unknown tool" path. Wiring the builtin-wasm execution
            // arm requires extending `HostFunctions` and is deferred.
            ToolDefinition {
                name: "loop.create".into(),
                description: "Create a recurring task that re-runs a prompt or wrapped skill on a trigger. Trigger grammar: time:<5m|1h|...>, time:dynamic, event:<topic>, state:tab=current|<id>:<css>:change, with AND/OR up to depth 3. The `mode` arg picks the iteration's tool catalog: 'chat' (default, safe — reasoning + scratchpad), 'browser' (chat + browser interaction), 'agent' (browser + write/edit/bash).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "trigger_expr": { "type": "string", "description": "trigger expression, e.g. 'time:5m' or 'event:ui:tab:click'" },
                        "prompt_text": { "type": "string", "description": "raw prompt re-issued each fire — XOR with wrapped_skill" },
                        "wrapped_skill": { "type": "object", "description": "{name, args} — XOR with prompt_text" },
                        "mode": {
                            "type": "string",
                            "enum": ["chat", "browser", "agent"],
                            "description": "Agent mode for iterations. Default 'chat'."
                        }
                    },
                    "required": ["trigger_expr"]
                }),
            },
            ToolDefinition {
                name: "loop.list".into(),
                description: "List loops in the current session.".into(),
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "loop.cancel".into(),
                description: "Cancel a loop. From inside an iteration you may only cancel your own loop_id.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "loop_id": { "type": "string" } },
                    "required": ["loop_id"]
                }),
            },
            ToolDefinition {
                name: "loop.scratchpad.get".into(),
                description: "Read the loop's ≤4KB scratchpad. Defaults to current iteration's loop.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "loop_id": { "type": "string" } }
                }),
            },
            ToolDefinition {
                name: "loop.scratchpad.set".into(),
                description: "Replace the loop's scratchpad (≤4096 bytes). Iteration-only.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "content": { "type": "string" } },
                    "required": ["content"]
                }),
            },
        ]
    }

    /// Get available tools for browser mode (without orchestrate). Used by
    /// `orchestrate_tool` and `get_agent_tools_without_orchestrate` to avoid recursion.
    fn get_browser_tools_without_orchestrate(&self) -> Vec<ToolDefinition> {
        let mut tools = self.get_chat_tools();

        // Browser navigation
        tools.push(ToolDefinition {
            name: "browser_navigate".into(),
            description: "Navigate to a URL in the CURRENT tab. \
If the site is ALREADY OPEN in another tab, use browser_activate_tab instead. \
Only set new_tab=true when the user EXPLICITLY says 'new tab'. \
For going back, use browser_go_back. NEVER use navigate to 'go back'.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to"
                    },
                    "new_tab": {
                        "type": "boolean",
                        "default": false,
                        "description": "Default false. Do NOT set this unless the user says 'new tab' or 'open in new tab'. When omitted or false, navigates the current tab."
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID (uses active tab if not specified)"
                    }
                },
                "required": ["url"]
            }),
        });

        tools.push(ToolDefinition {
            name: "browser_activate_tab".into(),
            description: "Switch to (activate) an already-open browser tab. \
When the user says 'activate', 'switch to', 'go to [site]' and that site is already open, \
use browser_get_tabs first to find the tab, then activate it. \
Do NOT use browser_navigate when the tab is already open — activate it instead."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "The tab ID to activate (from browser_get_tabs)"
                    }
                },
                "required": ["tab_id"]
            }),
        });

        // History navigation
        tools.push(ToolDefinition {
            name: "browser_go_back".into(),
            description: "Go back in browser history. Preserves page state and form data. Preferred over navigate for returning to previous page.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID (uses active tab if not specified)"
                    }
                }
            }),
        });

        tools.push(ToolDefinition {
            name: "browser_go_forward".into(),
            description: "Go forward in browser history. Returns updated page state.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID (uses active tab if not specified)"
                    }
                }
            }),
        });

        // Click by selector
        tools.push(ToolDefinition {
            name: "browser_click".into(),
            description: "Click on an element by CSS selector".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to click"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector"]
            }),
        });

        // Click by ID
        tools.push(ToolDefinition {
            name: "browser_click_by_id".into(),
            description: "Click an interactive element by its [eN] ID from the page state snapshot. Only use IDs from the MOST RECENT snapshot — they change after every action.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The ID attribute of the element to click"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["element_id"]
            }),
        });

        // Type by selector (keystrokes)
        tools.push(ToolDefinition {
            name: "browser_type".into(),
            description: "Type text into an element by CSS selector (simulates keystrokes)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the input element"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector", "text"]
            }),
        });

        // Type by ID
        tools.push(ToolDefinition {
            name: "browser_type_by_id".into(),
            description: "Type text character by character into an element. Use ONLY for autocomplete, search boxes, or real-time validation. For normal form fields, prefer browser_fill_by_id. (Deprecated 2026-04; prefer browser_input which handles rich text editors.)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The ID attribute of the input element"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["element_id", "text"]
            }),
        });

        // Fill by selector (set value)
        tools.push(ToolDefinition {
            name: "browser_fill".into(),
            description: "Fill an input element with a value by CSS selector (sets value directly)"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the input element"
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to fill"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector", "value"]
            }),
        });

        // Fill by ID
        tools.push(ToolDefinition {
            name: "browser_fill_by_id".into(),
            description: "Set a form field's value by element ID. DEFAULT for form filling. Faster than type_by_id. If fill doesn't trigger expected behavior, fall back to type_by_id. (Deprecated 2026-04; prefer browser_input which handles rich text editors.)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "element_id": {
                        "type": "string",
                        "description": "The ID attribute of the input element"
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to fill"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["element_id", "value"]
            }),
        });

        // PR #2 / #2.5: Browser input strategy engine tools
        //
        // browser_input is the high-level orchestrated text input tool that
        // automatically handles rich text editors (Draft.js, Lexical,
        // ProseMirror, Slate) where legacy browser_fill_by_id silently fails.
        // Daemon intercepts the action and runs a full probe → decide →
        // execute → verify pipeline.
        //
        // browser_probe is the escape-hatch inspection tool that returns a
        // rich Fingerprint (tag, is_content_editable, editor_framework, etc.)
        // for LLM-driven custom strategy selection.
        tools.push(ToolDefinition {
            name: "browser_input".into(),
            description: "High-level text input tool. **PREFER this over browser_fill_by_id \
and browser_type_by_id when targeting rich text editors** (Twitter/X compose, \
Facebook/Threads, LinkedIn, Discord, Reddit new compose, ProseMirror/Slate/Draft.js/Lexical). \
Probes the element, picks a strategy based on framework detection, executes, and verifies. \
Fixes 'silent success' on contentEditable div editors where legacy fill_by_id did nothing. \
Use mode='fill' to replace content, mode='type' to append. Use a CSS selector (not element_id).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the target input / contentEditable element"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to insert"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["fill", "type"],
                        "description": "'fill' replaces existing content; 'type' appends. Default: 'fill'."
                    },
                    "verify": {
                        "type": "boolean",
                        "description": "If true (default), read back the content after execution and report match/mismatch"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector", "text"]
            }),
        });

        tools.push(ToolDefinition {
            name: "browser_probe".into(),
            description: "Probe an element and return its Fingerprint: tag, input_type, \
is_content_editable, editor_framework (draft.js/lexical/prosemirror/slate/etc.), \
react_fiber_present, visibility, focusability, shadow DOM depth, iframe context, \
innermost_editable_selector, computed_role. Useful when you need to reason about \
page structure before choosing an input strategy, or when debugging why browser_input \
picked a particular path."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to probe"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector"]
            }),
        });

        // PR #5: File upload tool
        tools.push(ToolDefinition {
            name: "browser_upload_file".into(),
            description: "Upload a file to an <input type=\"file\"> element. \
REQUIRED for file uploads — do NOT use browser_fill or browser_input on file inputs. \
The file is served via a localhost HTTP bridge to bypass the native messaging size limit. \
Set workspace_dir to the directory containing the file to allow uploads from any location.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the <input type=\"file\"> element"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the file to upload"
                    },
                    "workspace_dir": {
                        "type": "string",
                        "description": "Directory containing the file (file_path must be inside this dir). Defaults to ~/.local/share/nevoflux/workspace/"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector", "file_path"]
            }),
        });

        // NOTE: browser_get_content, browser_get_markdown, and browser_screenshot
        // are already included via get_chat_tools() above.

        // Eval JS
        tools.push(ToolDefinition {
            name: "browser_eval_js".into(),
            description: "Execute JavaScript code in the page context".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "JavaScript code to execute"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["script"]
            }),
        });

        // Scroll
        tools.push(ToolDefinition {
            name: "browser_scroll".into(),
            description: "Scroll the page to reveal more content. Use to find elements not in current snapshot. Returns updated page state.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "left", "right"],
                        "description": "Direction to scroll"
                    },
                    "amount": {
                        "type": "string",
                        "description": "Scroll amount: 'page' (default), 'half', or pixel count as string (e.g., '300')"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["direction"]
            }),
        });

        // Wait for element
        tools.push(ToolDefinition {
            name: "browser_wait_for".into(),
            description: "Wait for an element to appear on the page".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to wait for"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Maximum time to wait in milliseconds (default: 10000)",
                        "default": 10000
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    }
                },
                "required": ["selector"]
            }),
        });

        // Get elements (usually not needed — page state is auto-injected)
        tools.push(ToolDefinition {
            name: "browser_get_elements".into(),
            description: "Get interactive elements on the page. Use keywords to locate specific elements by visible text (e.g. button labels, input placeholders). Keywords must match the page's actual language — check the 'lang:' field in page output. Keywords work across all frameworks including React, Vue, Shadow DOM.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID"
                    },
                    "keywords": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Keywords to locate target elements by visible text. IMPORTANT: keywords must be in the page's language (see 'lang:' in snapshot output). Example: [\"Log in\", \"username\"] for English pages, [\"登录\", \"用户名\"] for Chinese pages."
                    }
                }
            }),
        });

        // Find elements in cache (after browser_get_elements)
        tools.push(ToolDefinition {
            name: "browser_find_elements".into(),
            description: "Search cached elements from the last browser_get_elements call. Filters are AND-combined.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "description": "Filter by ARIA role (exact match, case-insensitive)"
                    },
                    "name": {
                        "type": "string",
                        "description": "Substring match on element name/text (case-insensitive)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Substring match on CSS selector"
                    },
                    "near_x": {
                        "type": "integer",
                        "description": "Find elements near this X coordinate (requires near_y)"
                    },
                    "near_y": {
                        "type": "integer",
                        "description": "Find elements near this Y coordinate (requires near_x)"
                    },
                    "radius": {
                        "type": "integer",
                        "description": "Search radius in pixels (default: 50, requires near_x + near_y)",
                        "default": 50
                    },
                    "unnamed_only": {
                        "type": "boolean",
                        "description": "Only return elements with empty name (default: false)",
                        "default": false
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results (default: 20)",
                        "default": 20
                    }
                }
            }),
        });

        // Get full details for one element from cache
        tools.push(ToolDefinition {
            name: "browser_element_info".into(),
            description: "Get full details for a single element by ID from the cache. Call browser_get_elements first.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Element ID (e.g., \"e42\")"
                    }
                },
                "required": ["id"]
            }),
        });

        // Meta-tool to load computer use tools (Browser mode only).
        // Calling this signals that computer use is needed and triggers
        // full prompt injection on the next turn.
        tools.push(ToolDefinition {
            name: "load_computer_use_tools".into(),
            description: "Load native computer control tools (mouse, keyboard, screenshot) for interacting with OS dialogs, desktop apps, or other elements outside the browser DOM. Call this when browser tools cannot reach the target.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        });

        // Dynamic MCP tool discovery (available in Browser and Agent modes)
        tools.push(ToolDefinition {
            name: "tool_search".into(),
            description: "Search for external MCP server tools only (NOT built-in tools like web_search, browser_*, read, write). Use when you need tools from connected MCP servers. Always search first — never guess tool names or schemas.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keywords to search for (e.g., 'git', 'database', 'image')"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        });

        tools.push(ToolDefinition {
            name: "tool_call_dynamic".into(),
            description: "Call a tool discovered via tool_search. Read the returned schema carefully before calling.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "The exact name of the tool to call"
                    },
                    "arguments": {
                        "type": "string",
                        "description": "JSON string of arguments to pass to the tool (e.g., '{\"key\": \"value\"}')"
                    }
                },
                "required": ["tool_name", "arguments"],
                "additionalProperties": false
            }),
        });

        tools
    }

    /// Get available tools for browser mode (includes dynamically-generated orchestrate tool).
    fn get_browser_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = self.get_browser_tools_without_orchestrate();
        tools.push(self.orchestrate_tool(AgentMode::Browser));
        tools
    }

    /// Build the orchestrate ToolDefinition with dynamic signatures scoped to `mode`.
    fn orchestrate_tool(&self, mode: AgentMode) -> ToolDefinition {
        // Get tools for this mode *without* orchestrate to avoid recursion.
        let mode_tools = match mode {
            AgentMode::Chat => self.get_chat_tools(),
            AgentMode::Browser => self.get_browser_tools_without_orchestrate(),
            AgentMode::Agent | AgentMode::Code => self.get_agent_tools_without_orchestrate(),
        };

        // Filter out meta-tools that don't make sense inside orchestrate.
        let orchestrable: Vec<&ToolDefinition> = mode_tools
            .iter()
            .filter(|t| {
                !matches!(
                    t.name.as_str(),
                    "orchestrate"
                        | "think"
                        | "plan"
                        | "create_artifact"
                        | "switch_model"
                        | "load_computer_use_tools"
                        | "subagent_spawn"
                        | "subagent_wait"
                        | "subagent_wait_all"
                        | "subagent_status"
                        | "subagent_kill"
                        | "subagent_list"
                        | "list_agents"
                        | "skill_load"
                )
            })
            .collect();

        // Build compact signatures.
        let mut signatures = String::new();
        for tool in &orchestrable {
            let is_seq = is_orchestrate_sequential(&tool.name);
            let prefix = if is_seq { "def" } else { "async def" };
            let tag = if is_seq { " [sequential]" } else { "" };
            let params = extract_params_compact(&tool.input_schema);
            let desc_short = tool
                .description
                .split('.')
                .next()
                .unwrap_or(&tool.description);
            signatures.push_str(&format!(
                "  {} {}({}) -> Any  # {}{}\n",
                prefix, tool.name, params, desc_short, tag
            ));
        }

        let description = format!(
            "Write Python to orchestrate multiple tool calls in one step.\n\n\
When to use: If you can reasonably predict what each tool will return, \
use orchestrate to batch the calls. If unsure what a page or file contains, \
call a read-only tool first to inspect, then orchestrate the remaining steps.\n\n\
Example — \"save top 10 HN titles to a file\":\n  \
items = browser_eval_js(\"Array.from(document.querySelectorAll('.titleline > a'))\
.slice(0,10).map(a => a.textContent)\")\n  \
write_file(\"hn.txt\", '\\n'.join(items))\n\n\
Available functions (call directly, no import needed):\n\
{}\n\
Rules:\n\
- async def tools can be combined with asyncio.gather() for parallel execution\n\
- def tools marked [sequential] must be called one at a time\n\
- Do NOT use: class, match/case, import, with, yield, decorators\n\
- Supported: variables, def, if/elif/else, for/while, try/except, \
comprehensions, f-strings, lambda, asyncio.gather\n\
- The last expression value is returned as the tool result\n\
- Use print() for debug output (included in result)",
            signatures.trim_end()
        );

        ToolDefinition {
            name: "orchestrate".into(),
            description,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python code to execute"
                    }
                },
                "required": ["code"]
            }),
        }
    }

    /// Get available tools for agent mode (without orchestrate). Used by
    /// `orchestrate_tool` to avoid infinite recursion.
    fn get_agent_tools_without_orchestrate(&self) -> Vec<ToolDefinition> {
        let mut tools = self.get_browser_tools_without_orchestrate();

        // Add file tools
        tools.push(ToolDefinition {
            name: "read".into(),
            description: "Read file contents. Returns partial content with metadata (total_lines). Default: first 200 lines. Use offset/limit for pagination. PREFER over bash cat.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The absolute path to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line offset to start reading from"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to read"
                    }
                },
                "required": ["file_path"]
            }),
        });

        tools.push(ToolDefinition {
            name: "write".into(),
            description: "Create a new file or overwrite an existing file entirely. WARNING: Overwrites the full file content. Use edit() for partial changes to existing files.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The absolute path to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write"
                    }
                },
                "required": ["file_path", "content"]
            }),
        });

        tools.push(ToolDefinition {
            name: "edit".into(),
            description: "Find and replace exact text in a file. PREFER over write() for modifying existing files. Always read() the file first to see current content. Include surrounding context in old_string to ensure a unique match.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The text to find"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement text"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences"
                    }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
        });

        tools.push(ToolDefinition {
            name: "bash".into(),
            description: "Execute a shell command. Default timeout: 30s. Output capped at 200 lines. Use ONLY when specialized tools (read, grep, glob, edit) cannot accomplish the task.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in milliseconds"
                    }
                },
                "required": ["command"]
            }),
        });

        tools.push(ToolDefinition {
            name: "glob".into(),
            description: "Find files by glob pattern (e.g. 'src/**/*.rs'). PREFER over bash find."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern like '**/*.rs'"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory"
                    }
                },
                "required": ["pattern"]
            }),
        });

        tools.push(ToolDefinition {
            name: "grep".into(),
            description: "Search file contents using regex. Returns structured matches with counts. PREFER over bash grep. Supports file type filter. Respects .gitignore.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search"
                    },
                    "type": {
                        "type": "string",
                        "description": "File type filter (e.g., 'rs', 'py', 'js') \u{2014} same types as ripgrep"
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Case insensitive search (default: false)"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum results to return (default: 50)"
                    }
                },
                "required": ["pattern"]
            }),
        });

        // tool_search and tool_call_dynamic are inherited from browser tools

        // Computer control tools
        tools.push(ToolDefinition {
            name: "computer_screenshot".into(),
            description: "Take a screenshot of the entire screen or a specific monitor".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "monitor": {
                        "type": "integer",
                        "description": "Monitor index (0-based). If not specified, captures primary monitor.",
                        "minimum": 0
                    }
                }
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_mouse_move".into(),
            description: "Move the mouse cursor to a specified position on screen without clicking"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    }
                },
                "required": ["x", "y"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_click".into(),
            description: "Click at a specific screen position".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to click",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    },
                    "click_type": {
                        "type": "string",
                        "description": "Type of click to perform",
                        "enum": ["single", "double", "triple"],
                        "default": "single"
                    }
                },
                "required": ["x", "y"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_type_text".into(),
            description: "Type text using the keyboard at the current cursor position".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The text to type"
                    },
                    "delay_ms": {
                        "type": "integer",
                        "description": "Delay between keystrokes in milliseconds",
                        "default": 0,
                        "minimum": 0
                    }
                },
                "required": ["text"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_key".into(),
            description: "Press keyboard keys or key combinations".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key to press (e.g., 'Enter', 'Tab', 'Escape', 'a', 'F1')"
                    },
                    "modifiers": {
                        "type": "array",
                        "description": "Modifier keys to hold while pressing the key",
                        "items": {
                            "type": "string",
                            "enum": ["ctrl", "alt", "shift", "meta", "super"]
                        },
                        "default": []
                    },
                    "repeat": {
                        "type": "integer",
                        "description": "Number of times to repeat the key press",
                        "default": 1,
                        "minimum": 1,
                        "maximum": 100
                    }
                },
                "required": ["key"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_scroll".into(),
            description: "Scroll at a specific screen position".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels for scroll position"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels for scroll position"
                    },
                    "direction": {
                        "type": "string",
                        "description": "Direction to scroll",
                        "enum": ["up", "down", "left", "right"]
                    },
                    "amount": {
                        "type": "integer",
                        "description": "Number of scroll units",
                        "default": 3,
                        "minimum": 1,
                        "maximum": 100
                    }
                },
                "required": ["x", "y", "direction"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_drag".into(),
            description: "Drag from one screen position to another (press, move, release)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "start_x": {
                        "type": "integer",
                        "description": "Starting X coordinate in pixels"
                    },
                    "start_y": {
                        "type": "integer",
                        "description": "Starting Y coordinate in pixels"
                    },
                    "end_x": {
                        "type": "integer",
                        "description": "Ending X coordinate in pixels"
                    },
                    "end_y": {
                        "type": "integer",
                        "description": "Ending Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to use for dragging",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["start_x", "start_y", "end_x", "end_y"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_cursor_position".into(),
            description: "Get the current mouse cursor position on screen".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_mouse_down".into(),
            description:
                "Press and hold a mouse button at a position (use computer_mouse_up to release)"
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to press",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["x", "y"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_mouse_up".into(),
            description: "Release a mouse button at a position (use after computer_mouse_down)"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate in pixels"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate in pixels"
                    },
                    "button": {
                        "type": "string",
                        "description": "Mouse button to release",
                        "enum": ["left", "right", "middle"],
                        "default": "left"
                    }
                },
                "required": ["x", "y"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_hold_key".into(),
            description: "Hold a key down for a specified duration, then release".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key to hold (e.g., 'Shift', 'Control', 'Alt', 'a')"
                    },
                    "duration_ms": {
                        "type": "integer",
                        "description": "Duration to hold the key in milliseconds",
                        "minimum": 100,
                        "maximum": 10000,
                        "default": 500
                    },
                    "modifiers": {
                        "type": "array",
                        "description": "Modifier keys to hold simultaneously",
                        "items": {
                            "type": "string",
                            "enum": ["ctrl", "alt", "shift", "meta", "super"]
                        },
                        "default": []
                    }
                },
                "required": ["key", "duration_ms"]
            }),
        });

        tools.push(ToolDefinition {
            name: "computer_wait".into(),
            description: "Wait for a specified duration before continuing. Use to wait for animations, loading, or transitions.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ms": {
                        "type": "integer",
                        "description": "Duration to wait in milliseconds",
                        "minimum": 100,
                        "maximum": 10000,
                        "default": 1000
                    }
                },
                "required": ["ms"]
            }),
        });

        // Subagent tools for parallel work
        tools.push(ToolDefinition {
            name: "subagent_spawn".into(),
            description: "Spawn a lightweight subagent for a focused parallel task. Returns an ID. Subagents have NO page interaction — read-only browser access (with tab_id) and web search. Use for: parallel research, parallel summarization, independent subtasks. Use list_agents to discover available roles.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The task description for the sub-agent to execute"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["chat", "browser", "agent"],
                        "description": "Execution mode for the sub-agent (default: agent)",
                        "default": "agent"
                    },
                    "tab_id": {
                        "type": "integer",
                        "description": "Optional tab ID for the sub-agent to read page content from (read-only access)"
                    },
                    "role": {
                        "type": "string",
                        "description": "Named role to apply (e.g. 'researcher', 'explorer'). Use list_agents to see available roles."
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Custom system prompt override for the sub-agent"
                    },
                    "provider": {
                        "type": "string",
                        "description": "LLM provider name (e.g. 'anthropic', 'openai'). Required when specifying model."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model name override (requires provider to be set)"
                    },
                    "max_iterations": {
                        "type": "integer",
                        "description": "Maximum iterations before timeout"
                    },
                    "tools": {
                        "type": "string",
                        "description": "Tool access configuration. Use \"none\" to disable all tools (pure text mode), or JSON like \"{\\\"Allow\\\": [\\\"read\\\", \\\"glob\\\", \\\"browser_*\\\"]}\" for an allowlist with optional wildcard."
                    }
                },
                "required": ["task"]
            }),
        });

        tools.push(ToolDefinition {
            name: "subagent_status".into(),
            description: "Check the current status of a sub-agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "integer",
                        "description": "The sub-agent ID returned by subagent_spawn"
                    }
                },
                "required": ["id"]
            }),
        });

        tools.push(ToolDefinition {
            name: "subagent_wait".into(),
            description: "Wait for a sub-agent to complete and get its result. Blocks until \
                          the sub-agent finishes execution."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "integer",
                        "description": "The sub-agent ID returned by subagent_spawn"
                    }
                },
                "required": ["id"]
            }),
        });

        tools.push(ToolDefinition {
            name: "subagent_wait_all".into(),
            description: "Wait for multiple subagents to complete. Returns all results at once. PREFERRED over calling subagent_wait multiple times.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Sub-agent IDs to wait for"
                    }
                },
                "required": ["ids"]
            }),
        });

        tools.push(ToolDefinition {
            name: "subagent_kill".into(),
            description: "Terminate a running sub-agent. Returns true if the sub-agent was \
                          killed, false if it had already completed."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "integer",
                        "description": "The sub-agent ID to terminate"
                    }
                },
                "required": ["id"]
            }),
        });

        tools.push(ToolDefinition {
            name: "subagent_list".into(),
            description: "List all sub-agents with their IDs, tasks, modes, and statuses.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        });

        tools.push(ToolDefinition {
            name: "list_agents".into(),
            description: "List available agent roles for subagent spawning. Returns role names and descriptions. Use these roles with subagent_spawn's role parameter.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        });

        tools
    }

    /// Get available tools for agent mode (includes dynamically-generated orchestrate tool).
    fn get_agent_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = self.get_agent_tools_without_orchestrate();
        tools.push(self.orchestrate_tool(AgentMode::Agent));
        tools
    }
}

/// Check if `text` ends with a non-empty prefix of `<tool_call>`.
///
/// Returns `Some(index)` where the partial prefix starts, or `None`.
/// This handles the case where the `<tool_call>` opening tag is split across
/// streaming chunks (e.g. `<tool_call` in one chunk and `>` in the next).
fn find_tool_call_tag_prefix_at_end(text: &str) -> Option<usize> {
    const TAG: &str = "<tool_call>";
    for prefix_len in (1..TAG.len()).rev() {
        if text.ends_with(&TAG[..prefix_len]) {
            return Some(text.len() - prefix_len);
        }
    }
    None
}

/// Parse `<tool_call>...</tool_call>` XML from text into `ToolCall` structs.
///
/// Returns `(cleaned_text_without_markers, extracted_tool_calls)`.
/// Text-based tool calls are emitted by some LLM providers as plain text instead
/// of structured `tool_use` blocks.
fn parse_tool_calls_from_text(text: &str) -> (String, Vec<ToolCall>) {
    let mut tool_calls = Vec::new();
    let mut cleaned = String::new();
    let mut remaining = text;

    loop {
        let Some(start_idx) = remaining.find("<tool_call>") else {
            // If we already extracted tool calls, discard any trailing text —
            // the model should have stopped after </tool_call> but may have
            // hallucinated tool results, explanations, or other garbage.
            if tool_calls.is_empty() {
                cleaned.push_str(remaining);
            }
            break;
        };

        // Add text before the marker
        cleaned.push_str(&remaining[..start_idx]);

        let after_start = &remaining[start_idx + "<tool_call>".len()..];

        let Some(end_idx) = after_start.find("</tool_call>") else {
            // No closing tag — keep the raw text as-is
            cleaned.push_str(&remaining[start_idx..]);
            break;
        };

        let json_str = after_start[..end_idx].trim();

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
            let id = parsed
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = parsed
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = parsed
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
                call_id: None,
                signature: None,
            });
        } else {
            // Malformed JSON — keep the raw text
            cleaned.push_str(
                &remaining
                    [start_idx..start_idx + "<tool_call>".len() + end_idx + "</tool_call>".len()],
            );
        }

        remaining = &after_start[end_idx + "</tool_call>".len()..];
    }

    let cleaned = cleaned.trim().to_string();
    (cleaned, tool_calls)
}

/// Format skill summaries for system prompt injection.
fn format_skill_summaries(skills: &[SkillSummary]) -> String {
    skills
        .iter()
        .map(|s| format!("- **{}**: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if a user message contains keywords that suggest computer use is needed.
/// Extract a `modifiers` string array from a JSON arguments object.
fn parse_modifiers_arg(args: &serde_json::Value) -> Vec<String> {
    args.get("modifiers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn should_trigger_computer_use(message: &str) -> bool {
    let lower = message.to_lowercase();
    // English keywords
    const EN_KEYWORDS: &[&str] = &[
        "desktop",
        "screenshot",
        "system settings",
        "system preferences",
        "file manager",
        "file explorer",
        "dialog",
        "file picker",
        "file upload",
        "drag and drop",
        "drag",
        "captcha",
        "native app",
        "native window",
        "taskbar",
        "dock",
        "start menu",
        "notification",
        "popup",
        "right-click menu",
        "context menu",
        "terminal",
        "command prompt",
    ];
    for kw in EN_KEYWORDS {
        if lower.contains(kw) {
            return true;
        }
    }
    // Chinese keywords
    const ZH_KEYWORDS: &[&str] = &[
        "桌面",
        "截屏",
        "截图",
        "系统设置",
        "文件管理器",
        "弹窗",
        "对话框",
        "上传文件",
        "拖拽",
        "拖放",
        "验证码",
        "原生应用",
        "任务栏",
        "右键菜单",
        "通知",
        "终端",
        "命令行",
    ];
    for kw in ZH_KEYWORDS {
        if message.contains(kw) {
            return true;
        }
    }
    false
}

/// Check if a browser tool result indicates a failure that suggests computer use is needed.
/// Detects native file pickers, OS dialogs, permission prompts, etc.
fn is_browser_failure_suggesting_computer_use(result: &str) -> bool {
    // Quick check on the original string for error indicators before allocating.
    // These keywords are ASCII-lowercase in JSON output, so no case conversion needed.
    let has_error =
        result.contains("error") || result.contains("failed") || result.contains("false");
    if !has_error {
        return false;
    }

    let lower = result.to_lowercase();
    const PATTERNS: &[&str] = &[
        "file picker",
        "file chooser",
        "file dialog",
        "upload dialog",
        "native dialog",
        "os dialog",
        "system dialog",
        "permission prompt",
        "permission dialog",
        "not interactable",
        "element not found",
        "element is not clickable",
        "intercepted",
        "obscured",
        // all_tiers_exhausted + effective:false means every programmatic click
        // method was tried and none worked — strong signal that the element
        // requires a trusted user gesture (e.g. OAuth popups, file inputs).
        "all_tiers_exhausted",
    ];
    for pattern in PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }
    false
}

/// Safely truncate a string to at most `max_bytes` bytes at a valid UTF-8 char boundary.
fn truncate_string_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Calculate the total size of all messages in bytes.
fn calculate_messages_size(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| {
            let mut size = m.content.len();
            // Add tool calls size for assistant messages
            for tc in &m.tool_calls {
                size += tc.id.len() + tc.name.len();
                // arguments is a serde_json::Value, estimate its string size
                size += tc.arguments.to_string().len();
                if let Some(ref call_id) = tc.call_id {
                    size += call_id.len();
                }
            }
            size
        })
        .sum()
}

/// Truncate tool result content if it would exceed the total message size limit.
///
/// This function dynamically calculates how much space is available based on:
/// - Current total message size
/// - Maximum allowed total size (300KB, ~75K tokens)
/// - Reserved space for LLM output (50KB)
/// - Minimum tool result size (10KB)
fn truncate_tool_result_if_needed(messages: &[Message], content: &str) -> String {
    // Total message size limit (~75K tokens for most models)
    const MAX_TOTAL_MESSAGE_SIZE: usize = 300 * 1024; // 300KB
                                                      // Reserved space for LLM output
    const RESERVED_OUTPUT_SIZE: usize = 50 * 1024; // 50KB
                                                   // Minimum tool result size (don't truncate below this)
    const MIN_TOOL_RESULT_SIZE: usize = 10 * 1024; // 10KB

    let current_size = calculate_messages_size(messages);
    let available_space = MAX_TOTAL_MESSAGE_SIZE
        .saturating_sub(current_size)
        .saturating_sub(RESERVED_OUTPUT_SIZE);

    // Calculate max size for this tool result
    let max_result_size = available_space.max(MIN_TOOL_RESULT_SIZE);

    if content.len() <= max_result_size {
        return content.to_string();
    }

    // Need to truncate
    eprintln!(
        "[AGENT] Tool result truncated: {} -> {} bytes (current_msgs={}, available={})",
        content.len(),
        max_result_size,
        current_size,
        available_space
    );

    // Use safe UTF-8 truncation
    let truncated = truncate_string_safe(content, max_result_size);

    format!(
        "{}...\n\n[Content truncated: {} bytes total, showing first {} bytes]",
        truncated,
        content.len(),
        truncated.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MockHostFunctions;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_iterations, 100);
        assert!(config.use_streaming);
        assert!(!config.suppress_streaming);
    }

    #[test]
    fn test_agent_config_for_subagent() {
        let config = AgentConfig::for_subagent();
        assert_eq!(config.max_iterations, 100);
        assert!(!config.use_streaming);
        assert!(config.suppress_streaming);
    }

    #[test]
    fn test_agent_config_with_suppress_streaming() {
        let config = AgentConfig::default().with_suppress_streaming(true);
        assert!(config.suppress_streaming);
    }

    #[test]
    fn test_agent_new() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        assert_eq!(agent.config.max_iterations, 100);
    }

    #[test]
    fn test_agent_with_config() {
        let mock = MockHostFunctions::new();
        let config = AgentConfig {
            max_iterations: 50,
            use_streaming: false,
            suppress_streaming: false,
            is_subagent: false,
        };
        let agent = Agent::with_config(mock, config);
        assert_eq!(agent.config.max_iterations, 50);
    }

    #[test]
    fn test_agent_run_with_custom_system_prompt() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Agent,
            user_message: "Search for files".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: Some(
                "You are a specialized file search sub-agent. Focus only on finding files.".into(),
            ),
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        // Should run successfully with custom prompt
        let output = agent.run(&input).unwrap();
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_build_system_prompt() {
        let no_cu = ComputerUseFlags::default();
        let prompt =
            Agent::<MockHostFunctions>::build_system_prompt(AgentMode::Chat, &[], &[], no_cu, None);
        assert!(!prompt.is_empty());
        assert_eq!(prompt, CHAT_PROMPT);

        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Browser,
            &[],
            &[],
            no_cu,
            None,
        );
        assert_eq!(prompt, BROWSER_PROMPT);

        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Agent,
            &[],
            &[],
            no_cu,
            None,
        );
        assert_eq!(prompt, AGENT_PROMPT);
    }

    #[test]
    fn test_format_tab_context_single_tab() {
        let tab = TabInfo {
            tab_id: 42,
            tab_title: "Test Page".into(),
            url: "https://example.com".into(),
            space: String::new(),
        };
        let ctx = Agent::<MockHostFunctions>::format_tab_context(Some(&tab), &[]);
        assert!(ctx.contains("current_tab: 42"));
        assert!(ctx.contains("\"Test Page\""));
        assert!(ctx.contains("https://example.com"));
        // Must NOT contain behavioral instructions
        assert!(!ctx.contains("IMPORTANT"));
        assert!(!ctx.contains("browser_get_markdown"));
    }

    #[test]
    fn test_format_tab_context_multi_tab() {
        let tabs = vec![
            TabInfo {
                tab_id: 1,
                tab_title: "Tab A".into(),
                url: "https://a.com".into(),
                space: "Work".into(),
            },
            TabInfo {
                tab_id: 2,
                tab_title: "Tab B".into(),
                url: "https://b.com".into(),
                space: "Work".into(),
            },
            TabInfo {
                tab_id: 3,
                tab_title: "Tab C".into(),
                url: "https://c.com".into(),
                space: String::new(),
            },
        ];
        let active = TabInfo {
            tab_id: 1,
            tab_title: "Tab A".into(),
            url: "https://a.com".into(),
            space: "Work".into(),
        };
        let ctx = Agent::<MockHostFunctions>::format_tab_context(Some(&active), &tabs);
        assert!(ctx.contains("[Work]"));
        assert!(ctx.contains("[Default]"));
        assert!(ctx.contains("current_tab: 1"));
        assert!(ctx.contains("\"Tab A\""));
        assert!(ctx.contains("https://a.com"));
    }

    #[test]
    fn test_format_tab_context_empty() {
        let ctx = Agent::<MockHostFunctions>::format_tab_context(None, &[]);
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_get_tools_for_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let chat_tools = agent.get_tools_for_mode(AgentMode::Chat);
        assert!(chat_tools.iter().any(|t| t.name == "web_search"));
        assert!(!chat_tools.iter().any(|t| t.name == "bash"));

        let browser_tools = agent.get_tools_for_mode(AgentMode::Browser);
        assert!(browser_tools.iter().any(|t| t.name == "browser_navigate"));

        let agent_tools = agent.get_tools_for_mode(AgentMode::Agent);
        assert!(agent_tools.iter().any(|t| t.name == "bash"));
        assert!(agent_tools.iter().any(|t| t.name == "subagent_spawn"));
    }

    #[test]
    fn test_orchestrate_tool_in_browser_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let browser_tools = agent.get_browser_tools();
        let orchestrate = browser_tools.iter().find(|t| t.name == "orchestrate");
        assert!(
            orchestrate.is_some(),
            "Browser mode should include orchestrate tool"
        );
        let desc = &orchestrate.unwrap().description;
        assert!(
            desc.contains("browser_navigate"),
            "Browser orchestrate should list browser tools"
        );
        assert!(
            !desc.contains("bash"),
            "Browser orchestrate should not list agent-only tools"
        );
    }

    #[test]
    fn test_orchestrate_tool_in_agent_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let agent_tools = agent.get_agent_tools();
        let orchestrate = agent_tools.iter().find(|t| t.name == "orchestrate");
        assert!(
            orchestrate.is_some(),
            "Agent mode should include orchestrate tool"
        );
        let desc = &orchestrate.unwrap().description;
        assert!(
            desc.contains("bash"),
            "Agent orchestrate should list bash tool"
        );
        assert!(
            desc.contains("async def"),
            "Should contain async def signatures"
        );
        assert!(
            desc.contains("[sequential]"),
            "Should contain sequential markers"
        );
    }

    #[test]
    fn test_orchestrate_excludes_meta_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let agent_tools = agent.get_agent_tools();
        let orchestrate = agent_tools
            .iter()
            .find(|t| t.name == "orchestrate")
            .unwrap();
        let desc = &orchestrate.description;
        assert!(!desc.contains("  orchestrate("), "Should not list itself");
        assert!(!desc.contains("  think("), "Should not list think tool");
        assert!(!desc.contains("  plan("), "Should not list plan tool");
        assert!(
            !desc.contains("  load_computer_use_tools("),
            "Should not list meta-tools"
        );
    }

    #[test]
    fn test_orchestrate_no_duplicate_in_agent_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let agent_tools = agent.get_agent_tools();
        let orchestrate_count = agent_tools
            .iter()
            .filter(|t| t.name == "orchestrate")
            .count();
        assert_eq!(
            orchestrate_count, 1,
            "Agent mode should have exactly 1 orchestrate tool (not duplicated from browser)"
        );
    }

    #[test]
    fn test_agent_run_chat() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        let output = agent.run(&input).unwrap();
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_agent_run_browser() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Browser,
            user_message: "Click the button".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        let output = agent.run(&input).unwrap();
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_agent_run_agent_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Agent,
            user_message: "List files".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        let output = agent.run(&input).unwrap();
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_agent_with_tool_calls() {
        let mock = MockHostFunctions::new();
        mock.add_llm_response(LlmResponse {
            text: "Let me search for that.".into(),
            tool_calls: vec![ToolCall {
                id: "call-001".into(),
                call_id: None,
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust programming"}),
                signature: None,
            }],
            reasoning: None,
        });
        mock.add_llm_response(LlmResponse {
            text: "Here's what I found about Rust.".into(),
            tool_calls: vec![],
            reasoning: None,
        });

        // Use non-streaming config since mock doesn't support streaming responses
        let config = AgentConfig {
            max_iterations: 100,
            use_streaming: false,
            suppress_streaming: false,
            is_subagent: false,
        };
        let agent = Agent::with_config(mock, config);
        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Tell me about Rust".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        let output = agent.run(&input).unwrap();
        assert_eq!(output.tool_calls.len(), 1);
        assert!(output.text.contains("Rust"));
    }

    #[test]
    fn test_static_prompts_content() {
        // Chat prompt should NOT mention interaction
        assert!(!CHAT_PROMPT.contains("browser_click"));

        // Browser prompt should contain interaction rules
        assert!(BROWSER_PROMPT.contains("element ID") || BROWSER_PROMPT.contains("[eN]"));

        // Agent prompt should contain tool strategy
        assert!(
            AGENT_PROMPT.contains("browser_get_markdown")
                || AGENT_PROMPT.contains("Tool selection")
        );

        // Subagent prompts
        assert!(SUBAGENT_BROWSER_PROMPT.contains("CANNOT interact"));
        assert!(SUBAGENT_AGENT_PROMPT.contains("sandbox"));
    }

    #[test]
    fn test_format_skill_summaries() {
        let skills = vec![
            SkillSummary {
                name: "code-review".into(),
                description: "Review code for issues".into(),
                tags: vec!["code".into()],
            },
            SkillSummary {
                name: "tdd".into(),
                description: "Test-driven development workflow".into(),
                tags: vec![],
            },
        ];

        let formatted = format_skill_summaries(&skills);
        assert!(formatted.contains("- **code-review**: Review code for issues"));
        assert!(formatted.contains("- **tdd**: Test-driven development workflow"));
        assert!(formatted.contains("\n")); // Multiple lines
    }

    #[test]
    fn test_format_skill_summaries_empty() {
        let skills: Vec<SkillSummary> = vec![];
        let formatted = format_skill_summaries(&skills);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_build_system_prompt_with_skills() {
        let skills = vec![SkillSummary {
            name: "web-tools".into(),
            description: "Web automation tools".into(),
            tags: vec![],
        }];
        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Chat,
            &skills,
            &[],
            ComputerUseFlags::default(),
            None,
        );
        assert!(prompt.contains("web-tools"));
        assert!(prompt.contains("# Skills"));
    }

    #[test]
    fn test_build_system_prompt_with_models() {
        let models = vec![("anthropic".into(), "claude-sonnet".into())];
        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Chat,
            &[],
            &models,
            ComputerUseFlags::default(),
            None,
        );
        assert!(prompt.contains("claude-sonnet"));
        assert!(prompt.contains("# Available models"));
    }

    #[test]
    fn test_build_system_prompt_no_extras() {
        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Chat,
            &[],
            &[],
            ComputerUseFlags::default(),
            None,
        );
        // Should be exactly the static prompt, no extras
        assert_eq!(prompt, CHAT_PROMPT);
        assert!(!prompt.contains("# Skills"));
        assert!(!prompt.contains("# Available models"));
    }

    // =========================================================================
    // Computer Use Tests
    // =========================================================================

    #[test]
    fn test_should_trigger_computer_use_english_keywords() {
        assert!(should_trigger_computer_use("Open the desktop app"));
        assert!(should_trigger_computer_use(
            "Take a screenshot of the screen"
        ));
        assert!(should_trigger_computer_use("Handle the file picker dialog"));
        assert!(should_trigger_computer_use("Click on the taskbar icon"));
        assert!(should_trigger_computer_use("Drag the file to the folder"));
        assert!(should_trigger_computer_use("Solve the captcha"));
    }

    #[test]
    fn test_should_trigger_computer_use_chinese_keywords() {
        assert!(should_trigger_computer_use("请在桌面上找到文件"));
        assert!(should_trigger_computer_use("处理上传文件的弹窗"));
        assert!(should_trigger_computer_use("拖拽这个图标"));
        assert!(should_trigger_computer_use("点击系统设置"));
        assert!(should_trigger_computer_use("解决验证码"));
    }

    #[test]
    fn test_should_not_trigger_computer_use_normal_messages() {
        assert!(!should_trigger_computer_use("Search for rust tutorials"));
        assert!(!should_trigger_computer_use("Fill in the login form"));
        assert!(!should_trigger_computer_use("Click the submit button"));
        assert!(!should_trigger_computer_use("Navigate to google.com"));
    }

    #[test]
    fn test_computer_use_flags_chat_mode() {
        let agent = Agent::new(MockHostFunctions::new());
        let flags = agent.computer_use_flags(AgentMode::Chat);
        assert!(!flags.inject_overview);
        assert!(!flags.inject_guide);
        assert!(!flags.inject_examples);
    }

    #[test]
    fn test_computer_use_flags_browser_default() {
        let agent = Agent::new(MockHostFunctions::new());
        let flags = agent.computer_use_flags(AgentMode::Browser);
        assert!(flags.inject_overview);
        assert!(!flags.inject_guide);
        assert!(!flags.inject_examples);
    }

    #[test]
    fn test_computer_use_flags_browser_triggered() {
        let agent = Agent::new(MockHostFunctions::new());
        agent.computer_use_triggered.set(true);
        let flags = agent.computer_use_flags(AgentMode::Browser);
        assert!(flags.inject_overview);
        assert!(flags.inject_guide);
        assert!(flags.inject_examples);
    }

    #[test]
    fn test_computer_use_flags_agent_default() {
        let agent = Agent::new(MockHostFunctions::new());
        let flags = agent.computer_use_flags(AgentMode::Agent);
        assert!(flags.inject_overview);
        assert!(flags.inject_guide);
        assert!(!flags.inject_examples);
    }

    #[test]
    fn test_computer_use_flags_agent_triggered() {
        let agent = Agent::new(MockHostFunctions::new());
        agent.computer_use_triggered.set(true);
        let flags = agent.computer_use_flags(AgentMode::Agent);
        assert!(flags.inject_overview);
        assert!(flags.inject_guide);
        assert!(flags.inject_examples);
    }

    #[test]
    fn test_build_system_prompt_with_computer_use_overview() {
        let flags = ComputerUseFlags {
            inject_overview: true,
            inject_guide: false,
            inject_examples: false,
        };
        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Browser,
            &[],
            &[],
            flags,
            None,
        );
        assert!(prompt.contains("# Computer Use"));
        assert!(prompt.contains("Browser Use"));
        assert!(!prompt.contains("# Computer Use Guide"));
        assert!(!prompt.contains("# Computer Use Examples"));
    }

    #[test]
    fn test_build_system_prompt_with_all_computer_use_layers() {
        let flags = ComputerUseFlags {
            inject_overview: true,
            inject_guide: true,
            inject_examples: true,
        };
        let prompt = Agent::<MockHostFunctions>::build_system_prompt(
            AgentMode::Agent,
            &[],
            &[],
            flags,
            None,
        );
        assert!(prompt.contains("# Computer Use"));
        assert!(prompt.contains("# Computer Use Guide"));
        assert!(prompt.contains("# Computer Use Examples"));
        assert!(prompt.contains("computer_drag"));
        assert!(prompt.contains("Observe Before Acting"));
    }

    #[test]
    fn test_is_browser_failure_suggesting_computer_use() {
        // Should trigger
        assert!(is_browser_failure_suggesting_computer_use(
            r#"{"success":false,"error":"Element not found - native file picker dialog detected"}"#
        ));
        assert!(is_browser_failure_suggesting_computer_use(
            r#"{"success":false,"error":"Cannot interact with file dialog"}"#
        ));
        assert!(is_browser_failure_suggesting_computer_use(
            r#"{"success":false,"error":"Element is not clickable - obscured by permission prompt"}"#
        ));
        // Should trigger: all click methods exhausted (OAuth/trusted gesture required)
        assert!(is_browser_failure_suggesting_computer_use(
            r#"{"success":true,"data":{"clickMethod":"all_tiers_exhausted","clicked":true,"domChanged":false,"effective":false,"element_id":"e4","networkRequestMade":false}}"#
        ));
        // Should NOT trigger (no error indicator)
        assert!(!is_browser_failure_suggesting_computer_use(
            r#"{"success":true,"data":"file picker button"}"#
        ));
        // Should NOT trigger (error but no matching pattern)
        assert!(!is_browser_failure_suggesting_computer_use(
            r#"{"success":false,"error":"Network timeout"}"#
        ));
    }

    #[test]
    fn test_agent_tools_include_new_computer_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let tools = agent.get_agent_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Verify all 12 computer tools are present in agent mode
        assert!(tool_names.contains(&"computer_screenshot"));
        assert!(tool_names.contains(&"computer_mouse_move"));
        assert!(tool_names.contains(&"computer_click"));
        assert!(tool_names.contains(&"computer_type_text"));
        assert!(tool_names.contains(&"computer_key"));
        assert!(tool_names.contains(&"computer_scroll"));
        assert!(tool_names.contains(&"computer_drag"));
        assert!(tool_names.contains(&"computer_cursor_position"));
        assert!(tool_names.contains(&"computer_mouse_down"));
        assert!(tool_names.contains(&"computer_mouse_up"));
        assert!(tool_names.contains(&"computer_hold_key"));
        assert!(tool_names.contains(&"computer_wait"));
    }

    #[test]
    fn test_browser_tools_include_load_computer_use_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let tools = agent.get_browser_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"load_computer_use_tools"));
    }

    #[test]
    fn test_computer_mouse_move_no_click_param() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let tools = agent.get_agent_tools();
        let mouse_move = tools
            .iter()
            .find(|t| t.name == "computer_mouse_move")
            .unwrap();
        let schema = &mouse_move.input_schema;
        // Verify click parameter has been removed
        assert!(schema["properties"]["click"].is_null());
        assert!(mouse_move.description.contains("without clicking"));
    }

    #[test]
    fn test_agent_get_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let chat_tools = agent.get_chat_tools();
        assert!(chat_tools.iter().any(|t| t.name == "web_search"));
        assert!(chat_tools.iter().any(|t| t.name == "ask_user"));

        let browser_tools = agent.get_browser_tools();
        assert!(browser_tools.len() > chat_tools.len());
        // Browser tools should include all 13 browser-specific tools
        assert!(browser_tools.iter().any(|t| t.name == "browser_navigate"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_click"));
        assert!(browser_tools
            .iter()
            .any(|t| t.name == "browser_click_by_id"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_type"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_type_by_id"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_fill"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_fill_by_id"));
        assert!(browser_tools
            .iter()
            .any(|t| t.name == "browser_get_content"));
        assert!(browser_tools
            .iter()
            .any(|t| t.name == "browser_get_markdown"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_screenshot"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_eval_js"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_scroll"));
        assert!(browser_tools.iter().any(|t| t.name == "browser_wait_for"));

        let agent_tools = agent.get_agent_tools();
        assert!(agent_tools.iter().any(|t| t.name == "bash"));
        assert!(agent_tools.iter().any(|t| t.name == "read"));
        assert!(agent_tools.iter().any(|t| t.name == "write"));
    }

    #[test]
    fn test_execute_tool_web_search() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "web_search".into(),
            arguments: serde_json::json!({"query": "test"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert_eq!(result.tool_call_id, "call-001");
    }

    #[test]
    fn test_execute_tool_unknown() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "unknown_tool".into(),
            arguments: serde_json::json!({}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.content.contains("Unknown tool"));
    }

    #[test]
    fn test_execute_tool_search() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "tool_search".into(),
            arguments: serde_json::json!({"query": "file", "max_results": 5}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        // Mock returns empty array
        assert!(result.content.contains("[]"));
    }

    #[test]
    fn test_execute_tool_call_dynamic() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "tool_call_dynamic".into(),
            arguments: serde_json::json!({
                "tool_name": "read_file",
                "arguments": r#"{"path": "/test.txt"}"#
            }),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("read_file"));
    }

    #[test]
    fn chat_mode_exposes_canvas_video_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tools = agent.get_chat_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"canvas_create_composition"),
            "missing canvas_create_composition; got {:?}",
            names
        );
        assert!(
            names.contains(&"canvas_render_video"),
            "missing canvas_render_video; got {:?}",
            names
        );

        let render = tools
            .iter()
            .find(|t| t.name == "canvas_render_video")
            .unwrap();
        assert!(
            render.description.to_lowercase().contains("sidebar"),
            "description must mention 'sidebar'; got: {}",
            render.description
        );
    }

    #[test]
    fn agent_mode_exposes_canvas_video_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let tools = agent.get_tools_for_mode(AgentMode::Agent);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"canvas_create_composition"),
            "agent mode missing canvas_create_composition; got {:?}",
            names
        );
        assert!(
            names.contains(&"canvas_render_video"),
            "agent mode missing canvas_render_video; got {:?}",
            names
        );
        assert!(
            names.contains(&"canvas_lint_composition"),
            "agent mode missing canvas_lint_composition; got {:?}",
            names
        );
    }

    #[test]
    fn test_agent_mode_tools_include_canvas_lint_composition() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        let tools = agent.get_tools_for_mode(AgentMode::Agent);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"canvas_lint_composition"),
            "agent mode missing canvas_lint_composition; got {:?}",
            names
        );
        let lint = tools
            .iter()
            .find(|t| t.name == "canvas_lint_composition")
            .unwrap();
        assert!(
            lint.description.to_lowercase().contains("lint"),
            "description must mention 'lint'; got: {}",
            lint.description
        );
        assert!(
            lint.description.to_lowercase().contains("composition"),
            "description must mention 'composition'; got: {}",
            lint.description
        );
    }

    #[test]
    fn test_agent_tools_include_dynamic_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let agent_tools = agent.get_agent_tools();
        assert!(agent_tools.iter().any(|t| t.name == "tool_search"));
        assert!(agent_tools.iter().any(|t| t.name == "tool_call_dynamic"));
    }

    #[test]
    fn test_execute_tool_browser_navigate() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_navigate".into(),
            arguments: serde_json::json!({"url": "https://example.com"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("success"));
    }

    #[test]
    fn test_execute_tool_browser_click() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_click".into(),
            arguments: serde_json::json!({"selector": "#submit-btn"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_computer_click_fallback_triggers() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "all_tiers_exhausted",
            "effective": false,
            "element_id": "e4",
            "screenBounds": { "x": 100.0, "y": 200.0, "width": 80.0, "height": 30.0 }
        }));
        let fallback = agent.try_computer_click_fallback(&result);
        assert!(fallback.is_some());
        let json: serde_json::Value = serde_json::from_str(&fallback.unwrap()).unwrap();
        assert_eq!(json["clickMethod"], "computer_click_fallback");
        assert_eq!(json["originalMethod"], "all_tiers_exhausted");
        // computerClickResult should be a parsed object, not a double-serialized string
        assert!(json["computerClickResult"].is_object());
        assert!(agent.computer_use_triggered.get());
    }

    #[test]
    fn test_computer_click_fallback_no_screen_bounds() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "all_tiers_exhausted",
            "effective": false,
            "element_id": "e4"
        }));
        assert!(agent.try_computer_click_fallback(&result).is_none());
        assert!(!agent.computer_use_triggered.get());
    }

    #[test]
    fn test_computer_click_fallback_effective_true() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "all_tiers_exhausted",
            "effective": true,
            "element_id": "e4",
            "screenBounds": { "x": 100.0, "y": 200.0, "width": 80.0, "height": 30.0 }
        }));
        assert!(agent.try_computer_click_fallback(&result).is_none());
    }

    #[test]
    fn test_computer_click_fallback_normal_click() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "dispatchEvent",
            "effective": true,
            "element_id": "e4",
            "screenBounds": { "x": 100.0, "y": 200.0, "width": 80.0, "height": 30.0 }
        }));
        assert!(agent.try_computer_click_fallback(&result).is_none());
    }

    #[test]
    fn test_computer_click_fallback_coordinates() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "all_tiers_exhausted",
            "effective": false,
            "screenBounds": { "x": 100.0, "y": 200.0, "width": 80.0, "height": 30.0 }
        }));
        let fallback = agent.try_computer_click_fallback(&result).unwrap();
        let json: serde_json::Value = serde_json::from_str(&fallback).unwrap();
        // Center: (100 + 80/2).round() = 140, (200 + 30/2).round() = 215
        assert_eq!(json["fallbackCoordinates"]["x"], 140);
        assert_eq!(json["fallbackCoordinates"]["y"], 215);
    }

    #[test]
    fn test_computer_click_fallback_zero_size_bounds() {
        let agent = Agent::new(MockHostFunctions::new());
        let result = BrowserToolResult::success(serde_json::json!({
            "clickMethod": "all_tiers_exhausted",
            "effective": false,
            "screenBounds": { "x": 100.0, "y": 200.0, "width": 0.0, "height": 0.0 }
        }));
        assert!(agent.try_computer_click_fallback(&result).is_none());
    }

    #[test]
    fn test_execute_tool_browser_type() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_type".into(),
            arguments: serde_json::json!({"selector": "#input", "text": "Hello World"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_tool_browser_screenshot() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_screenshot".into(),
            arguments: serde_json::json!({"full_page": true}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("screenshot"));
    }

    #[test]
    fn test_execute_tool_browser_scroll() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_scroll".into(),
            arguments: serde_json::json!({"direction": "down", "amount": "page"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_tool_browser_wait_for() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_wait_for".into(),
            arguments: serde_json::json!({"selector": "#loading", "timeout_ms": 5000}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_browser_tools_count() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let browser_tools = agent.get_browser_tools();
        let chat_tools = agent.get_chat_tools();
        // Browser tools = chat tools + 23 browser-specific tools
        // (15 browser interaction tools + 2 PR #2.5 strategy engine tools
        //  (browser_input + browser_probe) + 1 load_computer_use_tools
        //  meta-tool + 1 orchestrate + 2 MCP dynamic tools: tool_search,
        //  tool_call_dynamic + 2 additional browser-specific tools)
        // (browser_get_content, browser_get_markdown, browser_screenshot are
        //  already in chat tools)
        assert_eq!(browser_tools.len(), chat_tools.len() + 23);
    }

    #[test]
    fn test_agent_run_not_interrupted() {
        let mock = MockHostFunctions::new();
        // Default: not interrupted
        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        // Should complete normally
        let output = agent.run(&input).unwrap();
        assert!(!output.continue_loop);
    }

    #[test]
    fn test_agent_run_interrupted_before_llm_call() {
        // Create a mock that is interrupted immediately
        let mock = MockHostFunctions::new();
        mock.set_interrupted(true); // Interrupt immediately

        let agent = Agent::new(mock);

        let input = AgentInput {
            session_id: "sess-001".into(),
            mode: AgentMode::Chat,
            user_message: "Hello".into(),
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: None,
            tab_id: None,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config: None,
            os_platform: None,
        };

        // Should exit early due to interrupt
        let output = agent.run(&input).unwrap();
        // Output text should be empty because we never called LLM
        assert!(output.text.is_empty());
        assert!(output.tool_calls.is_empty());
    }

    #[test]
    fn test_agent_tools_include_subagent_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let agent_tools = agent.get_agent_tools();
        assert!(agent_tools.iter().any(|t| t.name == "subagent_spawn"));
        assert!(agent_tools.iter().any(|t| t.name == "subagent_status"));
        assert!(agent_tools.iter().any(|t| t.name == "subagent_wait"));
        assert!(agent_tools.iter().any(|t| t.name == "subagent_kill"));
        assert!(agent_tools.iter().any(|t| t.name == "subagent_list"));
    }

    #[test]
    fn test_execute_tool_subagent_spawn() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Search for files", "mode": "agent"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Spawned sub-agent with ID:"));
    }

    #[test]
    fn test_execute_tool_subagent_spawn_default_mode() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Do something"}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Spawned sub-agent"));
    }

    #[test]
    fn test_execute_tool_subagent_status() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // First spawn a subagent
        let spawn_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Test task"}),
            signature: None,
        };
        agent.execute_tool(&spawn_call).unwrap();

        // Then check its status
        let status_call = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "subagent_status".into(),
            arguments: serde_json::json!({"id": 1}),
            signature: None,
        };

        let result = agent.execute_tool(&status_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("status:"));
    }

    #[test]
    fn test_execute_tool_subagent_wait() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // First spawn a subagent
        let spawn_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Test task"}),
            signature: None,
        };
        agent.execute_tool(&spawn_call).unwrap();

        // Then wait for it
        let wait_call = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "subagent_wait".into(),
            arguments: serde_json::json!({"id": 1}),
            signature: None,
        };

        let result = agent.execute_tool(&wait_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Test task"));
    }

    #[test]
    fn test_execute_tool_subagent_kill() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // First spawn a subagent
        let spawn_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Long running task"}),
            signature: None,
        };
        agent.execute_tool(&spawn_call).unwrap();

        // Then kill it
        let kill_call = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "subagent_kill".into(),
            arguments: serde_json::json!({"id": 1}),
            signature: None,
        };

        let result = agent.execute_tool(&kill_call).unwrap();
        assert!(result.success);
        // Mock immediately completes, so it should say "already completed"
        assert!(result.content.contains("already completed"));
    }

    #[test]
    fn test_execute_tool_subagent_list() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // First spawn some subagents
        let spawn_call1 = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Task 1", "mode": "agent"}),
            signature: None,
        };
        agent.execute_tool(&spawn_call1).unwrap();

        let spawn_call2 = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Task 2", "mode": "browser"}),
            signature: None,
        };
        agent.execute_tool(&spawn_call2).unwrap();

        // Then list them
        let list_call = ToolCall {
            id: "call-003".into(),
            call_id: None,
            name: "subagent_list".into(),
            arguments: serde_json::json!({}),
            signature: None,
        };

        let result = agent.execute_tool(&list_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Task 1"));
        assert!(result.content.contains("Task 2"));
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1048576), "1.0 MB");
        assert_eq!(format_file_size(1073741824), "1.0 GB");
    }

    #[test]
    fn test_format_local_files_empty() {
        let result = format_local_files(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_local_files_single_file() {
        use nevoflux_protocol::LocalFileRef;
        let files = vec![LocalFileRef {
            path: "/home/user/test.rs".into(),
            is_directory: false,
            size: Some(2048),
            modified: None,
        }];
        let result = format_local_files(&files);
        assert!(result.contains("/home/user/test.rs"));
        assert!(result.contains("文件"));
        assert!(result.contains("2.0 KB"));
        assert!(result.contains("read_file"));
    }

    #[test]
    fn test_format_local_files_directory() {
        use nevoflux_protocol::LocalFileRef;
        let files = vec![LocalFileRef {
            path: "/home/user/project".into(),
            is_directory: true,
            size: None,
            modified: None,
        }];
        let result = format_local_files(&files);
        assert!(result.contains("/home/user/project"));
        assert!(result.contains("目录"));
        assert!(result.contains("list_directory"));
    }

    #[test]
    fn test_format_local_files_mixed() {
        use nevoflux_protocol::LocalFileRef;
        let files = vec![
            LocalFileRef {
                path: "/home/user/main.rs".into(),
                is_directory: false,
                size: Some(1024),
                modified: None,
            },
            LocalFileRef {
                path: "/home/user/src".into(),
                is_directory: true,
                size: None,
                modified: None,
            },
        ];
        let result = format_local_files(&files);
        assert!(result.contains("main.rs"));
        assert!(result.contains("/home/user/src"));
    }

    // ==================== Truncation Tests ====================

    #[test]
    fn test_calculate_messages_size() {
        let messages = vec![
            Message::system("System prompt"),
            Message::user("User message"),
        ];
        let size = calculate_messages_size(&messages);
        assert_eq!(size, "System prompt".len() + "User message".len());
    }

    #[test]
    fn test_truncate_tool_result_small_content() {
        // Small content should not be truncated
        let messages = vec![Message::system("System prompt")];
        let content = "Small content";
        let result = truncate_tool_result_if_needed(&messages, content);
        assert_eq!(result, content);
    }

    #[test]
    fn test_truncate_tool_result_large_content() {
        // Large content should be truncated
        let messages = vec![Message::system("System prompt")];
        // Create content larger than MAX_TOTAL_MESSAGE_SIZE
        let large_content = "x".repeat(400 * 1024); // 400KB
        let result = truncate_tool_result_if_needed(&messages, &large_content);

        // Should be truncated
        assert!(result.len() < large_content.len());
        assert!(result.contains("[Content truncated:"));
    }

    #[test]
    fn test_truncate_tool_result_respects_existing_messages() {
        // When messages already take up space, less space is available for tool result
        let large_system = "x".repeat(200 * 1024); // 200KB system message
        let messages = vec![Message::system(&large_system)];

        let content = "y".repeat(150 * 1024); // 150KB content
        let result = truncate_tool_result_if_needed(&messages, &content);

        // Should be truncated because total would exceed 300KB limit
        assert!(result.len() < content.len());
        assert!(result.contains("[Content truncated:"));
    }

    #[test]
    fn test_truncate_tool_result_minimum_size() {
        // Even when messages are very large, tool result should have minimum 10KB
        let huge_system = "x".repeat(290 * 1024); // 290KB - almost at limit
        let messages = vec![Message::system(&huge_system)];

        let content = "y".repeat(50 * 1024); // 50KB content
        let result = truncate_tool_result_if_needed(&messages, &content);

        // Should have at least 10KB (MIN_TOOL_RESULT_SIZE)
        assert!(result.len() >= 10 * 1024);
    }

    // =========================================================================
    // Elements caching tests
    // =========================================================================

    #[test]
    fn test_parse_elements_from_data_refs_format() {
        let data = serde_json::json!({
            "refs": {
                "e1": {"name": "Submit", "role": "button", "selectors": [{"type": "css", "strategy": "id", "value": "#submit"}], "rect": {"x": 10.0, "y": 20.0, "width": 100.0, "height": 30.0}},
                "e2": {"name": "Email", "role": "textbox", "selectors": [{"type": "css", "strategy": "css", "value": "input[name=email]"}]}
            }
        });
        let cache = parse_elements_from_data(&data).unwrap();
        assert_eq!(cache.elements.len(), 2);

        let e1 = cache.elements.iter().find(|e| e.id == "e1").unwrap();
        assert_eq!(e1.role, "button");
        assert_eq!(e1.name, "Submit");
        assert_eq!(e1.selector, "#submit");
        assert!(e1.rect.is_some());
        let rect = e1.rect.as_ref().unwrap();
        assert_eq!(rect.x, 10.0);
        assert_eq!(rect.width, 100.0);

        let e2 = cache.elements.iter().find(|e| e.id == "e2").unwrap();
        assert_eq!(e2.role, "textbox");
        assert_eq!(e2.selector, "input[name=email]");
        assert!(e2.rect.is_none());
    }

    #[test]
    fn test_parse_elements_from_data_elements_format() {
        let data = serde_json::json!({
            "elements": [
                {"id": "e1", "name": "Submit", "role": "button"},
                {"id": "e2", "name": "", "role": "generic"}
            ]
        });
        let cache = parse_elements_from_data(&data).unwrap();
        assert_eq!(cache.elements.len(), 2);
        assert_eq!(cache.elements[0].id, "e1");
        assert_eq!(cache.elements[1].role, "generic");
    }

    #[test]
    fn test_parse_elements_missing_refs() {
        let data = serde_json::json!({"other": "data"});
        assert!(parse_elements_from_data(&data).is_none());
    }

    #[test]
    fn test_parse_elements_empty_refs() {
        let data = serde_json::json!({"refs": {}});
        assert!(parse_elements_from_data(&data).is_none());
    }

    #[test]
    fn test_build_summary_interactive_elements() {
        let elements = vec![
            CachedElement {
                id: "e1".into(),
                role: "button".into(),
                name: "Submit".into(),
                selector: "#submit".into(),
                rect: Some(ElementRect {
                    x: 10.0,
                    y: 20.0,
                    width: 100.0,
                    height: 30.0,
                }),
                raw: serde_json::json!({}),
            },
            CachedElement {
                id: "e2".into(),
                role: "generic".into(),
                name: "Container".into(),
                selector: "div".into(),
                rect: None,
                raw: serde_json::json!({}),
            },
        ];
        let summary = build_elements_summary(&elements);
        let parsed: serde_json::Value = serde_json::from_str(&summary).unwrap();

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["element_count"], 2);
        // Button is interactive and named, should appear in interactive_elements
        assert_eq!(parsed["interactive_elements"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["interactive_elements"][0]["id"], "e1");
        // Generic is not interactive, should not appear
        assert_eq!(parsed["unnamed_interactive_count"], 0);
    }

    #[test]
    fn test_build_summary_unnamed_elements() {
        let elements = vec![CachedElement {
            id: "e1".into(),
            role: "button".into(),
            name: "".into(),
            selector: "button.icon".into(),
            rect: Some(ElementRect {
                x: 50.0,
                y: 60.0,
                width: 32.0,
                height: 32.0,
            }),
            raw: serde_json::json!({}),
        }];
        let summary = build_elements_summary(&elements);
        let parsed: serde_json::Value = serde_json::from_str(&summary).unwrap();

        assert_eq!(parsed["interactive_elements"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["unnamed_interactive_count"], 1);
        assert_eq!(parsed["unnamed_interactive_elements"][0]["id"], "e1");
        assert_eq!(parsed["unnamed_interactive_elements"][0]["name"], "");
    }

    #[test]
    fn test_build_summary_role_counts() {
        let elements = vec![
            CachedElement {
                id: "e1".into(),
                role: "button".into(),
                name: "A".into(),
                selector: "".into(),
                rect: None,
                raw: serde_json::json!({}),
            },
            CachedElement {
                id: "e2".into(),
                role: "button".into(),
                name: "B".into(),
                selector: "".into(),
                rect: None,
                raw: serde_json::json!({}),
            },
            CachedElement {
                id: "e3".into(),
                role: "link".into(),
                name: "C".into(),
                selector: "".into(),
                rect: None,
                raw: serde_json::json!({}),
            },
        ];
        let summary = build_elements_summary(&elements);
        let parsed: serde_json::Value = serde_json::from_str(&summary).unwrap();

        assert_eq!(parsed["roles"]["button"], 2);
        assert_eq!(parsed["roles"]["link"], 1);
    }

    #[test]
    fn test_find_elements_by_role() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // Populate cache
        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![
                CachedElement {
                    id: "e1".into(),
                    role: "button".into(),
                    name: "Submit".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e2".into(),
                    role: "textbox".into(),
                    name: "Email".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e3".into(),
                    role: "button".into(),
                    name: "Cancel".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
            ],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"role": "button"}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["count"], 2);
    }

    #[test]
    fn test_find_elements_by_name() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![
                CachedElement {
                    id: "e1".into(),
                    role: "button".into(),
                    name: "Submit Form".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e2".into(),
                    role: "button".into(),
                    name: "Cancel".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
            ],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"name": "submit"}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["elements"][0]["id"], "e1");
    }

    #[test]
    fn test_find_elements_by_position() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![
                CachedElement {
                    id: "e1".into(),
                    role: "button".into(),
                    name: "Near".into(),
                    selector: "".into(),
                    rect: Some(ElementRect {
                        x: 100.0,
                        y: 100.0,
                        width: 20.0,
                        height: 20.0,
                    }),
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e2".into(),
                    role: "button".into(),
                    name: "Far".into(),
                    selector: "".into(),
                    rect: Some(ElementRect {
                        x: 500.0,
                        y: 500.0,
                        width: 20.0,
                        height: 20.0,
                    }),
                    raw: serde_json::json!({}),
                },
            ],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"near_x": 115, "near_y": 115, "radius": 50}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["elements"][0]["id"], "e1");
    }

    #[test]
    fn test_find_elements_unnamed_only() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![
                CachedElement {
                    id: "e1".into(),
                    role: "button".into(),
                    name: "Submit".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e2".into(),
                    role: "button".into(),
                    name: "".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
            ],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"unnamed_only": true}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["elements"][0]["id"], "e2");
    }

    #[test]
    fn test_find_elements_combined_filters() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![
                CachedElement {
                    id: "e1".into(),
                    role: "button".into(),
                    name: "Submit".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e2".into(),
                    role: "link".into(),
                    name: "Submit Link".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
                CachedElement {
                    id: "e3".into(),
                    role: "button".into(),
                    name: "Cancel".into(),
                    selector: "".into(),
                    rect: None,
                    raw: serde_json::json!({}),
                },
            ],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"role": "button", "name": "submit"}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["elements"][0]["id"], "e1");
    }

    #[test]
    fn test_element_info_found() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let raw = serde_json::json!({"name": "Submit", "role": "button", "extra_field": "value"});
        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![CachedElement {
                id: "e42".into(),
                role: "button".into(),
                name: "Submit".into(),
                selector: "".into(),
                rect: None,
                raw: raw.clone(),
            }],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_element_info".into(),
            arguments: serde_json::json!({"id": "e42"}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["element"]["extra_field"], "value");
    }

    #[test]
    fn test_element_info_not_found() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        *agent.elements_cache.borrow_mut() = Some(ElementsCache {
            elements: vec![CachedElement {
                id: "e1".into(),
                role: "button".into(),
                name: "X".into(),
                selector: "".into(),
                rect: None,
                raw: serde_json::json!({}),
            }],
        });

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_element_info".into(),
            arguments: serde_json::json!({"id": "e999"}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.content.contains("not found"));
    }

    #[test]
    fn test_cache_empty_error() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);
        // Don't populate cache

        let find_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_find_elements".into(),
            arguments: serde_json::json!({"role": "button"}),
            signature: None,
        };
        let result = agent.execute_tool(&find_call).unwrap();
        assert!(result.content.contains("No elements cached"));

        let info_call = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "browser_element_info".into(),
            arguments: serde_json::json!({"id": "e1"}),
            signature: None,
        };
        let result = agent.execute_tool(&info_call).unwrap();
        assert!(result.content.contains("No elements cached"));
    }

    #[test]
    fn test_screenshot_cached_and_stripped() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_screenshot".into(),
            arguments: serde_json::json!({"full_page": false}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();

        // Content should NOT contain base64 data
        assert!(result.content.contains("screenshot_available"));
        assert!(!result.content.contains("iVBOR"));

        // Screenshot should be in cache
        let cached = agent.screenshot_cache.borrow();
        assert!(cached.is_some());
        assert!(cached.as_ref().unwrap().starts_with("iVBOR"));
    }

    #[test]
    fn test_browser_get_elements_caches() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "browser_get_elements".into(),
            arguments: serde_json::json!({}),
            signature: None,
        };
        let result = agent.execute_tool(&tool_call).unwrap();

        // Mock returns refs map format with selectors array
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["element_count"], 2);

        // Cache should be populated
        let cache = agent.elements_cache.borrow();
        assert!(cache.is_some());
        assert_eq!(cache.as_ref().unwrap().elements.len(), 2);
    }

    #[test]
    fn test_browser_tools_include_companion_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let browser_tools = agent.get_browser_tools();
        assert!(browser_tools
            .iter()
            .any(|t| t.name == "browser_find_elements"));
        assert!(browser_tools
            .iter()
            .any(|t| t.name == "browser_element_info"));
    }

    #[test]
    fn test_switch_model_tool_registered_in_all_modes() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        for mode in [
            AgentMode::Chat,
            AgentMode::Browser,
            AgentMode::Agent,
            AgentMode::Code,
        ] {
            let tools = agent.get_tools_for_mode(mode);
            let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
            assert!(
                tool_names.contains(&"switch_model"),
                "switch_model should be registered in {:?} mode",
                mode
            );
        }
    }

    #[test]
    fn test_switch_model_tool_schema() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tools = agent.get_tools_for_mode(AgentMode::Chat);
        let switch_model = tools.iter().find(|t| t.name == "switch_model").unwrap();

        // Verify it requires provider and model
        let required = switch_model.input_schema["required"].as_array().unwrap();
        let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_names.contains(&"provider"));
        assert!(required_names.contains(&"model"));
    }

    #[test]
    fn test_switch_model_handler_empty_args() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "switch_model".into(),
            arguments: serde_json::json!({"provider": "", "model": ""}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.content.contains("error"));
        assert!(result.content.contains("required"));
    }

    #[test]
    fn test_execute_tool_subagent_spawn_with_tab_id() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tool_call = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Read page", "mode": "browser", "tab_id": 42}),
            signature: None,
        };

        let result = agent.execute_tool(&tool_call).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Spawned sub-agent with ID:"));
    }

    #[test]
    fn test_execute_tool_subagent_wait_all() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // Spawn two subagents
        let spawn1 = ToolCall {
            id: "call-001".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Task A"}),
            signature: None,
        };
        agent.execute_tool(&spawn1).unwrap();

        let spawn2 = ToolCall {
            id: "call-002".into(),
            call_id: None,
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({"task": "Task B"}),
            signature: None,
        };
        agent.execute_tool(&spawn2).unwrap();

        // Wait for both
        let wait_all = ToolCall {
            id: "call-003".into(),
            call_id: None,
            name: "subagent_wait_all".into(),
            arguments: serde_json::json!({"ids": [1, 2]}),
            signature: None,
        };

        let result = agent.execute_tool(&wait_all).unwrap();
        assert!(result.success);
        assert!(result.content.contains("Task A"));
        assert!(result.content.contains("Task B"));
    }

    #[test]
    fn test_subagent_browser_read_tools() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let read_tools = agent.get_subagent_browser_read_tools();
        let chat_tools = agent.get_chat_tools();

        // Subagent browser read tools should be the same as chat tools
        // (which already include browser_get_content, browser_get_markdown, browser_screenshot)
        assert_eq!(read_tools.len(), chat_tools.len());

        // Verify read-only browser tools are present
        assert!(read_tools.iter().any(|t| t.name == "browser_get_content"));
        assert!(read_tools.iter().any(|t| t.name == "browser_get_markdown"));
        assert!(read_tools.iter().any(|t| t.name == "browser_screenshot"));

        // Verify interaction tools are NOT present
        assert!(!read_tools.iter().any(|t| t.name == "browser_click"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_click_by_id"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_type"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_fill"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_scroll"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_navigate"));
        assert!(!read_tools.iter().any(|t| t.name == "browser_eval_js"));
    }

    #[test]
    fn test_subagent_tools_for_mode_browser() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tools = agent.get_subagent_tools_for_mode(AgentMode::Browser);
        // Should be read-only browser tools (same as chat tools)
        assert!(tools.iter().any(|t| t.name == "browser_get_markdown"));
        assert!(!tools.iter().any(|t| t.name == "browser_click"));
        assert!(!tools.iter().any(|t| t.name == "read")); // No file tools in browser mode
    }

    #[test]
    fn test_subagent_tools_for_mode_agent() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let tools = agent.get_subagent_tools_for_mode(AgentMode::Agent);
        // Should have read-only browser tools + file tools
        assert!(tools.iter().any(|t| t.name == "browser_get_markdown"));
        assert!(tools.iter().any(|t| t.name == "read"));
        assert!(tools.iter().any(|t| t.name == "write"));
        assert!(tools.iter().any(|t| t.name == "bash"));
        assert!(tools.iter().any(|t| t.name == "grep"));
        // But no browser interaction tools
        assert!(!tools.iter().any(|t| t.name == "browser_click"));
        assert!(!tools.iter().any(|t| t.name == "browser_navigate"));
    }

    #[test]
    fn test_agent_config_for_subagent_is_subagent() {
        let config = AgentConfig::for_subagent();
        assert!(config.is_subagent);
        assert!(!config.use_streaming);
        assert!(config.suppress_streaming);
    }

    #[test]
    fn test_agent_tools_include_subagent_wait_all() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let agent_tools = agent.get_agent_tools();
        assert!(agent_tools.iter().any(|t| t.name == "subagent_wait_all"));
    }

    #[test]
    fn test_subagent_spawn_schema_has_tab_id() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let agent_tools = agent.get_agent_tools();
        let spawn_tool = agent_tools
            .iter()
            .find(|t| t.name == "subagent_spawn")
            .unwrap();
        let props = spawn_tool.input_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("tab_id"));
    }

    // ---- parse_tool_calls_from_text tests ----

    #[test]
    fn test_parse_tool_calls_plain_text_only() {
        let (cleaned, calls) = parse_tool_calls_from_text("Hello world");
        assert_eq!(cleaned, "Hello world");
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_extracts_valid_call() {
        let input =
            r#"Before <tool_call>{"id":"c1","name":"foo","arguments":{"x":1}}</tool_call> After"#;
        let (cleaned, calls) = parse_tool_calls_from_text(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "foo");
        // Text before the tag is kept
        assert!(cleaned.contains("Before"));
        // Trailing text after extracted tool call is discarded
        assert!(!cleaned.contains("After"));
    }

    #[test]
    fn test_parse_tool_calls_discards_hallucinated_tool_result() {
        // Simulates LLM hallucinating both a tool_call and a tool_result
        let input = concat!(
            "<tool_call>{\"id\":\"c1\",\"name\":\"bar\",\"arguments\":{}}</tool_call>\n",
            "[tool_result call_id=\"c1\"]\n",
            "{\"output\":\"fake\"}\n",
            "Here is the summary based on the results."
        );
        let (cleaned, calls) = parse_tool_calls_from_text(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bar");
        // All trailing text (hallucinated result + explanation) must be discarded
        assert!(!cleaned.contains("fake"));
        assert!(!cleaned.contains("summary"));
    }

    #[test]
    fn test_parse_tool_calls_no_closing_tag() {
        let input = "Text <tool_call>{\"id\":\"c1\",\"name\":\"x\",\"arguments\":{}}";
        let (cleaned, calls) = parse_tool_calls_from_text(input);
        // No closing tag — keep everything as-is
        assert!(calls.is_empty());
        assert!(cleaned.contains("Text"));
        assert!(cleaned.contains("<tool_call>"));
    }

    #[test]
    fn test_parse_tool_calls_malformed_json() {
        let input = "<tool_call>NOT JSON</tool_call> trailing";
        let (cleaned, calls) = parse_tool_calls_from_text(input);
        assert!(calls.is_empty());
        // Malformed JSON kept as raw text, trailing text also kept (no calls extracted)
        assert!(cleaned.contains("NOT JSON"));
        assert!(cleaned.contains("trailing"));
    }

    // ========================================================================
    // Tool filtering tests
    // ========================================================================

    #[test]
    fn test_tool_filter_allow_list() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        // Get the full browser tool set
        let all_tools = agent.get_browser_tools();
        assert!(!all_tools.is_empty(), "Browser mode should have tools");

        // Filter with browser_* wildcard
        let config = Some(ToolsConfig::Allow(vec!["browser_*".to_string()]));
        let filtered = agent.filter_tools(all_tools.clone(), &config);

        // All filtered tools should start with "browser_"
        assert!(!filtered.is_empty(), "Should have some browser tools");
        for tool in &filtered {
            assert!(
                tool.name.starts_with("browser_"),
                "Tool '{}' should start with 'browser_'",
                tool.name
            );
        }

        // Should have fewer tools than the full set (which includes non-browser tools)
        assert!(
            filtered.len() <= all_tools.len(),
            "Filtered set should be <= full set"
        );
    }

    #[test]
    fn test_tool_filter_none() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let all_tools = agent.get_agent_tools();
        assert!(!all_tools.is_empty(), "Agent mode should have tools");

        // Filter with ToolsConfig::None should return empty vec
        let config = Some(ToolsConfig::None);
        let filtered = agent.filter_tools(all_tools, &config);

        assert!(
            filtered.is_empty(),
            "ToolsConfig::None should disable all tools"
        );
    }

    #[test]
    fn test_tool_filter_inherit() {
        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let all_tools = agent.get_agent_tools();
        let tool_count = all_tools.len();
        assert!(tool_count > 0, "Agent mode should have tools");

        // Filter with None (inherit) should return the full set
        let config: Option<nevoflux_protocol::subagent::ToolsConfig> = None;
        let filtered = agent.filter_tools(all_tools, &config);

        assert_eq!(
            filtered.len(),
            tool_count,
            "Inherit (None) should return full tool set"
        );
    }

    #[test]
    fn test_tool_filter_allow_multiple_patterns() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let all_tools = agent.get_agent_tools();

        // Filter with specific tools + wildcard
        let config = Some(ToolsConfig::Allow(vec![
            "browser_*".to_string(),
            "read".to_string(),
        ]));
        let filtered = agent.filter_tools(all_tools, &config);

        // Should include browser tools and read
        let has_browser = filtered.iter().any(|t| t.name.starts_with("browser_"));
        let has_read = filtered.iter().any(|t| t.name == "read");
        assert!(has_browser, "Should include browser tools");
        assert!(has_read, "Should include read tool");

        // Should not include tools outside the allowlist
        let has_bash = filtered.iter().any(|t| t.name == "bash");
        assert!(!has_bash, "Should not include bash tool");
    }

    #[test]
    fn test_tool_filter_allow_empty_list() {
        use nevoflux_protocol::subagent::ToolsConfig;

        let mock = MockHostFunctions::new();
        let agent = Agent::new(mock);

        let all_tools = agent.get_chat_tools();
        assert!(!all_tools.is_empty());

        // Empty allowlist means nothing matches
        let config = Some(ToolsConfig::Allow(vec![]));
        let filtered = agent.filter_tools(all_tools, &config);

        assert!(filtered.is_empty(), "Empty allowlist should match no tools");
    }
}
