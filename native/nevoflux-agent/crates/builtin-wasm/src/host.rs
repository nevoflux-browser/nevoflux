//! Host function bindings.
//!
//! These are the external functions provided by the Wasmtime host.
//! When compiled for wasm32-wasi, these become actual imports.
//! For native testing, we provide mock implementations.

use crate::types::*;

/// Result type for host function calls.
pub type HostResult<T> = Result<T, HostError>;

/// Error from host function calls.
#[derive(Debug, Clone)]
pub struct HostError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Host error ({}): {}", self.code, self.message)
    }
}

impl std::error::Error for HostError {}

/// Host function interface.
///
/// This trait defines all host functions available to the Wasm guest.
/// The actual implementation is provided by the Wasmtime host at runtime.
pub trait HostFunctions {
    // =========================================================================
    // LLM Functions
    // =========================================================================

    /// Send a chat request to the LLM.
    fn llm_chat(&self, request: &LlmRequest) -> HostResult<LlmResponse>;

    /// Start a streaming chat request.
    fn llm_stream_start(&self, request: &LlmRequest) -> HostResult<u64>;

    /// Read the next chunk from a stream.
    fn llm_stream_next(&self, stream_id: u64) -> HostResult<Option<LlmChunk>>;

    /// Close a stream.
    fn llm_stream_close(&self, stream_id: u64) -> HostResult<()>;

    // =========================================================================
    // Memory Functions
    // =========================================================================

    /// Search memory.
    fn memory_search(&self, query: &str, limit: usize) -> HostResult<Vec<MemoryChunk>>;

    /// Create a memory chunk.
    fn memory_create(&self, content: &str, metadata: &serde_json::Value) -> HostResult<String>;

    /// Update a memory chunk.
    fn memory_update(&self, id: &str, content: &str) -> HostResult<()>;

    /// Delete a memory chunk.
    fn memory_delete(&self, id: &str) -> HostResult<()>;

    // =========================================================================
    // Knowledge Functions
    // =========================================================================

    /// Teach/record knowledge explicitly provided by the user.
    ///
    /// Creates a validated, hot knowledge entry that is immediately included
    /// in the system prompt's Layer 1 (learned knowledge).
    fn knowledge_teach(
        &self,
        category: &str,
        summary: &str,
        details: &str,
        domain: Option<&str>,
    ) -> HostResult<String>;

    /// List hot knowledge entries (active memories).
    fn memory_view(&self, limit: usize) -> HostResult<Vec<crate::types::KnowledgeViewEntry>>;

    // =========================================================================
    // Skills Functions
    // =========================================================================

    /// List available skills (Level 1 loading).
    fn skill_list(&self) -> HostResult<Vec<SkillSummary>>;

    /// Load a skill's full content (Level 2 loading).
    fn skill_load(&self, name: &str) -> HostResult<String>;

    /// Read skill auxiliary files (Level 3 loading).
    fn skill_read(&self, name: &str, path: &str) -> HostResult<String>;

    /// Execute a skill script (Level 3 loading).
    fn skill_execute(
        &self,
        name: &str,
        script: &str,
        args: &serde_json::Value,
    ) -> HostResult<String>;

    // =========================================================================
    // Built-in Tools
    // =========================================================================

    /// Read a file.
    fn tool_read(
        &self,
        path: &str,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> HostResult<ReadResult>;

    /// Write a file.
    fn tool_write(&self, path: &str, content: &str) -> HostResult<()>;

    /// Edit a file (search and replace).
    fn tool_edit(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> HostResult<()>;

    /// Execute a bash command.
    fn tool_bash(&self, command: &str, timeout_ms: Option<u64>) -> HostResult<BashResult>;

    /// Glob file patterns.
    fn tool_glob(&self, pattern: &str, path: Option<&str>) -> HostResult<Vec<String>>;

    /// Search file contents.
    fn tool_grep(
        &self,
        pattern: &str,
        path: Option<&str>,
        file_type: Option<&str>,
        case_insensitive: Option<bool>,
        max_results: Option<u64>,
    ) -> HostResult<GrepResult>;

    /// Web search.
    fn tool_web_search(&self, query: &str) -> HostResult<String>;

    /// Fetch a URL.
    fn tool_web_fetch(&self, url: &str, prompt: &str) -> HostResult<String>;

    /// Ask user a question.
    fn tool_ask_user(&self, question: &str, options: &[String]) -> HostResult<String>;

    // =========================================================================
    // Permission Functions
    // =========================================================================

    /// Request permission for an action.
    fn permission_request(
        &self,
        resource_type: &str,
        action: &str,
        resource: &str,
    ) -> HostResult<bool>;

    /// Check if permission is already granted.
    fn permission_check(
        &self,
        resource_type: &str,
        action: &str,
        resource: &str,
    ) -> HostResult<bool>;

    // =========================================================================
    // Dynamic Tool Functions
    // =========================================================================

    /// Search available tools by keyword using BM25 ranking.
    fn tool_search(&self, query: &str, max_results: usize) -> HostResult<Vec<ToolSearchResult>>;

    /// Call a dynamically discovered tool by name.
    fn tool_call_dynamic(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> HostResult<String>;

    // =========================================================================
    // Built-in Proxy (for plugins to inherit built-in capabilities)
    // =========================================================================

    /// Invoke built-in chat mode.
    fn builtin_chat(&self, input: &AgentInput) -> HostResult<AgentOutput>;

    /// Invoke built-in browser mode.
    fn builtin_browser(&self, input: &AgentInput) -> HostResult<AgentOutput>;

    /// Invoke built-in agent mode.
    fn builtin_agent(&self, input: &AgentInput) -> HostResult<AgentOutput>;

    // =========================================================================
    // Computer Tools
    // =========================================================================

    /// Take a screenshot of the screen.
    fn computer_screenshot(&self, monitor: Option<i64>) -> HostResult<String>;

    /// Move mouse cursor to position (pure movement, no click).
    fn computer_mouse_move(&self, x: i64, y: i64) -> HostResult<String>;

    /// Drag from one position to another.
    fn computer_drag(
        &self,
        start_x: i64,
        start_y: i64,
        end_x: i64,
        end_y: i64,
        button: Option<&str>,
    ) -> HostResult<String>;

    /// Get the current mouse cursor position.
    fn computer_cursor_position(&self) -> HostResult<String>;

    /// Press and hold a mouse button at a position.
    fn computer_mouse_down(&self, x: i64, y: i64, button: Option<&str>) -> HostResult<String>;

    /// Release a mouse button at a position.
    fn computer_mouse_up(&self, x: i64, y: i64, button: Option<&str>) -> HostResult<String>;

    /// Hold a key down for a specified duration.
    fn computer_hold_key(
        &self,
        key: &str,
        duration_ms: u64,
        modifiers: &[String],
    ) -> HostResult<String>;

    /// Wait for a specified duration (100-10000ms).
    fn computer_wait(&self, ms: u64) -> HostResult<String>;

    /// Click at screen position.
    fn computer_click(
        &self,
        x: i64,
        y: i64,
        button: Option<&str>,
        click_type: Option<&str>,
    ) -> HostResult<String>;

    /// Type text at current cursor position.
    fn computer_type_text(&self, text: &str, delay_ms: Option<u64>) -> HostResult<String>;

    /// Press keyboard key or combination.
    fn computer_key(
        &self,
        key: &str,
        modifiers: &[String],
        repeat: Option<u64>,
    ) -> HostResult<String>;

    /// Scroll at screen position.
    fn computer_scroll(
        &self,
        x: i64,
        y: i64,
        direction: &str,
        amount: Option<u64>,
    ) -> HostResult<String>;

    // =========================================================================
    // Browser Tools
    // =========================================================================

    /// Navigate to a URL.
    fn browser_navigate(&self, url: &str, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Go back in browser history.
    fn browser_go_back(&self, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Go forward in browser history.
    fn browser_go_forward(&self, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Click on an element by CSS selector.
    fn browser_click(&self, selector: &str, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Click on an element by element ID attribute.
    fn browser_click_by_id(
        &self,
        element_id: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Type text into an element (simulates keystrokes).
    fn browser_type(
        &self,
        selector: &str,
        text: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Type text into an element by ID (simulates keystrokes).
    fn browser_type_by_id(
        &self,
        element_id: &str,
        text: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Fill an input element with a value (sets value directly).
    fn browser_fill(
        &self,
        selector: &str,
        value: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Fill an input element by ID with a value.
    fn browser_fill_by_id(
        &self,
        element_id: &str,
        value: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Get page content as text/HTML.
    fn browser_get_content(&self, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Get page content as markdown.
    fn browser_get_markdown(&self, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Take a screenshot of the page.
    fn browser_screenshot(
        &self,
        full_page: bool,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Execute JavaScript in the page context.
    fn browser_eval_js(&self, script: &str, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Scroll the page.
    fn browser_scroll(
        &self,
        direction: &str,
        amount: &str,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Wait for an element to appear.
    fn browser_wait_for(
        &self,
        selector: &str,
        timeout_ms: u64,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Get accessibility tree with element IDs for interaction.
    fn browser_get_elements(
        &self,
        tab_id: Option<i64>,
        keywords: Option<Vec<String>>,
    ) -> HostResult<BrowserToolResult>;

    /// List all open browser tabs.
    fn browser_list_tabs(&self, tab_id: Option<i64>) -> HostResult<BrowserToolResult>;

    /// Query tabs with optional filters (url, title, active).
    fn browser_query_tabs(
        &self,
        params: &serde_json::Value,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Read canvas artifact source code.
    fn browser_read_artifact(
        &self,
        params: &serde_json::Value,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Edit canvas artifact using search-and-replace.
    fn browser_edit_artifact(
        &self,
        params: &serde_json::Value,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Extract a brand's visual identity (colors / fonts / logo / hero
    /// screenshot / name / tagline) from a URL or existing tab. Backs the
    /// `canvas_extract_visual_identity` tool — used by Mode 3 (website-to-
    /// video) to auto-fill DESIGN.md from a live site. See
    /// `nevoflux_protocol::extract::ExtractVisualIdentityRequest`.
    ///
    /// `tab_id` is supplied by the LLM dispatch arm when `target.tab_id` is
    /// present in the arguments; otherwise None and the extension opens a
    /// background tab from `target.url`.
    fn browser_extract_visual_identity(
        &self,
        params: &serde_json::Value,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Wait for page to stabilize after an action.
    fn browser_wait_for_stable(
        &self,
        strategy: &str,
        max_wait: u64,
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    /// Take a viewport-only snapshot (returns flat list of visible interactive elements).
    fn browser_viewport_snapshot(
        &self,
        tab_id: Option<i64>,
        keywords: Option<Vec<String>>,
    ) -> HostResult<BrowserToolResult>;

    /// Press a keyboard key.
    fn browser_key_press(
        &self,
        key: &str,
        modifiers: &[String],
        tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult>;

    // =========================================================================
    // Session Control Functions
    // =========================================================================

    /// Check if the current session has been interrupted (e.g., user clicked stop).
    ///
    /// This is used by the agent loop to check for user-requested interrupts
    /// and gracefully stop execution.
    fn is_interrupted(&self) -> HostResult<bool>;

    // =========================================================================
    // Subagent Functions
    // =========================================================================

    /// Spawn a sub-agent to execute a task.
    ///
    /// # Arguments
    /// * `task` - The task description for the sub-agent
    /// * `mode` - Execution mode: "chat", "browser", or "agent"
    /// * `tab_id` - Optional tab ID for the sub-agent to read page content from
    ///
    /// # Returns
    /// The sub-agent ID on success.
    fn subagent_spawn(&self, task: &str, mode: &str, tab_id: Option<i64>) -> HostResult<u64>;

    /// Wait for multiple sub-agents to complete and return all results.
    ///
    /// # Arguments
    /// * `ids` - The sub-agent IDs to wait for
    ///
    /// # Returns
    /// A JSON string with the results of all sub-agents.
    fn subagent_wait_all(&self, ids: &[u64]) -> HostResult<String>;

    /// Check the status of a sub-agent.
    ///
    /// # Arguments
    /// * `id` - The sub-agent ID
    ///
    /// # Returns
    /// The status string: "running", "completed", or "failed".
    fn subagent_status(&self, id: u64) -> HostResult<String>;

    /// Wait for a sub-agent to complete and get its result.
    ///
    /// # Arguments
    /// * `id` - The sub-agent ID
    ///
    /// # Returns
    /// The sub-agent's result text on completion.
    fn subagent_wait(&self, id: u64) -> HostResult<String>;

    /// Terminate a sub-agent.
    ///
    /// # Arguments
    /// * `id` - The sub-agent ID
    ///
    /// # Returns
    /// `true` if the sub-agent was terminated, `false` if it was already completed.
    fn subagent_kill(&self, id: u64) -> HostResult<bool>;

    /// List all sub-agents.
    ///
    /// # Returns
    /// A list of sub-agent information.
    fn subagent_list(&self) -> HostResult<Vec<SubagentInfo>>;

    /// List available agent roles for subagent spawning.
    ///
    /// Returns a JSON string of `Vec<AgentRoleSummary>` for WASM ABI compatibility.
    /// Returns an empty list if no role registry is configured.
    fn list_agents(&self) -> HostResult<String>;

    // =========================================================================
    // Streaming Functions
    // =========================================================================

    /// Emit a streaming chunk to the sidebar.
    ///
    /// This function sends incremental text content to the sidebar in real-time
    /// as the LLM generates it. Use this within the agent loop when processing
    /// streaming LLM responses.
    ///
    /// # Arguments
    /// * `text` - The incremental text content to send
    ///
    /// # Returns
    /// `Ok(())` on success.
    fn stream_emit(&self, text: &str) -> HostResult<()>;

    /// Signal end of streaming.
    ///
    /// This function signals the sidebar that streaming has completed.
    /// Call this after all chunks have been sent.
    ///
    /// # Returns
    /// `Ok(())` on success.
    fn stream_end(&self) -> HostResult<()>;

    // =========================================================================
    // Trace Functions
    // =========================================================================

    /// Update the current iteration counter for trace recording.
    ///
    /// Called by the agent loop after incrementing its iteration count so the
    /// host can associate LLM call traces with the correct iteration number.
    fn set_iteration(&self, iteration: u32) -> HostResult<()>;

    /// Override the active LLM provider and model for subsequent calls.
    fn set_model_override(&self, provider: &str, model: &str) -> HostResult<()>;

    // =========================================================================
    // Canvas Video (non-blocking render pipeline)
    // =========================================================================

    /// Create a composition artifact for video rendering. Returns the
    /// new artifact_id immediately; the composition HTML is stored
    /// daemon-side so the render page can later fetch it.
    ///
    /// Args/returns are `serde_json::Value` to keep `builtin-wasm`
    /// independent of `nevoflux-protocol::canvas_video`. The daemon-side
    /// impl deserializes into the typed request/response structs.
    fn canvas_video_create_composition(
        &self,
        request: &serde_json::Value,
    ) -> HostResult<serde_json::Value>;

    /// Non-blocking render start. Returns the new `job_id` immediately;
    /// progress and terminal state are observed by the sidebar via
    /// EventBus on `jobs:render:{job_id}`. The caller does NOT wait for
    /// render completion.
    fn canvas_video_render_start(
        &self,
        request: &serde_json::Value,
    ) -> HostResult<serde_json::Value>;

    /// Lint an existing composition. Returns a `LintReport` (serialized to
    /// `serde_json::Value`) or a `HostError` on timeout / invalid id.
    fn canvas_video_lint_composition(
        &self,
        request: &serde_json::Value,
    ) -> HostResult<serde_json::Value>;

    /// Re-inject DESIGN.md tokens into the composition's `index.html`.
    /// Idempotent and non-destructive — only the marked
    /// `<style data-nf-design-tokens>` block changes; LLM-edited content
    /// (text placeholders, copy, custom CSS outside the block) survives
    /// unmodified. Default implementation returns `Unsupported` so hosts
    /// that haven't wired the apply path don't need stub code.
    fn canvas_video_apply_design_md(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "canvas_video_apply_design_md not supported by this host".into(),
        })
    }

    /// Mode-3 entry: create a composition with DESIGN.md auto-derived from
    /// a `VisualIdentity` blob. Backs the `canvas_create_from_visual_identity`
    /// tool. Daemon runs `vi_to_design::vi_to_design_md(vi)` then delegates
    /// to the standard `create_composition` path. Default impl returns
    /// `Unsupported`.
    fn canvas_video_create_from_visual_identity(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "canvas_video_create_from_visual_identity not supported by this host".into(),
        })
    }

    /// Synthesize speech via the ElevenLabs HTTP API. Backs the
    /// `tts_synthesize_api` tool. Daemon-side reads
    /// `~/.config/nevoflux/config.toml [tts.elevenlabs]` for the API key;
    /// hosts without TTS configured return ConfigMissing. Default impl
    /// returns Unsupported so non-daemon hosts (e.g. test harnesses) don't
    /// need stub code.
    fn tts_synthesize_api(&self, _request: &serde_json::Value) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "tts_synthesize_api not supported by this host".into(),
        })
    }

    /// Attach an asset (image / audio / video / font) to a composition's
    /// `files["assets/<name>"]`. Backed by `canvas_attach_asset`. Daemon
    /// resolves the source variant (data_b64 / url / from_tab), stores
    /// the bytes, and returns the path the agent should reference in HTML.
    /// Default impl returns Unsupported.
    fn canvas_video_attach_asset(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "canvas_video_attach_asset not supported by this host".into(),
        })
    }

    /// Synthesize speech via local Kokoro ONNX inference (P5b-2).
    /// Daemon-side reads `[tts.kokoro] model_path / voices_path` and
    /// returns ConfigMissing until the model files exist. Hosts without
    /// TTS support inherit the default Unsupported.
    fn tts_synthesize_local(&self, _request: &serde_json::Value) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "tts_synthesize_local not supported by this host".into(),
        })
    }

    /// Transcribe audio via local Whisper ONNX (P5b-3). Backs auto-
    /// caption generation in P5c. Daemon reads `[tts.whisper] model_path`;
    /// returns ConfigMissing until configured.
    fn tts_transcribe(&self, _request: &serde_json::Value) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "tts_transcribe not supported by this host".into(),
        })
    }

    /// Run a layout / WCAG contrast audit on a composition. Backs the
    /// `canvas_inspect_layout` tool. Default impl returns Unsupported.
    fn canvas_video_inspect_layout(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 5,
            message: "canvas_video_inspect_layout not supported by this host".into(),
        })
    }

    // =========================================================================
    // /loop skill tool functions (spec §10)
    // =========================================================================

    /// Create a /loop. JSON args: {trigger_expr, prompt_text?, wrapped_skill?, allowed_tool_classes?}.
    /// Returns JSON {"loop_id":"…"} on success.
    /// Default impl returns Unsupported so non-daemon hosts (mocks, tests)
    /// don't need stub code.
    fn tool_loop_create(&self, _args_json: &str) -> HostResult<String> {
        Err(HostError {
            code: 5,
            message: "tool_loop_create not supported by this host".into(),
        })
    }

    /// List loops in the current session. Returns JSON array.
    fn tool_loop_list(&self) -> HostResult<String> {
        Err(HostError {
            code: 5,
            message: "tool_loop_list not supported by this host".into(),
        })
    }

    /// Cancel a loop by id. Returns JSON {"cancelled":true}.
    fn tool_loop_cancel(&self, _loop_id: &str) -> HostResult<String> {
        Err(HostError {
            code: 5,
            message: "tool_loop_cancel not supported by this host".into(),
        })
    }

    /// Get a loop's scratchpad. JSON args: {loop_id?: string}.
    /// Returns JSON {"content":"…","bytes":N}.
    fn tool_loop_scratchpad_get(&self, _args_json: &str) -> HostResult<String> {
        Err(HostError {
            code: 5,
            message: "tool_loop_scratchpad_get not supported by this host".into(),
        })
    }

    /// Set a loop's scratchpad (iteration-only).
    /// JSON args: {content:string, loop_id?:string}. Returns JSON {"bytes_written":N}.
    fn tool_loop_scratchpad_set(&self, _args_json: &str) -> HostResult<String> {
        Err(HostError {
            code: 5,
            message: "tool_loop_scratchpad_set not supported by this host".into(),
        })
    }
}

/// Mock host functions for testing.
#[cfg(test)]
pub struct MockHostFunctions {
    /// Simulated LLM responses.
    pub llm_responses: std::cell::RefCell<Vec<LlmResponse>>,
    /// Simulated skills.
    pub skills: std::cell::RefCell<Vec<SkillSummary>>,
    /// Interrupt flag for testing.
    pub interrupted: std::cell::Cell<bool>,
    /// Next subagent ID counter.
    next_subagent_id: std::cell::Cell<u64>,
    /// Subagent registry for tracking spawned subagents.
    subagents: std::cell::RefCell<Vec<SubagentInfo>>,
}

#[cfg(test)]
impl Default for MockHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MockHostFunctions {
    /// Create a new mock with default behavior.
    pub fn new() -> Self {
        Self {
            llm_responses: std::cell::RefCell::new(vec![]),
            skills: std::cell::RefCell::new(vec![]),
            interrupted: std::cell::Cell::new(false),
            next_subagent_id: std::cell::Cell::new(1),
            subagents: std::cell::RefCell::new(vec![]),
        }
    }

    /// Add an LLM response.
    pub fn add_llm_response(&self, response: LlmResponse) {
        self.llm_responses.borrow_mut().push(response);
    }

    /// Add a skill.
    pub fn add_skill(&self, skill: SkillSummary) {
        self.skills.borrow_mut().push(skill);
    }

    /// Set the interrupt flag.
    pub fn set_interrupted(&self, interrupted: bool) {
        self.interrupted.set(interrupted);
    }
}

#[cfg(test)]
impl HostFunctions for MockHostFunctions {
    fn llm_chat(&self, _request: &LlmRequest) -> HostResult<LlmResponse> {
        let mut responses = self.llm_responses.borrow_mut();
        if responses.is_empty() {
            Ok(LlmResponse {
                text: "Mock response".into(),
                tool_calls: vec![],
                reasoning: None,
            })
        } else {
            Ok(responses.remove(0))
        }
    }

    fn llm_stream_start(&self, _request: &LlmRequest) -> HostResult<u64> {
        Ok(1) // Mock stream ID
    }

    fn llm_stream_next(&self, _stream_id: u64) -> HostResult<Option<LlmChunk>> {
        Ok(Some(LlmChunk {
            text: Some("Mock".into()),
            tool_calls: vec![],
            done: true,
            reasoning: None,
            images: vec![],
        }))
    }

    fn llm_stream_close(&self, _stream_id: u64) -> HostResult<()> {
        Ok(())
    }

    fn memory_search(&self, _query: &str, _limit: usize) -> HostResult<Vec<MemoryChunk>> {
        Ok(vec![])
    }

    fn memory_create(&self, _content: &str, _metadata: &serde_json::Value) -> HostResult<String> {
        Ok("mem-001".into())
    }

    fn memory_update(&self, _id: &str, _content: &str) -> HostResult<()> {
        Ok(())
    }

    fn memory_delete(&self, _id: &str) -> HostResult<()> {
        Ok(())
    }

    fn knowledge_teach(
        &self,
        _category: &str,
        _summary: &str,
        _details: &str,
        _domain: Option<&str>,
    ) -> HostResult<String> {
        Ok("K-test01".into())
    }

    fn memory_view(&self, _limit: usize) -> HostResult<Vec<crate::types::KnowledgeViewEntry>> {
        Ok(vec![])
    }

    fn skill_list(&self) -> HostResult<Vec<SkillSummary>> {
        Ok(self.skills.borrow().clone())
    }

    fn skill_load(&self, _name: &str) -> HostResult<String> {
        Ok("# Mock Skill\n\nContent here.".into())
    }

    fn skill_read(&self, _name: &str, _path: &str) -> HostResult<String> {
        Ok("File content".into())
    }

    fn skill_execute(
        &self,
        _name: &str,
        _script: &str,
        _args: &serde_json::Value,
    ) -> HostResult<String> {
        Ok("Execution result".into())
    }

    fn tool_read(
        &self,
        _path: &str,
        _offset: Option<u64>,
        _limit: Option<u64>,
    ) -> HostResult<ReadResult> {
        Ok(ReadResult {
            total_lines: 1,
            total_bytes: 12,
            returned_lines: 1,
            offset: 0,
            content: "File content".into(),
            truncated: false,
        })
    }

    fn tool_write(&self, _path: &str, _content: &str) -> HostResult<()> {
        Ok(())
    }

    fn tool_edit(
        &self,
        _path: &str,
        _old_string: &str,
        _new_string: &str,
        _replace_all: bool,
    ) -> HostResult<()> {
        Ok(())
    }

    fn tool_bash(&self, _command: &str, _timeout_ms: Option<u64>) -> HostResult<BashResult> {
        Ok(BashResult {
            exit_code: Some(0),
            status: BashStatus::Success,
            total_lines: 1,
            total_bytes: 14,
            returned_lines: 1,
            stdout: "Command output".into(),
            stderr: None,
            truncated: false,
            hint: None,
        })
    }

    fn tool_glob(&self, _pattern: &str, _path: Option<&str>) -> HostResult<Vec<String>> {
        Ok(vec!["file1.rs".into(), "file2.rs".into()])
    }

    fn tool_grep(
        &self,
        _pattern: &str,
        _path: Option<&str>,
        _file_type: Option<&str>,
        _case_insensitive: Option<bool>,
        _max_results: Option<u64>,
    ) -> HostResult<GrepResult> {
        Ok(GrepResult {
            total_matches: 1,
            total_files: 1,
            returned: 1,
            results: vec![GrepMatch {
                file: "file.rs".into(),
                line: 1,
                content: "match1".into(),
            }],
            truncated: false,
        })
    }

    fn tool_web_search(&self, _query: &str) -> HostResult<String> {
        Ok("Search results".into())
    }

    fn tool_web_fetch(&self, _url: &str, _prompt: &str) -> HostResult<String> {
        Ok("Fetched content".into())
    }

    fn tool_ask_user(&self, _question: &str, _options: &[String]) -> HostResult<String> {
        Ok("User answer".into())
    }

    fn permission_request(
        &self,
        _resource_type: &str,
        _action: &str,
        _resource: &str,
    ) -> HostResult<bool> {
        Ok(true) // Always grant in mock
    }

    fn permission_check(
        &self,
        _resource_type: &str,
        _action: &str,
        _resource: &str,
    ) -> HostResult<bool> {
        Ok(true) // Always granted in mock
    }

    fn tool_search(&self, _query: &str, _max_results: usize) -> HostResult<Vec<ToolSearchResult>> {
        Ok(vec![])
    }

    fn tool_call_dynamic(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> HostResult<String> {
        Ok(format!("Mock result for tool: {}", tool_name))
    }

    fn computer_screenshot(&self, _monitor: Option<i64>) -> HostResult<String> {
        Ok(r#"{"width":1920,"height":1080,"format":"png","data":"mock_base64"}"#.into())
    }

    fn computer_mouse_move(&self, x: i64, y: i64) -> HostResult<String> {
        Ok(format!(r#"{{"moved_to":{{"x":{},"y":{}}}}}"#, x, y))
    }

    fn computer_drag(
        &self,
        start_x: i64,
        start_y: i64,
        end_x: i64,
        end_y: i64,
        _button: Option<&str>,
    ) -> HostResult<String> {
        Ok(format!(
            r#"{{"dragged":{{"from":{{"x":{},"y":{}}},"to":{{"x":{},"y":{}}}}}}}"#,
            start_x, start_y, end_x, end_y
        ))
    }

    fn computer_cursor_position(&self) -> HostResult<String> {
        Ok(r#"{"x":960,"y":540}"#.into())
    }

    fn computer_mouse_down(&self, x: i64, y: i64, _button: Option<&str>) -> HostResult<String> {
        Ok(format!(r#"{{"mouse_down":{{"x":{},"y":{}}}}}"#, x, y))
    }

    fn computer_mouse_up(&self, x: i64, y: i64, _button: Option<&str>) -> HostResult<String> {
        Ok(format!(r#"{{"mouse_up":{{"x":{},"y":{}}}}}"#, x, y))
    }

    fn computer_hold_key(
        &self,
        key: &str,
        duration_ms: u64,
        _modifiers: &[String],
    ) -> HostResult<String> {
        Ok(format!(
            r#"{{"held":"{}","duration_ms":{}}}"#,
            key, duration_ms
        ))
    }

    fn computer_wait(&self, ms: u64) -> HostResult<String> {
        Ok(format!(r#"{{"waited_ms":{}}}"#, ms))
    }

    fn computer_click(
        &self,
        x: i64,
        y: i64,
        _button: Option<&str>,
        _click_type: Option<&str>,
    ) -> HostResult<String> {
        Ok(format!(r#"{{"clicked":{{"x":{},"y":{}}}}}"#, x, y))
    }

    fn computer_type_text(&self, text: &str, _delay_ms: Option<u64>) -> HostResult<String> {
        Ok(format!(r#"{{"typed_chars":{}}}"#, text.len()))
    }

    fn computer_key(
        &self,
        key: &str,
        _modifiers: &[String],
        _repeat: Option<u64>,
    ) -> HostResult<String> {
        Ok(format!(r#"{{"pressed":"{}"}}"#, key))
    }

    fn computer_scroll(
        &self,
        x: i64,
        y: i64,
        direction: &str,
        _amount: Option<u64>,
    ) -> HostResult<String> {
        Ok(format!(
            r#"{{"scrolled":"{}","at":{{"x":{},"y":{}}}}}"#,
            direction, x, y
        ))
    }

    fn builtin_chat(&self, input: &AgentInput) -> HostResult<AgentOutput> {
        Ok(AgentOutput {
            text: format!("Chat response to: {}", input.user_message),
            tool_calls: vec![],
            continue_loop: false,
            plan_proposal: None,
            artifact: None,
        })
    }

    fn builtin_browser(&self, input: &AgentInput) -> HostResult<AgentOutput> {
        Ok(AgentOutput {
            text: format!("Browser response to: {}", input.user_message),
            tool_calls: vec![],
            continue_loop: false,
            plan_proposal: None,
            artifact: None,
        })
    }

    fn builtin_agent(&self, input: &AgentInput) -> HostResult<AgentOutput> {
        Ok(AgentOutput {
            text: format!("Agent response to: {}", input.user_message),
            tool_calls: vec![],
            continue_loop: false,
            plan_proposal: None,
            artifact: None,
        })
    }

    fn browser_navigate(&self, url: &str, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"url": url, "title": "Mock Page"}),
        ))
    }

    fn browser_go_back(&self, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"action": "go_back"}),
        ))
    }

    fn browser_go_forward(&self, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"action": "go_forward"}),
        ))
    }

    fn browser_click(&self, selector: &str, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"clicked": selector}),
        ))
    }

    fn browser_click_by_id(
        &self,
        element_id: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"clicked_id": element_id}),
        ))
    }

    fn browser_type(
        &self,
        selector: &str,
        text: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"typed": text, "into": selector}),
        ))
    }

    fn browser_type_by_id(
        &self,
        element_id: &str,
        text: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"typed": text, "into_id": element_id}),
        ))
    }

    fn browser_fill(
        &self,
        selector: &str,
        value: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"filled": selector, "value": value}),
        ))
    }

    fn browser_fill_by_id(
        &self,
        element_id: &str,
        value: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(
            serde_json::json!({"filled_id": element_id, "value": value}),
        ))
    }

    fn browser_get_content(&self, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "content": "<html><body>Mock page content</body></html>",
            "text": "Mock page content"
        })))
    }

    fn browser_get_markdown(&self, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "markdown": "# Mock Page\n\nThis is mock content."
        })))
    }

    fn browser_screenshot(
        &self,
        _full_page: bool,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::screenshot("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="))
    }

    fn browser_eval_js(&self, script: &str, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "result": format!("Executed: {}", script)
        })))
    }

    fn browser_scroll(
        &self,
        direction: &str,
        amount: &str,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "scrolled": direction,
            "amount": amount
        })))
    }

    fn browser_wait_for(
        &self,
        selector: &str,
        _timeout_ms: u64,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "found": selector
        })))
    }

    fn browser_get_elements(
        &self,
        _tab_id: Option<i64>,
        _keywords: Option<Vec<String>>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "refs": {
                "e1": {"role": "button", "name": "Submit", "selectors": [{"type": "css", "strategy": "id", "value": "#submit"}]},
                "e2": {"role": "textbox", "name": "Email", "selectors": [{"type": "css", "strategy": "id", "value": "#email"}]}
            }
        })))
    }

    fn browser_list_tabs(&self, _tab_id: Option<i64>) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "tabs": [
                {"id": 1, "url": "https://example.com", "title": "Example", "active": true, "windowId": 1},
                {"id": 2, "url": "https://test.com", "title": "Test Page", "active": false, "windowId": 1}
            ]
        })))
    }

    fn browser_query_tabs(
        &self,
        _params: &serde_json::Value,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "tabs": [
                {"id": 1, "url": "https://example.com", "title": "Example", "active": true, "windowId": 1}
            ]
        })))
    }

    fn browser_read_artifact(
        &self,
        _params: &serde_json::Value,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "content": "<!DOCTYPE html><html><body>Mock artifact</body></html>",
            "totalLines": 1,
            "truncated": false,
            "title": "Mock Artifact",
            "type": "html"
        })))
    }

    fn browser_edit_artifact(
        &self,
        _params: &serde_json::Value,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "lines": 1
        })))
    }

    fn browser_extract_visual_identity(
        &self,
        _params: &serde_json::Value,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        // Mock: return a minimal VisualIdentity-shaped response so unit tests
        // can verify the dispatch wiring without actually opening a tab.
        Ok(BrowserToolResult::success(serde_json::json!({
            "name": "Mock Brand",
            "tagline": "Mock tagline",
            "url": "https://example.com",
            "hero_screenshot_b64": null,
            "logo": null,
            "colors": [],
            "fonts": [],
            "key_assets": [],
            "extracted_at": 1777200000_i64,
            "warnings": ["mock_extraction"]
        })))
    }

    fn browser_wait_for_stable(
        &self,
        strategy: &str,
        _max_wait: u64,
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "stable": true,
            "strategy": strategy,
            "duration_ms": 100
        })))
    }

    fn browser_viewport_snapshot(
        &self,
        _tab_id: Option<i64>,
        _keywords: Option<Vec<String>>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "tree": "Page: \"Test\" | URL: https://example.com\nViewport: 1920x1080 | Scroll: 0/2000 (top)\n\n[e1] button \"Submit\"\n[e2] textbox \"Email\"",
            "refs": {"e1": {"selectors": [{"type": "css", "strategy": "id", "value": "#submit"}], "role": "button", "name": "Submit"}, "e2": {"selectors": [{"type": "css", "strategy": "id", "value": "#email"}], "role": "textbox", "name": "Email"}},
            "viewportInfo": {"scrollTop": 0, "scrollHeight": 2000, "viewportHeight": 1080, "viewportWidth": 1920, "canScrollUp": false, "canScrollDown": true, "pageTitle": "Test", "url": "https://example.com"}
        })))
    }

    fn browser_key_press(
        &self,
        key: &str,
        _modifiers: &[String],
        _tab_id: Option<i64>,
    ) -> HostResult<BrowserToolResult> {
        Ok(BrowserToolResult::success(serde_json::json!({
            "pressed": key
        })))
    }

    fn is_interrupted(&self) -> HostResult<bool> {
        Ok(self.interrupted.get())
    }

    fn subagent_spawn(&self, task: &str, mode: &str, _tab_id: Option<i64>) -> HostResult<u64> {
        let id = self.next_subagent_id.get();
        self.next_subagent_id.set(id + 1);

        let info = SubagentInfo {
            id,
            task: task.to_string(),
            mode: mode.to_string(),
            status: "completed".to_string(), // Mock: immediately complete
        };
        self.subagents.borrow_mut().push(info);

        Ok(id)
    }

    fn subagent_wait_all(&self, ids: &[u64]) -> HostResult<String> {
        let results: Vec<serde_json::Value> = ids
            .iter()
            .map(|&id| {
                let subagents = self.subagents.borrow();
                if let Some(s) = subagents.iter().find(|s| s.id == id) {
                    serde_json::json!({
                        "id": id,
                        "status": s.status,
                        "result": format!("Result from subagent {}: {}", id, s.task),
                    })
                } else {
                    serde_json::json!({
                        "id": id,
                        "status": "not_found",
                        "error": format!("Subagent not found: {}", id),
                    })
                }
            })
            .collect();
        Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
    }

    fn subagent_status(&self, id: u64) -> HostResult<String> {
        let subagents = self.subagents.borrow();
        subagents
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.status.clone())
            .ok_or_else(|| HostError {
                code: 404,
                message: format!("Subagent not found: {}", id),
            })
    }

    fn subagent_wait(&self, id: u64) -> HostResult<String> {
        let subagents = self.subagents.borrow();
        subagents
            .iter()
            .find(|s| s.id == id)
            .map(|s| format!("Result from subagent {}: {}", id, s.task))
            .ok_or_else(|| HostError {
                code: 404,
                message: format!("Subagent not found: {}", id),
            })
    }

    fn subagent_kill(&self, id: u64) -> HostResult<bool> {
        let mut subagents = self.subagents.borrow_mut();
        if let Some(info) = subagents.iter_mut().find(|s| s.id == id) {
            if info.status == "running" {
                info.status = "killed".to_string();
                Ok(true)
            } else {
                Ok(false) // Already completed
            }
        } else {
            Err(HostError {
                code: 404,
                message: format!("Subagent not found: {}", id),
            })
        }
    }

    fn subagent_list(&self) -> HostResult<Vec<SubagentInfo>> {
        Ok(self.subagents.borrow().clone())
    }

    fn list_agents(&self) -> HostResult<String> {
        // Mock: return empty list
        Ok("[]".to_string())
    }

    fn stream_emit(&self, _text: &str) -> HostResult<()> {
        // Mock: just accept the chunk
        Ok(())
    }

    fn stream_end(&self) -> HostResult<()> {
        // Mock: just accept the end signal
        Ok(())
    }

    fn set_iteration(&self, _iteration: u32) -> HostResult<()> {
        Ok(())
    }

    fn set_model_override(&self, _provider: &str, _model: &str) -> HostResult<()> {
        Ok(())
    }

    fn canvas_video_create_composition(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 501,
            message: "canvas_video_create_composition not implemented in MockHostFunctions".into(),
        })
    }

    fn canvas_video_render_start(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 501,
            message: "canvas_video_render_start not implemented in MockHostFunctions".into(),
        })
    }

    fn canvas_video_lint_composition(
        &self,
        _request: &serde_json::Value,
    ) -> HostResult<serde_json::Value> {
        Err(HostError {
            code: 501,
            message: "canvas_video_lint_composition not implemented in MockHostFunctions".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_error_display() {
        let err = HostError {
            code: 404,
            message: "Not found".into(),
        };
        assert!(err.to_string().contains("404"));
        assert!(err.to_string().contains("Not found"));
    }

    #[test]
    fn test_mock_host_functions_llm_chat() {
        let mock = MockHostFunctions::new();
        let request = LlmRequest {
            messages: vec![Message::user("Hello")],
            tools: vec![],
            stream: false,
        };
        let response = mock.llm_chat(&request).unwrap();
        assert_eq!(response.text, "Mock response");
    }

    #[test]
    fn test_mock_host_functions_llm_chat_with_response() {
        let mock = MockHostFunctions::new();
        mock.add_llm_response(LlmResponse {
            text: "Custom response".into(),
            tool_calls: vec![],
            reasoning: None,
        });

        let request = LlmRequest {
            messages: vec![Message::user("Hello")],
            tools: vec![],
            stream: false,
        };
        let response = mock.llm_chat(&request).unwrap();
        assert_eq!(response.text, "Custom response");
    }

    #[test]
    fn test_mock_host_functions_streaming() {
        let mock = MockHostFunctions::new();
        let request = LlmRequest {
            messages: vec![Message::user("Hello")],
            tools: vec![],
            stream: true,
        };

        let stream_id = mock.llm_stream_start(&request).unwrap();
        assert_eq!(stream_id, 1);

        let chunk = mock.llm_stream_next(stream_id).unwrap().unwrap();
        assert!(chunk.done);

        assert!(mock.llm_stream_close(stream_id).is_ok());
    }

    #[test]
    fn test_mock_host_functions_memory() {
        let mock = MockHostFunctions::new();

        let results = mock.memory_search("test", 10).unwrap();
        assert!(results.is_empty());

        let id = mock
            .memory_create("content", &serde_json::json!({}))
            .unwrap();
        assert_eq!(id, "mem-001");

        assert!(mock.memory_update("mem-001", "new content").is_ok());
        assert!(mock.memory_delete("mem-001").is_ok());
    }

    #[test]
    fn test_mock_host_functions_skills() {
        let mock = MockHostFunctions::new();
        mock.add_skill(SkillSummary {
            name: "test-skill".into(),
            description: "A test".into(),
            tags: vec![],
        });

        let skills = mock.skill_list().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");

        let content = mock.skill_load("test-skill").unwrap();
        assert!(content.contains("Mock Skill"));
    }

    #[test]
    fn test_mock_host_functions_tools() {
        let mock = MockHostFunctions::new();

        assert!(mock.tool_read("/path", None, None).is_ok());
        assert!(mock.tool_write("/path", "content").is_ok());
        assert!(mock.tool_edit("/path", "old", "new", false).is_ok());
        assert!(mock.tool_bash("ls", None).is_ok());
        assert!(mock.tool_glob("*.rs", None).is_ok());
        assert!(mock.tool_grep("pattern", None, None, None, None).is_ok());
        assert!(mock.tool_web_search("query").is_ok());
        assert!(mock.tool_web_fetch("http://example.com", "prompt").is_ok());
        assert!(mock
            .tool_ask_user("Question?", &["A".into(), "B".into()])
            .is_ok());
    }

    #[test]
    fn test_mock_host_functions_permission() {
        let mock = MockHostFunctions::new();

        assert!(mock.permission_check("file", "read", "/home").unwrap());
        assert!(mock.permission_request("file", "write", "/home").unwrap());
    }

    #[test]
    fn test_mock_host_functions_builtin_proxy() {
        let mock = MockHostFunctions::new();
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

        let chat_output = mock.builtin_chat(&input).unwrap();
        assert!(chat_output.text.contains("Hello"));

        let browser_output = mock.builtin_browser(&input).unwrap();
        assert!(browser_output.text.contains("Hello"));

        let agent_output = mock.builtin_agent(&input).unwrap();
        assert!(agent_output.text.contains("Hello"));
    }

    #[test]
    fn test_mock_host_functions_tool_search() {
        let mock = MockHostFunctions::new();
        let results = mock.tool_search("file", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_mock_host_functions_tool_call_dynamic() {
        let mock = MockHostFunctions::new();
        let result = mock
            .tool_call_dynamic("read_file", &serde_json::json!({"path": "/test.txt"}))
            .unwrap();
        assert!(result.contains("read_file"));
    }

    #[test]
    fn test_mock_host_functions_browser_navigate() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_navigate("https://example.com", None).unwrap();
        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[test]
    fn test_mock_host_functions_browser_click() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_click("#submit-btn", None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_click_by_id() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_click_by_id("submit-btn", None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_type() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_type("#input", "Hello", None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_fill() {
        let mock = MockHostFunctions::new();
        let result = mock
            .browser_fill("#email", "test@example.com", None)
            .unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_get_content() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_get_content(None).unwrap();
        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[test]
    fn test_mock_host_functions_browser_get_markdown() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_get_markdown(None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_screenshot() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_screenshot(false, None).unwrap();
        assert!(result.success);
        assert!(result.screenshot.is_some());
    }

    #[test]
    fn test_mock_host_functions_browser_eval_js() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_eval_js("document.title", None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_scroll() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_scroll("down", "page", None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_browser_wait_for() {
        let mock = MockHostFunctions::new();
        let result = mock.browser_wait_for("#element", 5000, None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_mock_host_functions_is_interrupted_default() {
        let mock = MockHostFunctions::new();
        let result = mock.is_interrupted().unwrap();
        assert!(!result); // Default is not interrupted
    }

    #[test]
    fn test_mock_host_functions_is_interrupted_set() {
        let mock = MockHostFunctions::new();

        // Set interrupted
        mock.set_interrupted(true);
        assert!(mock.is_interrupted().unwrap());

        // Reset
        mock.set_interrupted(false);
        assert!(!mock.is_interrupted().unwrap());
    }

    #[test]
    fn test_mock_host_functions_subagent_spawn() {
        let mock = MockHostFunctions::new();

        let id1 = mock
            .subagent_spawn("Search for files", "agent", None)
            .unwrap();
        assert_eq!(id1, 1);

        let id2 = mock.subagent_spawn("Chat with user", "chat", None).unwrap();
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_mock_host_functions_subagent_status() {
        let mock = MockHostFunctions::new();

        let id = mock.subagent_spawn("Test task", "agent", None).unwrap();
        let status = mock.subagent_status(id).unwrap();
        assert_eq!(status, "completed");
    }

    #[test]
    fn test_mock_host_functions_subagent_status_not_found() {
        let mock = MockHostFunctions::new();

        let result = mock.subagent_status(999);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, 404);
    }

    #[test]
    fn test_mock_host_functions_subagent_wait() {
        let mock = MockHostFunctions::new();

        let id = mock
            .subagent_spawn("Find documents", "agent", None)
            .unwrap();
        let result = mock.subagent_wait(id).unwrap();
        assert!(result.contains("Find documents"));
    }

    #[test]
    fn test_mock_host_functions_subagent_wait_not_found() {
        let mock = MockHostFunctions::new();

        let result = mock.subagent_wait(999);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, 404);
    }

    #[test]
    fn test_mock_host_functions_subagent_kill() {
        let mock = MockHostFunctions::new();

        let id = mock.subagent_spawn("Long task", "agent", None).unwrap();
        // Mock sets status to "completed" immediately, so kill returns false
        let killed = mock.subagent_kill(id).unwrap();
        assert!(!killed);
    }

    #[test]
    fn test_mock_host_functions_subagent_kill_not_found() {
        let mock = MockHostFunctions::new();

        let result = mock.subagent_kill(999);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, 404);
    }

    #[test]
    fn test_mock_host_functions_subagent_list() {
        let mock = MockHostFunctions::new();

        // Initially empty
        let list = mock.subagent_list().unwrap();
        assert!(list.is_empty());

        // Spawn some subagents
        mock.subagent_spawn("Task 1", "agent", None).unwrap();
        mock.subagent_spawn("Task 2", "browser", None).unwrap();

        let list = mock.subagent_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].task, "Task 1");
        assert_eq!(list[0].mode, "agent");
        assert_eq!(list[1].task, "Task 2");
        assert_eq!(list[1].mode, "browser");
    }

    #[test]
    fn test_mock_host_functions_subagent_spawn_with_tab_id() {
        let mock = MockHostFunctions::new();

        let id = mock
            .subagent_spawn("Read tab content", "browser", Some(42))
            .unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn test_mock_host_functions_subagent_wait_all() {
        let mock = MockHostFunctions::new();

        let id1 = mock.subagent_spawn("Task 1", "agent", None).unwrap();
        let id2 = mock.subagent_spawn("Task 2", "browser", None).unwrap();

        let result = mock.subagent_wait_all(&[id1, id2]).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["id"], id1);
        assert_eq!(parsed[1]["id"], id2);
        assert!(parsed[0]["result"].as_str().unwrap().contains("Task 1"));
        assert!(parsed[1]["result"].as_str().unwrap().contains("Task 2"));
    }

    #[test]
    fn test_mock_host_functions_subagent_wait_all_not_found() {
        let mock = MockHostFunctions::new();

        let result = mock.subagent_wait_all(&[999]).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["status"], "not_found");
    }
}
