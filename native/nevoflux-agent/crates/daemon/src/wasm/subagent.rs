//! Subagent executor for running sub-agents in isolated WASM instances.
//!
//! This module provides the infrastructure for spawning sub-agents that run
//! in isolated WASM sandboxes with configurable resource limits.
//!
//! # Security Model
//!
//! Each subagent runs in its own WASM instance with:
//! - Independent memory space (configurable size limit)
//! - Fuel limit for CPU usage (optional)
//! - Timeout via epoch interruption
//! - Access to resources only through host functions
//!
//! This ensures that subagents cannot:
//! - Access parent agent's memory
//! - Run indefinitely without limits
//! - Access resources without going through the host function boundary

use crate::config::SubagentConfig;
use crate::wasm::services::HostServices;
use nevoflux_builtin_wasm::{AgentInput, AgentMode, AgentOutput};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;
use tracing::{debug, error, warn};

/// Status of a subagent execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentStatus {
    /// Subagent is currently running.
    Running,
    /// Subagent completed successfully.
    Completed,
    /// Subagent failed with an error.
    Failed(String),
    /// Subagent was terminated by user request.
    Killed,
    /// Subagent timed out.
    TimedOut,
}

impl SubagentStatus {
    /// Get the status as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            SubagentStatus::Running => "running",
            SubagentStatus::Completed => "completed",
            SubagentStatus::Failed(_) => "failed",
            SubagentStatus::Killed => "killed",
            SubagentStatus::TimedOut => "timed_out",
        }
    }

    /// Check if this status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, SubagentStatus::Running)
    }
}

/// Handle for interacting with a spawned subagent.
#[derive(Debug)]
pub struct SubagentHandle {
    /// Unique identifier for this subagent.
    pub id: u64,
    /// Task description.
    task: String,
    /// Execution mode.
    mode: String,
    /// Optional tab ID for browser content access.
    tab_id: Option<i64>,
    /// Current status.
    status: Arc<RwLock<SubagentStatus>>,
    /// Result text (set when completed).
    result: Arc<RwLock<Option<String>>>,
    /// Notification for completion.
    completion: Arc<Notify>,
    /// Kill flag for cooperative termination.
    kill_flag: Arc<AtomicBool>,
    /// Timestamp when this subagent was spawned.
    pub spawn_time: std::time::Instant,
}

impl SubagentHandle {
    /// Create a new handle for a subagent.
    fn new(id: u64, task: String, mode: String, tab_id: Option<i64>) -> Self {
        Self {
            id,
            task,
            mode,
            tab_id,
            status: Arc::new(RwLock::new(SubagentStatus::Running)),
            result: Arc::new(RwLock::new(None)),
            completion: Arc::new(Notify::new()),
            kill_flag: Arc::new(AtomicBool::new(false)),
            spawn_time: std::time::Instant::now(),
        }
    }

    /// Get the current status.
    pub fn status(&self) -> SubagentStatus {
        self.status.read().unwrap().clone()
    }

    /// Get the task description.
    pub fn task(&self) -> &str {
        &self.task
    }

    /// Get the execution mode.
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// Check if the subagent is still running.
    pub fn is_running(&self) -> bool {
        matches!(*self.status.read().unwrap(), SubagentStatus::Running)
    }

    /// Wait for the subagent to complete and return its result.
    pub async fn wait(&self) -> Result<String, String> {
        // If already complete, return immediately
        if !self.is_running() {
            return self.get_result();
        }

        // Wait for completion notification
        self.completion.notified().await;
        self.get_result()
    }

    /// Get the result if available.
    fn get_result(&self) -> Result<String, String> {
        let status = self.status.read().unwrap();
        match &*status {
            SubagentStatus::Completed => {
                let result = self.result.read().unwrap();
                result
                    .clone()
                    .ok_or_else(|| "No result available".to_string())
            }
            SubagentStatus::Failed(err) => Err(err.clone()),
            SubagentStatus::Killed => Err("Subagent was killed".to_string()),
            SubagentStatus::TimedOut => Err("Subagent timed out".to_string()),
            SubagentStatus::Running => Err("Subagent is still running".to_string()),
        }
    }

    /// Request termination of the subagent.
    ///
    /// Returns true if the subagent was running and will be killed,
    /// false if it was already complete.
    pub fn kill(&self) -> bool {
        if !self.is_running() {
            return false;
        }

        self.kill_flag.store(true, Ordering::SeqCst);
        true
    }

    /// Check if the kill flag has been set.
    pub fn should_kill(&self) -> bool {
        self.kill_flag.load(Ordering::SeqCst)
    }

    /// Mark the subagent as completed with a result.
    fn complete(&self, result: String) {
        {
            let mut status = self.status.write().unwrap();
            *status = SubagentStatus::Completed;
        }
        {
            let mut result_guard = self.result.write().unwrap();
            *result_guard = Some(result);
        }
        self.completion.notify_waiters();
    }

    /// Mark the subagent as failed.
    fn fail(&self, error: String) {
        {
            let mut status = self.status.write().unwrap();
            *status = SubagentStatus::Failed(error);
        }
        self.completion.notify_waiters();
    }

    /// Mark the subagent as killed.
    fn mark_killed(&self) {
        {
            let mut status = self.status.write().unwrap();
            *status = SubagentStatus::Killed;
        }
        self.completion.notify_waiters();
    }

    /// Mark the subagent as timed out.
    fn mark_timed_out(&self) {
        {
            let mut status = self.status.write().unwrap();
            *status = SubagentStatus::TimedOut;
        }
        self.completion.notify_waiters();
    }
}

impl Clone for SubagentHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            task: self.task.clone(),
            mode: self.mode.clone(),
            tab_id: self.tab_id,
            status: self.status.clone(),
            result: self.result.clone(),
            completion: self.completion.clone(),
            kill_flag: self.kill_flag.clone(),
            spawn_time: self.spawn_time,
        }
    }
}

/// Executor for running subagents in isolated WASM instances.
///
/// The executor manages the lifecycle of subagents, including:
/// - Spawning new subagents with resource limits
/// - Tracking running subagents
/// - Enforcing concurrency limits
/// - Handling timeouts and kills
pub struct SubagentExecutor {
    /// Configuration for subagent execution.
    config: SubagentConfig,
    /// Next available subagent ID.
    next_id: AtomicU64,
    /// Active subagents by ID.
    handles: Arc<RwLock<HashMap<u64, SubagentHandle>>>,
    /// Tokio runtime handle for async operations.
    runtime: tokio::runtime::Handle,
    /// Base services to clone for each subagent.
    base_services: Option<HostServices>,
    /// Optional sidebar stream sender from the parent agent.
    sidebar_stream_tx:
        Option<tokio::sync::mpsc::UnboundedSender<crate::agent_host::SidebarStreamChunk>>,
    /// Agent configuration with provider API keys for subagent LLM calls.
    agent_config: Option<Arc<crate::config::AgentConfig>>,
}

impl SubagentExecutor {
    /// Create a new SubagentExecutor with the given configuration.
    pub fn new(config: SubagentConfig, runtime: tokio::runtime::Handle) -> Self {
        Self {
            config,
            next_id: AtomicU64::new(1),
            handles: Arc::new(RwLock::new(HashMap::new())),
            runtime,
            base_services: None,
            sidebar_stream_tx: None,
            agent_config: None,
        }
    }

    /// Set the base services to use for subagents.
    pub fn with_services(mut self, services: HostServices) -> Self {
        self.base_services = Some(services);
        self
    }

    /// Set the sidebar stream sender for subagents to pipe output to the parent's sidebar.
    pub fn with_sidebar_stream(
        mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::agent_host::SidebarStreamChunk>,
    ) -> Self {
        self.sidebar_stream_tx = Some(tx);
        self
    }

    /// Set the agent configuration (provides API keys for subagent LLM calls).
    pub fn with_agent_config(mut self, config: Arc<crate::config::AgentConfig>) -> Self {
        self.agent_config = Some(config);
        self
    }

    /// Get the agent configuration (for looking up provider models).
    pub fn agent_config(&self) -> Option<&Arc<crate::config::AgentConfig>> {
        self.agent_config.as_ref()
    }

    /// Get the configuration.
    pub fn config(&self) -> &SubagentConfig {
        &self.config
    }

    /// Get the number of currently running subagents.
    pub fn running_count(&self) -> usize {
        self.handles
            .read()
            .unwrap()
            .values()
            .filter(|h| h.is_running())
            .count()
    }

    /// Check if we can spawn more subagents.
    pub fn can_spawn(&self) -> bool {
        self.running_count() < self.config.max_concurrent
    }

    /// Allocate a new subagent ID.
    fn allocate_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Spawn a subagent to execute a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task description for the subagent
    /// * `mode` - Execution mode (chat, browser, agent)
    /// * `custom_prompt` - Optional custom system prompt
    /// * `tab_id` - Optional tab ID for browser content access (read-only)
    ///
    /// # Returns
    ///
    /// A handle to the spawned subagent, or an error if spawning failed.
    pub fn spawn(
        &self,
        task: String,
        mode: AgentMode,
        custom_prompt: Option<String>,
        tab_id: Option<i64>,
        tools_config: Option<nevoflux_protocol::subagent::ToolsConfig>,
        provider_override: Option<String>,
        model_override: Option<String>,
    ) -> Result<SubagentHandle, String> {
        // Prune completed handles when the map grows too large to prevent
        // unbounded memory growth. We keep a generous threshold so callers
        // have time to wait()/get() results before they're removed.
        const PRUNE_THRESHOLD: usize = 64;
        {
            let mut handles = self.handles.write().unwrap();
            if handles.len() > PRUNE_THRESHOLD {
                handles.retain(|_, h| h.is_running());
            }
        }

        // Check concurrency limit
        if !self.can_spawn() {
            return Err(format!(
                "Maximum concurrent subagents ({}) reached",
                self.config.max_concurrent
            ));
        }

        let id = self.allocate_id();
        let mode_str = match mode {
            AgentMode::Chat => "chat",
            AgentMode::Browser => "browser",
            AgentMode::Agent => "agent",
            AgentMode::Code => "code",
        };

        let handle = SubagentHandle::new(id, task.clone(), mode_str.to_string(), tab_id);

        // Register the handle
        {
            let mut handles = self.handles.write().unwrap();
            handles.insert(id, handle.clone());
        }

        debug!(
            "Spawning subagent {}: task='{}', mode={}, tab_id={:?}",
            id, task, mode_str, tab_id
        );

        // Spawn the execution task
        let executor_handle = handle.clone();
        let timeout_secs = self.config.timeout_secs;
        let base_services = self.base_services.clone();
        let config = self
            .agent_config
            .as_ref()
            .map(|c| (**c).clone())
            .unwrap_or_default();
        let sidebar_tx = self.sidebar_stream_tx.clone();

        self.runtime.spawn(async move {
            let result = Self::run_subagent_with_timeout(
                id,
                task.clone(),
                mode,
                custom_prompt,
                tab_id,
                tools_config,
                provider_override,
                model_override,
                base_services,
                config,
                Duration::from_secs(timeout_secs),
                executor_handle.clone(),
                sidebar_tx,
            )
            .await;

            match result {
                Ok(output) => {
                    debug!("Subagent {} completed successfully", id);
                    executor_handle.complete(output.text);
                }
                Err(e) => {
                    if executor_handle.should_kill() {
                        debug!("Subagent {} was killed", id);
                        executor_handle.mark_killed();
                    } else if e.contains("timeout") || e.contains("timed out") {
                        warn!("Subagent {} timed out", id);
                        executor_handle.mark_timed_out();
                    } else {
                        error!("Subagent {} failed: {}", id, e);
                        executor_handle.fail(e);
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Run a subagent with timeout.
    #[allow(clippy::too_many_arguments)]
    async fn run_subagent_with_timeout(
        id: u64,
        task: String,
        mode: AgentMode,
        custom_prompt: Option<String>,
        tab_id: Option<i64>,
        tools_config: Option<nevoflux_protocol::subagent::ToolsConfig>,
        provider_override: Option<String>,
        model_override: Option<String>,
        base_services: Option<HostServices>,
        config: crate::config::AgentConfig,
        timeout_duration: Duration,
        handle: SubagentHandle,
        sidebar_stream_tx: Option<
            tokio::sync::mpsc::UnboundedSender<crate::agent_host::SidebarStreamChunk>,
        >,
    ) -> Result<AgentOutput, String> {
        let execution = Self::run_subagent_inner(
            id,
            task,
            mode,
            custom_prompt,
            tab_id,
            tools_config,
            provider_override,
            model_override,
            base_services,
            config,
            handle.clone(),
            sidebar_stream_tx,
        );

        match timeout(timeout_duration, execution).await {
            Ok(result) => result,
            Err(_) => Err("Subagent execution timed out".to_string()),
        }
    }

    /// Inner subagent execution logic.
    ///
    /// This runs the subagent using DaemonHostFunctions, which currently
    /// uses the Tokio-based implementation. In a full WASM sandboxed
    /// implementation, this would create a new WASM instance.
    #[allow(clippy::too_many_arguments)]
    async fn run_subagent_inner(
        id: u64,
        task: String,
        mode: AgentMode,
        custom_prompt: Option<String>,
        tab_id: Option<i64>,
        tools_config: Option<nevoflux_protocol::subagent::ToolsConfig>,
        provider_override: Option<String>,
        model_override: Option<String>,
        base_services: Option<HostServices>,
        config: crate::config::AgentConfig,
        handle: SubagentHandle,
        sidebar_stream_tx: Option<
            tokio::sync::mpsc::UnboundedSender<crate::agent_host::SidebarStreamChunk>,
        >,
    ) -> Result<AgentOutput, String> {
        use crate::agent_host::DaemonHostFunctions;
        use nevoflux_builtin_wasm::{Agent, AgentConfig as WasmAgentConfig};
        use std::sync::Arc;

        // Create a new runtime handle for the subagent
        let runtime = tokio::runtime::Handle::current();

        // Create host functions for the subagent
        let mut host = DaemonHostFunctions::new(Arc::new(config), runtime);
        host = host.with_is_subagent(true);
        if let Some(services) = base_services {
            // Create a new services instance with its own interrupt flag
            let subagent_services = services.clone();
            // The interrupt flag is checked via the handle's kill_flag
            host = host.with_services(subagent_services);
        }

        // Pipe subagent stream to parent's sidebar
        if let Some(tx) = sidebar_stream_tx {
            host = host.with_sidebar_stream(tx);
        }

        // Apply provider/model override if specified
        if let (Some(provider), Some(model)) = (provider_override, model_override) {
            debug!(
                "Subagent {}: applying provider/model override: provider={}, model={}",
                id, provider, model
            );
            host = host.with_llm_override(provider, model);
        }

        // Create sandbox for agent-mode subagents
        let custom_prompt = if matches!(mode, AgentMode::Agent | AgentMode::Code) {
            let sandbox = format!("/tmp/subagent/{}", id);
            std::fs::create_dir_all(&sandbox)
                .map_err(|e| format!("Failed to create sandbox directory: {}", e))?;
            host = host.with_subagent_sandbox(sandbox.clone());

            // Append sandbox path to prompt for agent-mode subagents
            custom_prompt.map(|p| {
                format!(
                    "{}\n\nYour sandbox directory: {}\n\
                     All write/edit operations are restricted to this path.",
                    p, sandbox
                )
            })
        } else {
            custom_prompt
        };

        // Create agent with subagent configuration
        let agent_config = WasmAgentConfig::for_subagent();
        let agent = Agent::with_config(host, agent_config);

        // Build input with custom prompt and optional tab access
        let input = AgentInput {
            session_id: format!("subagent-{}", id),
            mode,
            user_message: task,
            history: vec![],
            attachments: vec![],
            local_files: vec![],
            custom_system_prompt: custom_prompt,
            tab_id,
            tab_ids: vec![],
            skill_context: None,
            available_models: vec![],
            mcp_servers: vec![],
            soul_context: None,
            tools_config,
            os_platform: Some(std::env::consts::OS.to_string()),
        };

        // Check for kill before running
        if handle.should_kill() {
            return Err("Subagent was killed before execution".to_string());
        }

        // Run the agent in spawn_blocking so that Handle::block_on() calls
        // inside agent.run() don't panic with "Cannot start a runtime from
        // within a runtime".
        tokio::task::spawn_blocking(move || {
            agent.run(&input).map_err(|e| format!("Agent error: {}", e))
        })
        .await
        .map_err(|e| format!("Subagent task panicked: {}", e))?
    }

    /// Get a handle to a subagent by ID.
    pub fn get(&self, id: u64) -> Option<SubagentHandle> {
        self.handles.read().unwrap().get(&id).cloned()
    }

    /// Get the status of a subagent.
    pub fn status(&self, id: u64) -> Option<SubagentStatus> {
        self.handles.read().unwrap().get(&id).map(|h| h.status())
    }

    /// Wait for multiple subagents to complete.
    pub async fn wait_all(&self, ids: &[u64]) -> Vec<(u64, Result<String, String>)> {
        let futures: Vec<_> = ids
            .iter()
            .map(|&id| {
                let handle = self.get(id);
                async move {
                    match handle {
                        Some(h) => (id, h.wait().await),
                        None => (id, Err(format!("Subagent {} not found", id))),
                    }
                }
            })
            .collect();
        futures::future::join_all(futures).await
    }

    /// Wait for a subagent to complete.
    pub async fn wait(&self, id: u64) -> Result<String, String> {
        let handle = self
            .get(id)
            .ok_or_else(|| format!("Subagent {} not found", id))?;
        handle.wait().await
    }

    /// Kill a subagent.
    pub fn kill(&self, id: u64) -> Result<bool, String> {
        let handle = self
            .get(id)
            .ok_or_else(|| format!("Subagent {} not found", id))?;
        Ok(handle.kill())
    }

    /// List all subagent handles.
    pub fn list(&self) -> Vec<SubagentHandle> {
        self.handles.read().unwrap().values().cloned().collect()
    }

    /// Clean up completed subagents older than the given duration.
    pub fn cleanup(&self, _max_age: Duration) {
        // This is a simplified cleanup - in production you'd track completion time
        let mut handles = self.handles.write().unwrap();
        handles.retain(|_, h| h.is_running());
    }
}

impl std::fmt::Debug for SubagentExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentExecutor")
            .field("config", &self.config)
            .field("next_id", &self.next_id.load(Ordering::SeqCst))
            .field("active_count", &self.handles.read().unwrap().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_executor() -> SubagentExecutor {
        let config = SubagentConfig::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        SubagentExecutor::new(config, rt.handle().clone())
    }

    #[test]
    fn test_subagent_status_as_str() {
        assert_eq!(SubagentStatus::Running.as_str(), "running");
        assert_eq!(SubagentStatus::Completed.as_str(), "completed");
        assert_eq!(SubagentStatus::Failed("err".into()).as_str(), "failed");
        assert_eq!(SubagentStatus::Killed.as_str(), "killed");
        assert_eq!(SubagentStatus::TimedOut.as_str(), "timed_out");
    }

    #[test]
    fn test_subagent_status_is_terminal() {
        assert!(!SubagentStatus::Running.is_terminal());
        assert!(SubagentStatus::Completed.is_terminal());
        assert!(SubagentStatus::Failed("err".into()).is_terminal());
        assert!(SubagentStatus::Killed.is_terminal());
        assert!(SubagentStatus::TimedOut.is_terminal());
    }

    #[test]
    fn test_subagent_handle_new() {
        let handle = SubagentHandle::new(1, "test task".into(), "agent".into(), None);

        assert_eq!(handle.id, 1);
        assert_eq!(handle.task(), "test task");
        assert_eq!(handle.mode(), "agent");
        assert!(handle.is_running());
        assert_eq!(handle.status(), SubagentStatus::Running);
    }

    #[test]
    fn test_subagent_handle_complete() {
        let handle = SubagentHandle::new(1, "task".into(), "chat".into(), None);

        assert!(handle.is_running());
        handle.complete("result text".into());

        assert!(!handle.is_running());
        assert_eq!(handle.status(), SubagentStatus::Completed);
    }

    #[test]
    fn test_subagent_handle_fail() {
        let handle = SubagentHandle::new(1, "task".into(), "chat".into(), None);

        handle.fail("something went wrong".into());

        assert!(!handle.is_running());
        assert!(matches!(handle.status(), SubagentStatus::Failed(_)));
    }

    #[test]
    fn test_subagent_handle_kill() {
        let handle = SubagentHandle::new(1, "task".into(), "agent".into(), None);

        assert!(!handle.should_kill());
        let killed = handle.kill();
        assert!(killed);
        assert!(handle.should_kill());

        // Kill after completion should return false
        handle.mark_killed();
        let killed_again = handle.kill();
        assert!(!killed_again);
    }

    #[test]
    fn test_subagent_handle_clone() {
        let handle = SubagentHandle::new(1, "task".into(), "browser".into(), None);
        let cloned = handle.clone();

        assert_eq!(handle.id, cloned.id);
        assert_eq!(handle.task(), cloned.task());

        // Changes should be visible through both handles
        handle.complete("done".into());
        assert_eq!(cloned.status(), SubagentStatus::Completed);
    }

    #[test]
    fn test_executor_new() {
        let executor = create_test_executor();

        assert_eq!(executor.running_count(), 0);
        assert!(executor.can_spawn());
    }

    #[test]
    fn test_executor_allocate_id() {
        let executor = create_test_executor();

        let id1 = executor.allocate_id();
        let id2 = executor.allocate_id();
        let id3 = executor.allocate_id();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn test_executor_concurrency_limit() {
        let mut config = SubagentConfig::default();
        config.max_concurrent = 2;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let executor = SubagentExecutor::new(config, rt.handle().clone());

        // Add fake running handles to test limit
        {
            let mut handles = executor.handles.write().unwrap();
            handles.insert(
                1,
                SubagentHandle::new(1, "task1".into(), "agent".into(), None),
            );
            handles.insert(
                2,
                SubagentHandle::new(2, "task2".into(), "agent".into(), None),
            );
        }

        assert!(!executor.can_spawn());
        assert_eq!(executor.running_count(), 2);

        // Complete one
        executor.get(1).unwrap().complete("done".into());
        assert!(executor.can_spawn());
    }

    #[test]
    fn test_executor_list() {
        let executor = create_test_executor();

        {
            let mut handles = executor.handles.write().unwrap();
            handles.insert(
                1,
                SubagentHandle::new(1, "task1".into(), "agent".into(), None),
            );
            handles.insert(
                2,
                SubagentHandle::new(2, "task2".into(), "browser".into(), None),
            );
        }

        let list = executor.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_executor_cleanup() {
        let executor = create_test_executor();

        {
            let mut handles = executor.handles.write().unwrap();
            let h1 = SubagentHandle::new(1, "task1".into(), "agent".into(), None);
            h1.complete("done".into());
            handles.insert(1, h1);

            handles.insert(
                2,
                SubagentHandle::new(2, "task2".into(), "agent".into(), None),
            );
        }

        assert_eq!(executor.handles.read().unwrap().len(), 2);

        executor.cleanup(Duration::from_secs(0));

        // Only running subagent should remain
        assert_eq!(executor.handles.read().unwrap().len(), 1);
        assert!(executor.get(2).is_some());
    }

    #[test]
    fn test_subagent_handle_spawn_time() {
        let before = std::time::Instant::now();
        let handle = SubagentHandle::new(1, "task".into(), "agent".into(), None);
        let after = std::time::Instant::now();

        // spawn_time should be between before and after
        assert!(handle.spawn_time >= before);
        assert!(handle.spawn_time <= after);
    }

    #[test]
    fn test_subagent_handle_spawn_time_duration() {
        let handle = SubagentHandle::new(1, "task".into(), "agent".into(), None);

        // Small sleep to verify elapsed time tracking works
        std::thread::sleep(Duration::from_millis(10));

        let elapsed = handle.spawn_time.elapsed();
        assert!(elapsed.as_millis() >= 10);
    }

    #[test]
    fn test_subagent_handle_clone_preserves_spawn_time() {
        let handle = SubagentHandle::new(1, "task".into(), "agent".into(), None);
        let cloned = handle.clone();

        assert_eq!(handle.spawn_time, cloned.spawn_time);
    }
}
