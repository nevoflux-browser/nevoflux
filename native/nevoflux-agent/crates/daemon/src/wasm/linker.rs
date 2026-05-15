//! Host function linker for Wasm guest modules.
//!
//! This module provides the host functions that Wasm guest modules can call
//! to interact with the NevoFlux daemon.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Engine, Linker};

use crate::error::{DaemonError, Result};
use crate::wasm::services::HostServices;
use nevoflux_storage::{
    CheckPermissionParams, MemoryChunk, MemoryRepository, PermissionRepository,
};

/// Initial capacity for the memory buffer (1MB).
const MEMORY_BUFFER_CAPACITY: usize = 1024 * 1024;

/// Status of a subagent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentStatus {
    /// Subagent is currently running.
    Running,
    /// Subagent completed successfully.
    Completed,
    /// Subagent failed with an error.
    Failed(String),
    /// Subagent was killed.
    Killed,
}

/// Information about a spawned subagent.
#[derive(Debug, Clone)]
pub struct SubagentInfo {
    /// Unique identifier for this subagent.
    pub id: u64,
    /// The task description given to the subagent.
    pub task: String,
    /// The mode the subagent is running in.
    pub mode: String,
    /// Current status.
    pub status: SubagentStatus,
    /// Result if completed.
    pub result: Option<String>,
}

/// Registry for managing subagents.
#[derive(Debug, Clone, Default)]
pub struct SubagentRegistry {
    /// Next subagent ID.
    next_id: Arc<Mutex<u64>>,
    /// Active subagents by ID.
    subagents: Arc<Mutex<HashMap<u64, SubagentInfo>>>,
}

impl SubagentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            next_id: Arc::new(Mutex::new(1)),
            subagents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn a new subagent and return its ID.
    pub fn spawn(&self, task: String, mode: String, _tab_id: Option<i64>) -> u64 {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;

        let info = SubagentInfo {
            id,
            task,
            mode,
            status: SubagentStatus::Running,
            result: None,
        };

        self.subagents.lock().unwrap().insert(id, info);
        id
    }

    /// Get the status of a subagent.
    pub fn get_status(&self, id: u64) -> Option<SubagentStatus> {
        self.subagents
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.status.clone())
    }

    /// Get a subagent's info.
    pub fn get(&self, id: u64) -> Option<SubagentInfo> {
        self.subagents.lock().unwrap().get(&id).cloned()
    }

    /// Complete a subagent with a result.
    pub fn complete(&self, id: u64, result: String) -> bool {
        if let Some(info) = self.subagents.lock().unwrap().get_mut(&id) {
            info.status = SubagentStatus::Completed;
            info.result = Some(result);
            true
        } else {
            false
        }
    }

    /// Mark a subagent as failed.
    pub fn fail(&self, id: u64, error: String) -> bool {
        if let Some(info) = self.subagents.lock().unwrap().get_mut(&id) {
            info.status = SubagentStatus::Failed(error);
            true
        } else {
            false
        }
    }

    /// Kill a subagent.
    pub fn kill(&self, id: u64) -> bool {
        if let Some(info) = self.subagents.lock().unwrap().get_mut(&id) {
            if info.status == SubagentStatus::Running {
                info.status = SubagentStatus::Killed;
                true
            } else {
                false // Already finished
            }
        } else {
            false
        }
    }

    /// Check if a subagent is still running.
    pub fn is_running(&self, id: u64) -> bool {
        self.subagents
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.status == SubagentStatus::Running)
            .unwrap_or(false)
    }

    /// Get the result of a completed subagent.
    pub fn get_result(&self, id: u64) -> Option<String> {
        self.subagents
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|s| s.result.clone())
    }

    /// Remove a subagent from the registry.
    pub fn remove(&self, id: u64) -> Option<SubagentInfo> {
        self.subagents.lock().unwrap().remove(&id)
    }

    /// List all subagent IDs.
    pub fn list_ids(&self) -> Vec<u64> {
        self.subagents.lock().unwrap().keys().cloned().collect()
    }
}

/// Host state for Wasm guest modules.
///
/// This struct holds the state that is accessible to host functions
/// when called from Wasm guest modules.
pub struct HostState {
    /// Memory buffer for passing data between host and guest.
    pub memory_buffer: Vec<u8>,

    /// Last error message from host functions.
    last_error: Option<String>,

    /// Optional services for host functions.
    ///
    /// When set, host functions can access the database, skills registry,
    /// and other services provided by the daemon.
    pub services: Option<HostServices>,

    /// Subagent registry for managing spawned subagents.
    pub subagents: SubagentRegistry,
}

impl Default for HostState {
    fn default() -> Self {
        Self::new()
    }
}

impl HostState {
    /// Create a new HostState with default values.
    pub fn new() -> Self {
        Self {
            memory_buffer: Vec::with_capacity(MEMORY_BUFFER_CAPACITY),
            last_error: None,
            services: None,
            subagents: SubagentRegistry::new(),
        }
    }

    /// Set the services for host functions.
    ///
    /// This enables host functions to access the database, skills registry,
    /// and other services provided by the daemon.
    ///
    /// # Arguments
    ///
    /// * `services` - The services container to attach.
    ///
    /// # Returns
    ///
    /// Returns self for method chaining.
    pub fn with_services(mut self, services: HostServices) -> Self {
        self.services = Some(services);
        self
    }

    /// Set the last error message.
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
    }

    /// Take the last error message, clearing it.
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    /// Check if services are available.
    pub fn has_services(&self) -> bool {
        self.services.is_some()
    }
}

impl std::fmt::Debug for HostState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostState")
            .field("memory_buffer_capacity", &self.memory_buffer.capacity())
            .field("memory_buffer_len", &self.memory_buffer.len())
            .field("last_error", &self.last_error)
            .field("services", &self.services.as_ref().map(|_| "Some(...)"))
            .finish()
    }
}

/// Create a Linker with host functions registered under the "nevoflux" namespace.
///
/// # Arguments
///
/// * `engine` - The Wasmtime engine to create the linker for.
///
/// # Returns
///
/// A configured Linker with all host functions registered.
///
/// # Errors
///
/// Returns an error if any host function fails to register.
pub fn create_linker(engine: &Engine) -> Result<Linker<HostState>> {
    let mut linker = Linker::new(engine);

    // Register get_error_len: returns the length of the last error message
    linker
        .func_wrap(
            "nevoflux",
            "get_error_len",
            |caller: Caller<'_, HostState>| -> i32 {
                caller
                    .data()
                    .last_error
                    .as_ref()
                    .map(|e| e.len() as i32)
                    .unwrap_or(0)
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register get_error_len: {}", e))
        })?;

    // Register get_error: copies the error message to guest memory
    linker
        .func_wrap(
            "nevoflux",
            "get_error",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                let error = match caller.data().last_error.as_ref() {
                    Some(e) => e.clone(),
                    None => return 0,
                };

                let error_bytes = error.as_bytes();
                let copy_len = std::cmp::min(error_bytes.len(), len as usize);

                // Get the memory export from the guest
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => {
                        tracing::error!("Guest module has no memory export");
                        return -1;
                    }
                };

                // Write error bytes to guest memory
                match memory.write(&mut caller, ptr as usize, &error_bytes[..copy_len]) {
                    Ok(()) => copy_len as i32,
                    Err(e) => {
                        tracing::error!("Failed to write error to guest memory: {}", e);
                        -1
                    }
                }
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register get_error: {}", e)))?;

    // Register llm_chat: performs LLM chat completion
    // llm_chat: request_ptr, request_len, response_ptr, response_len -> bytes written or -1
    linker
        .func_wrap(
            "nevoflux",
            "llm_chat",
            |mut caller: Caller<'_, HostState>,
             request_ptr: i32,
             request_len: i32,
             response_ptr: i32,
             response_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => {
                        tracing::error!("Guest module has no memory export");
                        return -1;
                    }
                };

                // Read JSON request from guest memory
                let mut request_buf = vec![0u8; request_len as usize];
                if memory
                    .read(&caller, request_ptr as usize, &mut request_buf)
                    .is_err()
                {
                    caller
                        .data_mut()
                        .set_error("Failed to read request from memory");
                    return -1;
                }

                // Parse the request JSON
                let request_str = match String::from_utf8(request_buf) {
                    Ok(s) => s,
                    Err(_) => {
                        caller.data_mut().set_error("Invalid UTF-8 in request");
                        return -1;
                    }
                };

                let request: crate::wasm::llm::LlmChatRequest =
                    match serde_json::from_str(&request_str) {
                        Ok(r) => r,
                        Err(e) => {
                            caller
                                .data_mut()
                                .set_error(format!("Failed to parse request: {}", e));
                            return -1;
                        }
                    };

                // Get LLM config from services
                let services = match &caller.data().services {
                    Some(s) => s.clone(),
                    None => {
                        caller.data_mut().set_error("Services not available");
                        return -1;
                    }
                };

                let llm_config = match &services.llm_config {
                    Some(c) => c.clone(),
                    None => {
                        caller.data_mut().set_error("LLM not configured");
                        return -1;
                    }
                };

                // Execute the LLM chat using the current tokio runtime
                let result = match tokio::runtime::Handle::try_current() {
                    Ok(handle) => std::thread::scope(|s| {
                        s.spawn(|| {
                            handle.block_on(crate::wasm::llm::execute_llm_chat(
                                llm_config.provider,
                                &llm_config.api_key,
                                &llm_config.model,
                                request,
                                llm_config.base_url.as_deref(),
                            ))
                        })
                        .join()
                        .expect("Thread panicked")
                    }),
                    Err(_) => {
                        caller.data_mut().set_error("No tokio runtime available");
                        return -1;
                    }
                };

                let response = match result {
                    Ok(r) => r,
                    Err(e) => {
                        caller
                            .data_mut()
                            .set_error(format!("LLM chat failed: {}", e));
                        return -1;
                    }
                };

                // Serialize response to JSON
                let response_json = match serde_json::to_string(&response) {
                    Ok(j) => j,
                    Err(e) => {
                        caller
                            .data_mut()
                            .set_error(format!("Failed to serialize response: {}", e));
                        return -1;
                    }
                };

                // Write response to guest memory
                let response_bytes = response_json.as_bytes();
                let write_len = std::cmp::min(response_bytes.len(), response_len as usize);

                if memory
                    .write(
                        &mut caller,
                        response_ptr as usize,
                        &response_bytes[..write_len],
                    )
                    .is_err()
                {
                    caller
                        .data_mut()
                        .set_error("Failed to write response to memory");
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register llm_chat: {}", e)))?;

    // Register memory_create: creates a memory chunk and returns the ID
    // memory_create: content_ptr, content_len, metadata_ptr, metadata_len, result_ptr, result_len -> id_len or -1
    linker
        .func_wrap(
            "nevoflux",
            "memory_create",
            |mut caller: Caller<'_, HostState>,
             content_ptr: i32,
             content_len: i32,
             _metadata_ptr: i32,
             _metadata_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read content from guest memory
                let mut content_buf = vec![0u8; content_len as usize];
                if memory
                    .read(&caller, content_ptr as usize, &mut content_buf)
                    .is_err()
                {
                    return -1;
                }

                let content = match String::from_utf8(content_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Generate UUID for ID
                let id = format!("mem-{}", uuid::Uuid::new_v4());

                // Store in database if services available
                if let Some(services) = &caller.data().services {
                    let chunk = MemoryChunk::new(&content).with_id(&id);
                    let repo = MemoryRepository::new(&services.database);
                    if repo.create(&chunk).is_err() {
                        // Still return the ID even if storage fails
                        tracing::warn!("Failed to store memory chunk in database");
                    }
                }

                // Write ID to guest memory
                let id_bytes = id.as_bytes();
                let write_len = std::cmp::min(id_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &id_bytes[..write_len])
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register memory_create: {}", e))
        })?;

    // Register memory_delete: deletes a memory chunk
    // memory_delete: id_ptr, id_len -> 1 for success, 0 for failure
    linker
        .func_wrap(
            "nevoflux",
            "memory_delete",
            |mut caller: Caller<'_, HostState>, id_ptr: i32, id_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                // Read ID from guest memory
                let mut id_buf = vec![0u8; id_len as usize];
                if memory.read(&caller, id_ptr as usize, &mut id_buf).is_err() {
                    return 0;
                }

                let id = match String::from_utf8(id_buf) {
                    Ok(s) => s,
                    Err(_) => return 0,
                };

                // Delete from database if services available
                match &caller.data().services {
                    Some(services) => {
                        let repo = MemoryRepository::new(&services.database);
                        match repo.delete(&id) {
                            Ok(true) => 1,  // Successfully deleted
                            Ok(false) => 0, // Not found
                            Err(_) => 0,    // Error
                        }
                    }
                    None => 1, // No services, return success (no-op)
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register memory_delete: {}", e))
        })?;

    // Register memory_search: searches memory and returns results
    // memory_search: query_ptr, query_len, limit, results_ptr, results_len -> bytes written or -1
    linker
        .func_wrap(
            "nevoflux",
            "memory_search",
            |mut caller: Caller<'_, HostState>,
             query_ptr: i32,
             query_len: i32,
             limit: i32,
             results_ptr: i32,
             results_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read query from guest memory
                let mut query_buf = vec![0u8; query_len as usize];
                if memory
                    .read(&caller, query_ptr as usize, &mut query_buf)
                    .is_err()
                {
                    return -1;
                }

                let query = match String::from_utf8(query_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Get database from services and search
                let result_json = match &caller.data().services {
                    Some(services) => {
                        let repo = MemoryRepository::new(&services.database);
                        let limit = if limit <= 0 { 10 } else { limit as usize };

                        match repo.search_fts(&query, limit) {
                            Ok(chunks) => {
                                // Serialize results to JSON with id, content, metadata fields
                                let results: Vec<serde_json::Value> = chunks
                                    .into_iter()
                                    .map(|chunk| {
                                        serde_json::json!({
                                            "id": chunk.id,
                                            "content": chunk.content,
                                            "metadata": chunk.metadata
                                        })
                                    })
                                    .collect();
                                serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
                            }
                            Err(_) => "[]".to_string(),
                        }
                    }
                    None => "[]".to_string(),
                };

                let result_bytes = result_json.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), results_len as usize);

                if memory
                    .write(
                        &mut caller,
                        results_ptr as usize,
                        &result_bytes[..write_len],
                    )
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register memory_search: {}", e))
        })?;

    // Register memory_update: updates an existing memory chunk
    // memory_update: id_ptr, id_len, content_ptr, content_len -> 1 for success, 0 for not found, -1 for error
    linker
        .func_wrap(
            "nevoflux",
            "memory_update",
            |mut caller: Caller<'_, HostState>,
             id_ptr: i32,
             id_len: i32,
             content_ptr: i32,
             content_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read ID from guest memory
                let mut id_buf = vec![0u8; id_len as usize];
                if memory.read(&caller, id_ptr as usize, &mut id_buf).is_err() {
                    return -1;
                }
                let id = match String::from_utf8(id_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Read content from guest memory
                let mut content_buf = vec![0u8; content_len as usize];
                if memory
                    .read(&caller, content_ptr as usize, &mut content_buf)
                    .is_err()
                {
                    return -1;
                }
                let content = match String::from_utf8(content_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Get the database and update
                match caller.data().services.as_ref() {
                    Some(services) => {
                        let repo = MemoryRepository::new(&services.database);
                        match repo.update(&id, &content) {
                            Ok(true) => 1,  // Updated successfully
                            Ok(false) => 0, // Not found
                            Err(e) => {
                                caller.data_mut().set_error(e.to_string());
                                -1
                            }
                        }
                    }
                    None => -1,
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register memory_update: {}", e))
        })?;

    // Register memory_get: retrieves a memory chunk by ID
    // memory_get: id_ptr, id_len, result_ptr, result_len -> bytes written or 0 (not found) or -1 (error)
    linker
        .func_wrap(
            "nevoflux",
            "memory_get",
            |mut caller: Caller<'_, HostState>,
             id_ptr: i32,
             id_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read ID from guest memory
                let mut id_buf = vec![0u8; id_len as usize];
                if memory.read(&caller, id_ptr as usize, &mut id_buf).is_err() {
                    return -1;
                }
                let id = match String::from_utf8(id_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Get the database and fetch the memory chunk
                match caller.data().services.as_ref() {
                    Some(services) => {
                        let repo = MemoryRepository::new(&services.database);
                        match repo.get(&id) {
                            Ok(Some(chunk)) => {
                                // Serialize to JSON
                                let json = match serde_json::to_string(&chunk) {
                                    Ok(j) => j,
                                    Err(e) => {
                                        caller.data_mut().set_error(e.to_string());
                                        return -1;
                                    }
                                };

                                let result_bytes = json.as_bytes();
                                let write_len =
                                    std::cmp::min(result_bytes.len(), result_len as usize);

                                if memory
                                    .write(
                                        &mut caller,
                                        result_ptr as usize,
                                        &result_bytes[..write_len],
                                    )
                                    .is_err()
                                {
                                    return -1;
                                }

                                write_len as i32
                            }
                            Ok(None) => 0, // Not found
                            Err(e) => {
                                caller.data_mut().set_error(e.to_string());
                                -1
                            }
                        }
                    }
                    None => -1,
                }
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register memory_get: {}", e)))?;

    // Register permission_check: checks permission against database
    // permission_check: resource_ptr, resource_len, action_ptr, action_len -> 1 (allowed), 0 (denied), -1 (error)
    linker
        .func_wrap(
            "nevoflux",
            "permission_check",
            |mut caller: Caller<'_, HostState>,
             resource_ptr: i32,
             resource_len: i32,
             action_ptr: i32,
             action_len: i32|
             -> i32 {
                // If no services available, allow in development mode
                let services = match &caller.data().services {
                    Some(s) => s.clone(),
                    None => return 1, // Allow in development mode
                };

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => {
                        tracing::error!("Guest module has no memory export");
                        return -1;
                    }
                };

                // Read resource string from guest memory
                let mut resource_buf = vec![0u8; resource_len as usize];
                if memory
                    .read(&caller, resource_ptr as usize, &mut resource_buf)
                    .is_err()
                {
                    caller
                        .data_mut()
                        .set_error("Failed to read resource from memory");
                    return -1;
                }

                let resource = match String::from_utf8(resource_buf) {
                    Ok(s) => s,
                    Err(_) => {
                        caller.data_mut().set_error("Invalid UTF-8 in resource");
                        return -1;
                    }
                };

                // Read action string from guest memory
                let mut action_buf = vec![0u8; action_len as usize];
                if memory
                    .read(&caller, action_ptr as usize, &mut action_buf)
                    .is_err()
                {
                    caller
                        .data_mut()
                        .set_error("Failed to read action from memory");
                    return -1;
                }

                let action = match String::from_utf8(action_buf) {
                    Ok(s) => s,
                    Err(_) => {
                        caller.data_mut().set_error("Invalid UTF-8 in action");
                        return -1;
                    }
                };

                // Check permission using database
                // Use "default" as session_id for now
                let repo = PermissionRepository::new(&services.database);
                let params = CheckPermissionParams::new("resource", &action, &resource)
                    .with_session_id("default");

                match repo.check(params) {
                    Ok(Some(true)) => 1,  // Permission granted
                    Ok(Some(false)) => 0, // Permission denied
                    Ok(None) => 1,        // No permission found, allow by default
                    Err(e) => {
                        caller
                            .data_mut()
                            .set_error(format!("Permission check failed: {}", e));
                        -1
                    }
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register permission_check: {}", e))
        })?;

    // Register skill_list: returns JSON array of skill summaries
    // skill_list: result_ptr, result_len -> bytes written or -1
    linker
        .func_wrap(
            "nevoflux",
            "skill_list",
            |mut caller: Caller<'_, HostState>, result_ptr: i32, result_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Get skill summaries from registry
                let result_json = match &caller.data().services {
                    Some(services) => {
                        let skills = services.skills.clone();

                        // Use tokio runtime to access the async RwLock
                        match tokio::runtime::Handle::try_current() {
                            Ok(handle) => std::thread::scope(|s| {
                                s.spawn(|| {
                                    handle.block_on(async {
                                        let registry = skills.read().await;
                                        let summaries = registry.list();

                                        // Serialize to JSON array with name, description, tags
                                        let json_summaries: Vec<serde_json::Value> = summaries
                                            .into_iter()
                                            .map(|s| {
                                                serde_json::json!({
                                                    "name": s.name,
                                                    "description": s.description,
                                                    "tags": s.tags
                                                })
                                            })
                                            .collect();

                                        serde_json::to_string(&json_summaries)
                                            .unwrap_or_else(|_| "[]".to_string())
                                    })
                                })
                                .join()
                                .expect("Thread panicked")
                            }),
                            Err(_) => "[]".to_string(),
                        }
                    }
                    None => "[]".to_string(),
                };

                let result_bytes = result_json.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register skill_list: {}", e)))?;

    // Register skill_load: loads a skill by name
    // skill_load: name_ptr, name_len, result_ptr, result_len -> bytes written or -1 (not found) or -2 (error)
    linker
        .func_wrap(
            "nevoflux",
            "skill_load",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -2,
                };

                // Read skill name from guest memory
                let mut name_buf = vec![0u8; name_len as usize];
                if memory
                    .read(&caller, name_ptr as usize, &mut name_buf)
                    .is_err()
                {
                    return -2;
                }
                let name = match String::from_utf8(name_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };

                // Get skill from registry
                let skill_json = match &caller.data().services {
                    Some(services) => {
                        let skills = services.skills.clone();

                        // Use tokio runtime to access the async RwLock
                        match tokio::runtime::Handle::try_current() {
                            Ok(handle) => std::thread::scope(|s| {
                                s.spawn(|| {
                                    handle.block_on(async {
                                        let registry = skills.read().await;
                                        match registry.get(&name) {
                                            Some(skill) => {
                                                // Serialize skill to JSON with name, description, content, tags
                                                let json = serde_json::json!({
                                                    "name": skill.name(),
                                                    "description": skill.description(),
                                                    "content": skill.content,
                                                    "tags": skill.metadata.tags
                                                });
                                                Some(
                                                    serde_json::to_string(&json)
                                                        .unwrap_or_else(|_| "{}".to_string()),
                                                )
                                            }
                                            None => None,
                                        }
                                    })
                                })
                                .join()
                                .expect("Thread panicked")
                            }),
                            Err(_) => None,
                        }
                    }
                    None => None,
                };

                // Return -1 if skill not found
                let skill_json = match skill_json {
                    Some(json) => json,
                    None => return -1,
                };

                let result_bytes = skill_json.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -2;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register skill_load: {}", e)))?;

    // Register skill_read: reads an auxiliary file from a skill's directory (Level 3 loading)
    // skill_read: name_ptr, name_len, path_ptr, path_len, result_ptr, result_len -> bytes written or -1 (not found) or -2 (error)
    linker
        .func_wrap(
            "nevoflux",
            "skill_read",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             path_ptr: i32,
             path_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -2,
                };

                // Read skill name from guest memory
                let mut name_buf = vec![0u8; name_len as usize];
                if memory
                    .read(&caller, name_ptr as usize, &mut name_buf)
                    .is_err()
                {
                    return -2;
                }
                let skill_name = match String::from_utf8(name_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };

                // Read path from guest memory
                let mut path_buf = vec![0u8; path_len as usize];
                if memory
                    .read(&caller, path_ptr as usize, &mut path_buf)
                    .is_err()
                {
                    return -2;
                }
                let relative_path = match String::from_utf8(path_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };

                // Try to read the auxiliary file
                let content = match caller.data().services.as_ref() {
                    Some(services) => {
                        let registry = services.skills.blocking_read();
                        match registry.read_auxiliary_file(&skill_name, &relative_path) {
                            Ok(content) => content,
                            Err(_) => return -1, // Not found or read error
                        }
                    }
                    None => return -1,
                };

                let result_bytes = content.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -2;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register skill_read: {}", e)))?;

    // Register skill_execute: executes a script from a skill's directory (Level 3 loading)
    // skill_execute: name_ptr, name_len, script_ptr, script_len, args_ptr, args_len, result_ptr, result_len -> bytes written or -1 (not found) or -2 (error)
    linker
        .func_wrap(
            "nevoflux",
            "skill_execute",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             script_ptr: i32,
             script_len: i32,
             args_ptr: i32,
             args_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -2,
                };

                // Read skill name from guest memory
                let mut name_buf = vec![0u8; name_len as usize];
                if memory
                    .read(&caller, name_ptr as usize, &mut name_buf)
                    .is_err()
                {
                    return -2;
                }
                let skill_name = match String::from_utf8(name_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };

                // Read script path from guest memory
                let mut script_buf = vec![0u8; script_len as usize];
                if memory
                    .read(&caller, script_ptr as usize, &mut script_buf)
                    .is_err()
                {
                    return -2;
                }
                let script_path = match String::from_utf8(script_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };

                // Read args JSON from guest memory
                let mut args_buf = vec![0u8; args_len as usize];
                if memory
                    .read(&caller, args_ptr as usize, &mut args_buf)
                    .is_err()
                {
                    return -2;
                }
                let args_str = match String::from_utf8(args_buf) {
                    Ok(s) => s,
                    Err(_) => return -2,
                };
                let args: serde_json::Value = match serde_json::from_str(&args_str) {
                    Ok(v) => v,
                    Err(_) => return -2,
                };

                // Try to execute the script
                // Clone the skills Arc to avoid borrow issues
                let skills = match caller.data().services.as_ref() {
                    Some(services) => services.skills.clone(),
                    None => return -1,
                };

                let output = {
                    let registry = skills.blocking_read();
                    match registry.execute_script(&skill_name, &script_path, &args) {
                        Ok(output) => output,
                        Err(e) => {
                            drop(registry); // Explicitly drop before mutable borrow
                            caller.data_mut().set_error(e.to_string());
                            return -1;
                        }
                    }
                };

                let result_bytes = output.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -2;
                }

                write_len as i32
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register skill_execute: {}", e))
        })?;

    // Register subagent_spawn: spawns a new subagent
    // subagent_spawn: task_ptr, task_len, mode_ptr, mode_len, tab_id -> subagent_id (u64 as i64) or -1 on error
    // tab_id: -1 means None, otherwise the tab ID
    linker
        .func_wrap(
            "nevoflux",
            "subagent_spawn",
            |mut caller: Caller<'_, HostState>,
             task_ptr: i32,
             task_len: i32,
             mode_ptr: i32,
             mode_len: i32,
             tab_id: i64|
             -> i64 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read task from guest memory
                let mut task_buf = vec![0u8; task_len as usize];
                if memory
                    .read(&caller, task_ptr as usize, &mut task_buf)
                    .is_err()
                {
                    return -1;
                }
                let task = match String::from_utf8(task_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Read mode from guest memory
                let mut mode_buf = vec![0u8; mode_len as usize];
                if memory
                    .read(&caller, mode_ptr as usize, &mut mode_buf)
                    .is_err()
                {
                    return -1;
                }
                let mode = match String::from_utf8(mode_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Spawn the subagent
                let opt_tab_id = if tab_id < 0 { None } else { Some(tab_id) };
                let id = caller.data().subagents.spawn(task, mode, opt_tab_id);
                id as i64
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register subagent_spawn: {}", e))
        })?;

    // Register subagent_status: gets the status of a subagent
    // subagent_status: subagent_id -> status (0=running, 1=completed, 2=failed, 3=killed, -1=not found)
    linker
        .func_wrap(
            "nevoflux",
            "subagent_status",
            |caller: Caller<'_, HostState>, subagent_id: i64| -> i32 {
                match caller.data().subagents.get_status(subagent_id as u64) {
                    Some(SubagentStatus::Running) => 0,
                    Some(SubagentStatus::Completed) => 1,
                    Some(SubagentStatus::Failed(_)) => 2,
                    Some(SubagentStatus::Killed) => 3,
                    None => -1,
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register subagent_status: {}", e))
        })?;

    // Register subagent_wait: waits for a subagent and gets its result
    // subagent_wait: subagent_id, result_ptr, result_len -> bytes written or -1 (not found) or -2 (still running) or -3 (failed/killed)
    linker
        .func_wrap(
            "nevoflux",
            "subagent_wait",
            |mut caller: Caller<'_, HostState>,
             subagent_id: i64,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                let info = match caller.data().subagents.get(subagent_id as u64) {
                    Some(info) => info,
                    None => return -1, // Not found
                };

                match info.status {
                    SubagentStatus::Running => -2, // Still running
                    SubagentStatus::Completed => {
                        // Return the result
                        if let Some(result) = info.result {
                            let result_bytes = result.as_bytes();
                            let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                            if memory
                                .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                                .is_err()
                            {
                                return -1;
                            }

                            write_len as i32
                        } else {
                            0 // Completed with no result
                        }
                    }
                    SubagentStatus::Failed(_) | SubagentStatus::Killed => -3, // Failed or killed
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register subagent_wait: {}", e))
        })?;

    // Register subagent_kill: kills a running subagent
    // subagent_kill: subagent_id -> 1 (success), 0 (already finished), -1 (not found)
    linker
        .func_wrap(
            "nevoflux",
            "subagent_kill",
            |caller: Caller<'_, HostState>, subagent_id: i64| -> i32 {
                match caller.data().subagents.get(subagent_id as u64) {
                    Some(_) => {
                        if caller.data().subagents.kill(subagent_id as u64) {
                            1 // Successfully killed
                        } else {
                            0 // Already finished
                        }
                    }
                    None => -1, // Not found
                }
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register subagent_kill: {}", e))
        })?;

    // Register subagent_list: lists all subagent IDs
    // subagent_list: result_ptr, result_len -> bytes written (JSON array of IDs) or -1 on error
    linker
        .func_wrap(
            "nevoflux",
            "subagent_list",
            |mut caller: Caller<'_, HostState>, result_ptr: i32, result_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                let ids = caller.data().subagents.list_ids();
                let json = match serde_json::to_string(&ids) {
                    Ok(j) => j,
                    Err(_) => return -1,
                };

                let result_bytes = json.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register subagent_list: {}", e))
        })?;

    // Register tool_read: reads file contents
    // tool_read: path_ptr, path_len, offset, limit, result_ptr, result_len -> bytes written or -1
    linker
        .func_wrap(
            "nevoflux",
            "tool_read",
            |mut caller: Caller<'_, HostState>,
             path_ptr: i32,
             path_len: i32,
             offset: i64,
             limit: i64,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read path from guest memory
                let mut path_buf = vec![0u8; path_len as usize];
                if memory
                    .read(&caller, path_ptr as usize, &mut path_buf)
                    .is_err()
                {
                    return -1;
                }
                let path = match String::from_utf8(path_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Read file (permission check would be needed in production)
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        caller
                            .data_mut()
                            .set_error(format!("Failed to read {}: {}", path, e));
                        return -1;
                    }
                };

                let total_bytes = content.len() as u64;
                let total_lines = content.lines().count() as u64;

                // Apply offset and limit (line-based)
                let lines: Vec<&str> = content.lines().collect();
                let start_line = (offset as usize).min(lines.len());
                let end_line = if limit > 0 {
                    (start_line + limit as usize).min(lines.len())
                } else {
                    lines.len()
                };

                let selected: Vec<&str> = lines[start_line..end_line].to_vec();
                let returned_lines = selected.len() as u64;
                let truncated = end_line < lines.len();
                let selected_content = selected.join("\n");

                let read_result = nevoflux_protocol::ReadResult {
                    total_lines,
                    total_bytes,
                    returned_lines,
                    offset: start_line as u64,
                    content: selected_content,
                    truncated,
                };

                let json = match serde_json::to_string(&read_result) {
                    Ok(j) => j,
                    Err(e) => {
                        caller
                            .data_mut()
                            .set_error(format!("Failed to serialize ReadResult: {}", e));
                        return -1;
                    }
                };

                let json_bytes = json.as_bytes();
                let write_len = json_bytes.len().min(result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &json_bytes[..write_len])
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register tool_read: {}", e)))?;

    // Register tool_glob: glob pattern matching
    // tool_glob: pattern_ptr, pattern_len, base_ptr, base_len, result_ptr, result_len -> bytes written or -1
    linker
        .func_wrap(
            "nevoflux",
            "tool_glob",
            |mut caller: Caller<'_, HostState>,
             pattern_ptr: i32,
             pattern_len: i32,
             base_ptr: i32,
             base_len: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read pattern
                let mut pattern_buf = vec![0u8; pattern_len as usize];
                if memory
                    .read(&caller, pattern_ptr as usize, &mut pattern_buf)
                    .is_err()
                {
                    return -1;
                }
                let pattern = match String::from_utf8(pattern_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Read base path (optional)
                let base = if base_len > 0 {
                    let mut base_buf = vec![0u8; base_len as usize];
                    if memory
                        .read(&caller, base_ptr as usize, &mut base_buf)
                        .is_err()
                    {
                        ".".to_string()
                    } else {
                        String::from_utf8(base_buf).unwrap_or_else(|_| ".".to_string())
                    }
                } else {
                    ".".to_string()
                };

                // Execute glob
                let full_pattern = if base == "." {
                    pattern
                } else {
                    format!("{}/{}", base, pattern)
                };

                let paths: Vec<String> = glob::glob(&full_pattern)
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .map(|p| p.display().to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let result = serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string());
                let result_bytes = result.as_bytes();
                let write_len = result_bytes.len().min(result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -1;
                }

                write_len as i32
            },
        )
        .map_err(|e| DaemonError::InternalError(format!("Failed to register tool_glob: {}", e)))?;

    // Register tool_search: search tools by keyword using BM25 ranking
    // tool_search: query_ptr, query_len, max_results, result_ptr, result_len -> bytes written or -1 (no services) or -2 (error)
    linker
        .func_wrap(
            "nevoflux",
            "tool_search",
            |mut caller: Caller<'_, HostState>,
             query_ptr: i32,
             query_len: i32,
             max_results: i32,
             result_ptr: i32,
             result_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                // Read query from guest memory
                let mut query_buf = vec![0u8; query_len as usize];
                if memory
                    .read(&caller, query_ptr as usize, &mut query_buf)
                    .is_err()
                {
                    return -1;
                }
                let query = match String::from_utf8(query_buf) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                // Get tool search index from services
                let services = match &caller.data().services {
                    Some(s) => s.clone(),
                    None => return -1, // No services configured
                };

                let tool_search = match &services.tool_search {
                    Some(ts) => ts.clone(),
                    None => {
                        // Return empty array if no tool search configured
                        let empty = "[]";
                        let empty_bytes = empty.as_bytes();
                        let write_len = std::cmp::min(empty_bytes.len(), result_len as usize);
                        if memory
                            .write(&mut caller, result_ptr as usize, &empty_bytes[..write_len])
                            .is_err()
                        {
                            return -2;
                        }
                        return write_len as i32;
                    }
                };

                // Perform the search
                let results = {
                    let index = tool_search.blocking_read();
                    if max_results > 0 {
                        index.search_limit(&query, max_results as usize)
                    } else {
                        index.search(&query)
                    }
                };

                // Convert results to JSON
                let json_results: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "name": r.tool.name,
                            "description": r.tool.description,
                            "score": r.score,
                            "input_schema": r.tool.input_schema
                        })
                    })
                    .collect();

                let json = match serde_json::to_string(&json_results) {
                    Ok(j) => j,
                    Err(_) => return -2,
                };

                // Write result to guest memory
                let result_bytes = json.as_bytes();
                let write_len = std::cmp::min(result_bytes.len(), result_len as usize);

                if memory
                    .write(&mut caller, result_ptr as usize, &result_bytes[..write_len])
                    .is_err()
                {
                    return -2;
                }

                write_len as i32
            },
        )
        .map_err(|e| {
            DaemonError::InternalError(format!("Failed to register tool_search: {}", e))
        })?;

    Ok(linker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_skills::{Skill, SkillMetadata, SkillRegistry};
    use nevoflux_storage::Database;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wasmtime::{Module, Store};

    #[test]
    fn test_host_state_default() {
        let state = HostState::default();

        assert_eq!(state.memory_buffer.capacity(), MEMORY_BUFFER_CAPACITY);
        assert!(state.memory_buffer.is_empty());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_host_state_error() {
        let mut state = HostState::new();

        // Initially no error
        assert!(state.take_error().is_none());

        // Set an error
        state.set_error("Test error message");
        assert!(state.last_error.is_some());

        // Take the error
        let error = state.take_error();
        assert_eq!(error, Some("Test error message".to_string()));

        // Error should be cleared
        assert!(state.take_error().is_none());
    }

    #[test]
    fn test_create_linker() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Verify all functions are registered by creating a module that imports them
        let wat = r#"
            (module
                (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
                (import "nevoflux" "get_error" (func $get_error (param i32 i32) (result i32)))
                (import "nevoflux" "llm_chat" (func $llm_chat (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "memory_create" (func $memory_create (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
                (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "memory_update" (func $memory_update (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "memory_get" (func $memory_get (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))
                (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "skill_read" (func $skill_read (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "skill_execute" (func $skill_execute (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "subagent_spawn" (func $subagent_spawn (param i32 i32 i32 i32 i64) (result i64)))
                (import "nevoflux" "subagent_status" (func $subagent_status (param i64) (result i32)))
                (import "nevoflux" "subagent_wait" (func $subagent_wait (param i64 i32 i32) (result i32)))
                (import "nevoflux" "subagent_kill" (func $subagent_kill (param i64) (result i32)))
                (import "nevoflux" "subagent_list" (func $subagent_list (param i32 i32) (result i32)))
                (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
                (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "tool_search" (func $tool_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());

        // Instantiate should succeed if all imports are satisfied
        let instance = linker.instantiate(&mut store, &module);
        assert!(
            instance.is_ok(),
            "Failed to instantiate module with linker: {:?}",
            instance.err()
        );
    }

    #[test]
    fn test_get_error_len_no_error() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
                (memory (export "memory") 1)
                (func (export "test") (result i32)
                    call $get_error_len
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_get_error_len_with_error() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
                (memory (export "memory") 1)
                (func (export "test") (result i32)
                    call $get_error_len
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut state = HostState::new();
        state.set_error("Test error");
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 10); // "Test error".len()
    }

    #[test]
    fn test_llm_chat_no_services() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "llm_chat" (func $llm_chat (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{\"messages\":[{\"role\":\"user\",\"content\":\"Hello\"}]}")
                (func (export "test") (result i32)
                    i32.const 0   ;; request_ptr
                    i32.const 52  ;; request_len
                    i32.const 100 ;; response_ptr
                    i32.const 256 ;; response_len
                    call $llm_chat
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // Returns -1 because no services are configured
        assert_eq!(result, -1);
    }

    #[test]
    fn test_llm_chat_invalid_json() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "llm_chat" (func $llm_chat (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "not valid json")
                (func (export "test") (result i32)
                    i32.const 0   ;; request_ptr
                    i32.const 14  ;; request_len
                    i32.const 100 ;; response_ptr
                    i32.const 256 ;; response_len
                    call $llm_chat
                )
                (func (export "error_len") (result i32)
                    call $get_error_len
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // Returns -1 because JSON is invalid
        assert_eq!(result, -1);

        // Error should be set
        let error_len_func = instance
            .get_typed_func::<(), i32>(&mut store, "error_len")
            .expect("Failed to get error_len function");
        let error_len = error_len_func
            .call(&mut store, ())
            .expect("Failed to call error_len");
        assert!(error_len > 0);
    }

    #[test]
    fn test_llm_chat_no_llm_config() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "llm_chat" (func $llm_chat (param i32 i32 i32 i32) (result i32)))
                (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{\"messages\":[{\"role\":\"user\",\"content\":\"Hi\"}]}")
                (func (export "test") (result i32)
                    i32.const 0   ;; request_ptr
                    i32.const 48  ;; request_len
                    i32.const 100 ;; response_ptr
                    i32.const 256 ;; response_len
                    call $llm_chat
                )
                (func (export "error_len") (result i32)
                    call $get_error_len
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        // Create services without LLM config
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // Returns -1 because LLM is not configured
        assert_eq!(result, -1);

        // Error should mention LLM not configured
        let error_len_func = instance
            .get_typed_func::<(), i32>(&mut store, "error_len")
            .expect("Failed to get error_len function");
        let error_len = error_len_func
            .call(&mut store, ())
            .expect("Failed to call error_len");
        assert!(error_len > 0);
    }

    #[test]
    fn test_memory_search_empty() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "test") (result i32)
                    i32.const 0   ;; query_ptr
                    i32.const 0   ;; query_len
                    i32.const 10  ;; limit
                    i32.const 100 ;; results_ptr
                    i32.const 256 ;; results_len
                    call $memory_search
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 2); // Returns "[]" which is 2 bytes
    }

    #[test]
    fn test_permission_check_no_services() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Test with no services - should return 1 (allowed in development mode)
        let wat = r#"
            (module
                (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/home/user/file.txt")
                (data (i32.const 50) "read")
                (func (export "test") (result i32)
                    i32.const 0   ;; resource_ptr
                    i32.const 19  ;; resource_len ("/home/user/file.txt")
                    i32.const 50  ;; action_ptr
                    i32.const 4   ;; action_len ("read")
                    call $permission_check
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Allowed in development mode (no services)
    }

    #[test]
    fn test_permission_check_with_database_no_permission() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Test with database but no permission set - should return 1 (allowed by default)
        let wat = r#"
            (module
                (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/home/user/file.txt")
                (data (i32.const 50) "read")
                (func (export "test") (result i32)
                    i32.const 0   ;; resource_ptr
                    i32.const 19  ;; resource_len
                    i32.const 50  ;; action_ptr
                    i32.const 4   ;; action_len
                    call $permission_check
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open database"));
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Allowed by default when no permission is set
    }

    #[test]
    fn test_permission_check_granted() {
        use nevoflux_storage::{CreatePermissionParams, PermissionScope};

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/allowed/path")
                (data (i32.const 50) "read")
                (func (export "test") (result i32)
                    i32.const 0   ;; resource_ptr
                    i32.const 13  ;; resource_len ("/allowed/path")
                    i32.const 50  ;; action_ptr
                    i32.const 4   ;; action_len ("read")
                    call $permission_check
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open database"));

        // Create a permission that grants access
        let repo = PermissionRepository::new(&db);
        repo.create(
            CreatePermissionParams::new("resource", "read", "/allowed/path")
                .with_scope(PermissionScope::Session)
                .with_session_id("default")
                .with_granted(true),
        )
        .expect("Failed to create permission");

        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Permission granted
    }

    #[test]
    fn test_permission_check_denied() {
        use nevoflux_storage::{CreatePermissionParams, PermissionScope};

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/denied/path")
                (data (i32.const 50) "write")
                (func (export "test") (result i32)
                    i32.const 0   ;; resource_ptr
                    i32.const 12  ;; resource_len ("/denied/path")
                    i32.const 50  ;; action_ptr
                    i32.const 5   ;; action_len ("write")
                    call $permission_check
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open database"));

        // Create a permission that denies access
        let repo = PermissionRepository::new(&db);
        repo.create(
            CreatePermissionParams::new("resource", "write", "/denied/path")
                .with_scope(PermissionScope::Session)
                .with_session_id("default")
                .with_granted(false),
        )
        .expect("Failed to create permission");

        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 0); // Permission denied
    }

    #[test]
    fn test_memory_create() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // WAT module that calls memory_create and returns the length of the generated ID
        let wat = r#"
            (module
                (import "nevoflux" "memory_create" (func $memory_create (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test content")
                (func (export "test") (result i32)
                    i32.const 0    ;; content_ptr
                    i32.const 12   ;; content_len ("test content")
                    i32.const 0    ;; metadata_ptr (unused)
                    i32.const 0    ;; metadata_len (unused)
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $memory_create
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // ID format is "mem-{uuid}" which is 4 + 36 = 40 characters
        assert_eq!(result, 40);
    }

    #[test]
    fn test_memory_delete() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "mem-12345678-1234-1234-1234-123456789012")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 40  ;; id_len
                    call $memory_delete
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Success
    }

    #[test]
    fn test_skill_list_empty() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "test") (result i32)
                    i32.const 100 ;; result_ptr
                    i32.const 256 ;; result_len
                    call $skill_list
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 2); // Returns "[]" which is 2 bytes
    }

    #[test]
    fn test_skill_load_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-skill")
                (func (export "test") (result i32)
                    i32.const 0    ;; name_ptr
                    i32.const 17   ;; name_len ("nonexistent-skill")
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $skill_load
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, -1); // Not found
    }

    #[test]
    fn test_tool_read_success() {
        use std::io::Write;

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Create a temporary file to read
        let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"Hello, World!")
            .expect("Failed to write temp file");
        let temp_path = temp_file.path().to_string_lossy().to_string();
        let path_len = temp_path.len();

        // Build expected JSON to know the expected byte count
        let expected_result = nevoflux_protocol::ReadResult {
            total_lines: 1,
            total_bytes: 13,
            returned_lines: 1,
            offset: 0,
            content: "Hello, World!".into(),
            truncated: false,
        };
        let expected_json = serde_json::to_string(&expected_result).unwrap();
        let expected_len = expected_json.len();

        // Create WAT with the path embedded
        let wat = format!(
            r#"
            (module
                (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "test") (result i32)
                    i32.const 0    ;; path_ptr
                    i32.const {}   ;; path_len
                    i64.const 0    ;; offset
                    i64.const 0    ;; limit (0 = no limit)
                    i32.const 200  ;; result_ptr
                    i32.const 1024 ;; result_len (larger for JSON)
                    call $tool_read
                )
            )
        "#,
            temp_path, path_len
        );

        let module = Module::new(&engine, &wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result as usize, expected_len); // JSON-serialized ReadResult length

        // Verify the JSON content written to memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut buf = vec![0u8; result as usize];
        memory
            .read(&store, 200, &mut buf)
            .expect("Failed to read memory");
        let json_str = String::from_utf8(buf).expect("Invalid UTF-8");
        let read_result: nevoflux_protocol::ReadResult =
            serde_json::from_str(&json_str).expect("Invalid JSON");
        assert_eq!(read_result.content, "Hello, World!");
        assert_eq!(read_result.total_lines, 1);
        assert_eq!(read_result.total_bytes, 13);
        assert!(!read_result.truncated);
    }

    #[test]
    fn test_tool_read_with_offset_and_limit() {
        use std::io::Write;

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Create a temporary file with multiple lines
        let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"line one\nline two\nline three\nline four\nline five")
            .expect("Failed to write temp file");
        let temp_path = temp_file.path().to_string_lossy().to_string();
        let path_len = temp_path.len();

        // offset=1 means skip first line, limit=2 means read 2 lines
        let wat = format!(
            r#"
            (module
                (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "test") (result i32)
                    i32.const 0    ;; path_ptr
                    i32.const {}   ;; path_len
                    i64.const 1    ;; offset (skip 1 line)
                    i64.const 2    ;; limit (read 2 lines)
                    i32.const 200  ;; result_ptr
                    i32.const 1024 ;; result_len (larger for JSON)
                    call $tool_read
                )
            )
        "#,
            temp_path, path_len
        );

        let module = Module::new(&engine, &wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 0);

        // Verify the JSON content
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut buf = vec![0u8; result as usize];
        memory
            .read(&store, 200, &mut buf)
            .expect("Failed to read memory");
        let json_str = String::from_utf8(buf).expect("Invalid UTF-8");
        let read_result: nevoflux_protocol::ReadResult =
            serde_json::from_str(&json_str).expect("Invalid JSON");
        assert_eq!(read_result.content, "line two\nline three");
        assert_eq!(read_result.offset, 1);
        assert_eq!(read_result.returned_lines, 2);
        assert_eq!(read_result.total_lines, 5);
        assert!(read_result.truncated); // 2 lines remaining
    }

    #[test]
    fn test_tool_read_file_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/nonexistent/path/to/file.txt")
                (func (export "test") (result i32)
                    i32.const 0    ;; path_ptr
                    i32.const 29   ;; path_len
                    i64.const 0    ;; offset
                    i64.const 0    ;; limit
                    i32.const 200  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $tool_read
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, -1); // File not found
    }

    #[test]
    fn test_tool_glob_empty_pattern() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Use a pattern that won't match anything
        let wat = r#"
            (module
                (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "/nonexistent/*.xyz")
                (func (export "test") (result i32)
                    i32.const 0    ;; pattern_ptr
                    i32.const 18   ;; pattern_len
                    i32.const 0    ;; base_ptr (unused when base_len=0)
                    i32.const 0    ;; base_len
                    i32.const 200  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $tool_glob
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 2); // Returns "[]" which is 2 bytes (no matches)
    }

    #[test]
    fn test_tool_glob_with_matches() {
        use std::io::Write;

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Create a temporary directory with some files
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file1 = temp_dir.path().join("test1.txt");
        let file2 = temp_dir.path().join("test2.txt");

        std::fs::File::create(&file1)
            .expect("Failed to create file1")
            .write_all(b"content1")
            .expect("Failed to write file1");
        std::fs::File::create(&file2)
            .expect("Failed to create file2")
            .write_all(b"content2")
            .expect("Failed to write file2");

        let pattern = format!("{}/*.txt", temp_dir.path().display());
        let pattern_len = pattern.len();

        let wat = format!(
            r#"
            (module
                (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "test") (result i32)
                    i32.const 0    ;; pattern_ptr
                    i32.const {}   ;; pattern_len
                    i32.const 0    ;; base_ptr
                    i32.const 0    ;; base_len
                    i32.const 200  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $tool_glob
                )
            )
        "#,
            pattern, pattern_len
        );

        let module = Module::new(&engine, &wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // Should return more than 2 bytes (empty array) since we have matches
        assert!(
            result > 2,
            "Expected glob to find matches, got {} bytes",
            result
        );
    }

    #[test]
    fn test_tool_glob_with_base_path() {
        use std::io::Write;

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Create a temporary directory with some files
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file1 = temp_dir.path().join("file.rs");
        std::fs::File::create(&file1)
            .expect("Failed to create file")
            .write_all(b"fn main() {}")
            .expect("Failed to write file");

        let base_path = temp_dir.path().display().to_string();
        let base_len = base_path.len();

        // WAT module with pattern "*.rs" and base path
        let wat = format!(
            r#"
            (module
                (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "*.rs")
                (data (i32.const 100) "{}")
                (func (export "test") (result i32)
                    i32.const 0    ;; pattern_ptr ("*.rs")
                    i32.const 4    ;; pattern_len
                    i32.const 100  ;; base_ptr
                    i32.const {}   ;; base_len
                    i32.const 300  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $tool_glob
                )
            )
        "#,
            base_path, base_len
        );

        let module = Module::new(&engine, &wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // Should return more than 2 bytes (empty array) since we have a match
        assert!(
            result > 2,
            "Expected glob to find file.rs, got {} bytes",
            result
        );
    }

    #[test]
    fn test_memory_create_with_database() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "memory_create" (func $memory_create (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test content for database")
                (func (export "test") (result i32)
                    i32.const 0    ;; content_ptr
                    i32.const 25   ;; content_len
                    i32.const 0    ;; metadata_ptr (unused)
                    i32.const 0    ;; metadata_len (unused)
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $memory_create
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services = crate::wasm::services::HostServices::new(db.clone());
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 40); // ID format is "mem-{uuid}" which is 40 chars

        // Read the ID from guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut id_buf = [0u8; 40];
        memory
            .read(&store, 100, &mut id_buf)
            .expect("Failed to read ID");
        let id = String::from_utf8(id_buf.to_vec()).expect("Invalid UTF-8");

        // Verify the chunk was stored in the database
        let repo = MemoryRepository::new(&db);
        let chunk = repo.get(&id).expect("Failed to get chunk from database");
        assert!(chunk.is_some());
        let chunk = chunk.unwrap();
        assert_eq!(chunk.content, "test content for database");
    }

    #[test]
    fn test_memory_delete_with_database() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // First create a memory chunk in the database
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let chunk = MemoryChunk::new("content to delete").with_id("test-delete-id");
        let repo = MemoryRepository::new(&db);
        repo.create(&chunk).expect("Failed to create chunk");

        // Verify chunk exists
        assert!(repo.get("test-delete-id").unwrap().is_some());

        let wat = r#"
            (module
                (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test-delete-id")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 14  ;; id_len
                    call $memory_delete
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db.clone());
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Success

        // Verify chunk was deleted from the database
        assert!(repo.get("test-delete-id").unwrap().is_none());
    }

    #[test]
    fn test_memory_search_with_database() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        // Create some memory chunks in the database
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let repo = MemoryRepository::new(&db);

        let chunk1 = MemoryChunk::new("The quick brown fox jumps").with_id("search-1");
        let chunk2 = MemoryChunk::new("A lazy dog sleeps").with_id("search-2");
        let chunk3 = MemoryChunk::new("The brown bear runs").with_id("search-3");

        repo.create(&chunk1).expect("Failed to create chunk1");
        repo.create(&chunk2).expect("Failed to create chunk2");
        repo.create(&chunk3).expect("Failed to create chunk3");

        let wat = r#"
            (module
                (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "brown")
                (func (export "test") (result i32)
                    i32.const 0    ;; query_ptr
                    i32.const 5    ;; query_len ("brown")
                    i32.const 10   ;; limit
                    i32.const 100  ;; results_ptr
                    i32.const 1024 ;; results_len
                    call $memory_search
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db.clone());
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 2, "Expected search results, got {} bytes", result);

        // Read and parse the results from guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read results");
        let result_json = String::from_utf8(result_buf).expect("Invalid UTF-8");

        let results: Vec<serde_json::Value> =
            serde_json::from_str(&result_json).expect("Failed to parse JSON");

        // Should find 2 chunks with "brown"
        assert_eq!(results.len(), 2);

        // Verify the result structure has id, content, and metadata
        for result in &results {
            assert!(result.get("id").is_some());
            assert!(result.get("content").is_some());
            assert!(result.get("metadata").is_some());
        }
    }

    #[test]
    fn test_memory_delete_nonexistent() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));

        let wat = r#"
            (module
                (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-id")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 14  ;; id_len
                    call $memory_delete
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 0); // Not found
    }

    #[test]
    fn test_memory_update_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));

        let wat = r#"
            (module
                (import "nevoflux" "memory_update" (func $memory_update (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-id")
                (data (i32.const 20) "new content")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 14  ;; id_len
                    i32.const 20  ;; content_ptr
                    i32.const 11  ;; content_len
                    call $memory_update
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 0); // Not found
    }

    #[test]
    fn test_memory_update_success() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));

        // Create a memory chunk first
        let chunk = MemoryChunk::new("original content").with_id("test-update-id");
        MemoryRepository::new(&db)
            .create(&chunk)
            .expect("Failed to create chunk");

        let wat = r#"
            (module
                (import "nevoflux" "memory_update" (func $memory_update (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test-update-id")
                (data (i32.const 20) "updated content")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 14  ;; id_len
                    i32.const 20  ;; content_ptr
                    i32.const 15  ;; content_len
                    call $memory_update
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db.clone());
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // Success

        // Verify the content was updated
        let updated = MemoryRepository::new(&db)
            .get("test-update-id")
            .expect("Failed to get chunk")
            .expect("Chunk should exist");
        assert_eq!(updated.content, "updated content");
    }

    #[test]
    fn test_memory_get_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));

        let wat = r#"
            (module
                (import "nevoflux" "memory_get" (func $memory_get (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-id")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 14  ;; id_len
                    i32.const 100 ;; result_ptr
                    i32.const 256 ;; result_len
                    call $memory_get
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 0); // Not found
    }

    #[test]
    fn test_memory_get_success() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));

        // Create a memory chunk first
        let chunk = MemoryChunk::new("test content for get").with_id("test-get-id");
        MemoryRepository::new(&db)
            .create(&chunk)
            .expect("Failed to create chunk");

        let wat = r#"
            (module
                (import "nevoflux" "memory_get" (func $memory_get (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test-get-id")
                (func (export "test") (result i32)
                    i32.const 0   ;; id_ptr
                    i32.const 11  ;; id_len
                    i32.const 100 ;; result_ptr
                    i32.const 512 ;; result_len
                    call $memory_get
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 0, "Expected JSON bytes, got {}", result);

        // Read and verify the JSON
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read result");
        let json_str = String::from_utf8(result_buf).expect("Invalid UTF-8");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON");

        assert_eq!(parsed["id"].as_str().unwrap(), "test-get-id");
        assert_eq!(parsed["content"].as_str().unwrap(), "test content for get");
    }

    #[tokio::test]
    async fn test_skill_list_with_registry() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "test") (result i32)
                    i32.const 100  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $skill_list
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");

        // Create a registry with some skills
        let mut registry = SkillRegistry::new();
        let skill1 = Skill::new(
            SkillMetadata::new("code-review")
                .with_description("Review code for best practices")
                .with_tag("code")
                .with_tag("review"),
            "# Code Review\n\nReview guidelines...",
        );
        let skill2 = Skill::new(
            SkillMetadata::new("testing")
                .with_description("Write and run tests")
                .with_tag("testing"),
            "# Testing\n\nTesting best practices...",
        );
        registry
            .register(skill1)
            .expect("Failed to register skill1");
        registry
            .register(skill2)
            .expect("Failed to register skill2");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services =
            crate::wasm::services::HostServices::with_skills(db, Arc::new(RwLock::new(registry)));
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 2, "Expected skill list JSON, got {} bytes", result);

        // Read and parse the results from guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read results");
        let result_json = String::from_utf8(result_buf).expect("Invalid UTF-8");

        let summaries: Vec<serde_json::Value> =
            serde_json::from_str(&result_json).expect("Failed to parse JSON");

        // Should have 2 skills
        assert_eq!(summaries.len(), 2);

        // Verify structure has name, description, tags
        for summary in &summaries {
            assert!(summary.get("name").is_some());
            assert!(summary.get("description").is_some());
            assert!(summary.get("tags").is_some());
        }

        // Verify specific skills are present
        let names: Vec<&str> = summaries
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"testing"));
    }

    #[tokio::test]
    async fn test_skill_load_with_registry() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "code-review")
                (func (export "test") (result i32)
                    i32.const 0    ;; name_ptr
                    i32.const 11   ;; name_len ("code-review")
                    i32.const 100  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $skill_load
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");

        // Create a registry with a skill
        let mut registry = SkillRegistry::new();
        let skill = Skill::new(
            SkillMetadata::new("code-review")
                .with_description("Review code for best practices")
                .with_tag("code")
                .with_tag("review"),
            "# Code Review\n\nWhen reviewing code, check for:\n1. Logic errors\n2. Style issues",
        );
        registry.register(skill).expect("Failed to register skill");

        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services =
            crate::wasm::services::HostServices::with_skills(db, Arc::new(RwLock::new(registry)));
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 0, "Expected skill JSON, got {} bytes", result);

        // Read and parse the result from guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read result");
        let result_json = String::from_utf8(result_buf).expect("Invalid UTF-8");

        let skill: serde_json::Value =
            serde_json::from_str(&result_json).expect("Failed to parse JSON");

        // Verify structure has name, description, content, tags
        assert_eq!(skill["name"].as_str().unwrap(), "code-review");
        assert_eq!(
            skill["description"].as_str().unwrap(),
            "Review code for best practices"
        );
        assert!(skill["content"]
            .as_str()
            .unwrap()
            .contains("When reviewing code"));
        let tags: Vec<&str> = skill["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert!(tags.contains(&"code"));
        assert!(tags.contains(&"review"));
    }

    #[tokio::test]
    async fn test_skill_load_not_found_with_registry() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-skill")
                (func (export "test") (result i32)
                    i32.const 0    ;; name_ptr
                    i32.const 17   ;; name_len ("nonexistent-skill")
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $skill_load
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");

        // Create an empty registry
        let registry = SkillRegistry::new();
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services =
            crate::wasm::services::HostServices::with_skills(db, Arc::new(RwLock::new(registry)));
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, -1); // Not found
    }

    #[test]
    fn test_skill_read_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_read" (func $skill_read (param i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-skill")
                (data (i32.const 20) "file.txt")
                (func (export "test") (result i32)
                    i32.const 0    ;; name_ptr
                    i32.const 17   ;; name_len ("nonexistent-skill")
                    i32.const 20   ;; path_ptr
                    i32.const 8    ;; path_len ("file.txt")
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $skill_read
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let registry = SkillRegistry::new();
        let services =
            crate::wasm::services::HostServices::with_skills(db, Arc::new(RwLock::new(registry)));
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, -1); // Skill not found
    }

    #[test]
    fn test_skill_execute_not_found() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "skill_execute" (func $skill_execute (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "nonexistent-skill")
                (data (i32.const 20) "script.sh")
                (data (i32.const 30) "{}")
                (func (export "test") (result i32)
                    i32.const 0    ;; name_ptr
                    i32.const 17   ;; name_len ("nonexistent-skill")
                    i32.const 20   ;; script_ptr
                    i32.const 9    ;; script_len ("script.sh")
                    i32.const 30   ;; args_ptr
                    i32.const 2    ;; args_len ("{}")
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $skill_execute
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let registry = SkillRegistry::new();
        let services =
            crate::wasm::services::HostServices::with_skills(db, Arc::new(RwLock::new(registry)));
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, -1); // Skill not found
    }

    #[test]
    fn test_subagent_registry_spawn() {
        let registry = SubagentRegistry::new();

        let id1 = registry.spawn("task1".to_string(), "chat".to_string(), None);
        let id2 = registry.spawn("task2".to_string(), "agent".to_string(), None);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        let info1 = registry.get(id1).expect("Subagent 1 not found");
        assert_eq!(info1.task, "task1");
        assert_eq!(info1.mode, "chat");
        assert_eq!(info1.status, SubagentStatus::Running);

        let info2 = registry.get(id2).expect("Subagent 2 not found");
        assert_eq!(info2.task, "task2");
        assert_eq!(info2.mode, "agent");
    }

    #[test]
    fn test_subagent_registry_complete() {
        let registry = SubagentRegistry::new();
        let id = registry.spawn("task".to_string(), "chat".to_string(), None);

        registry.complete(id, "result".to_string());

        let info = registry.get(id).expect("Subagent not found");
        assert_eq!(info.status, SubagentStatus::Completed);
        assert_eq!(info.result, Some("result".to_string()));
    }

    #[test]
    fn test_subagent_registry_fail() {
        let registry = SubagentRegistry::new();
        let id = registry.spawn("task".to_string(), "chat".to_string(), None);

        registry.fail(id, "error message".to_string());

        let info = registry.get(id).expect("Subagent not found");
        assert_eq!(
            info.status,
            SubagentStatus::Failed("error message".to_string())
        );
    }

    #[test]
    fn test_subagent_registry_kill() {
        let registry = SubagentRegistry::new();
        let id = registry.spawn("task".to_string(), "chat".to_string(), None);

        // Kill running subagent
        assert!(registry.kill(id));

        let info = registry.get(id).expect("Subagent not found");
        assert_eq!(info.status, SubagentStatus::Killed);

        // Cannot kill already killed subagent
        assert!(!registry.kill(id));
    }

    #[test]
    fn test_subagent_registry_list_ids() {
        let registry = SubagentRegistry::new();
        registry.spawn("task1".to_string(), "chat".to_string(), None);
        registry.spawn("task2".to_string(), "agent".to_string(), None);
        registry.spawn("task3".to_string(), "browser".to_string(), None);

        let ids = registry.list_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
    }

    #[test]
    fn test_subagent_spawn_wasm() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "subagent_spawn" (func $subagent_spawn (param i32 i32 i32 i32 i64) (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "test task")
                (data (i32.const 20) "chat")
                (func (export "test") (result i64)
                    i32.const 0    ;; task_ptr
                    i32.const 9    ;; task_len ("test task")
                    i32.const 20   ;; mode_ptr
                    i32.const 4    ;; mode_len ("chat")
                    i64.const -1   ;; tab_id (None)
                    call $subagent_spawn
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i64>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert_eq!(result, 1); // First subagent gets ID 1
    }

    #[test]
    fn test_subagent_status_wasm() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "subagent_spawn" (func $subagent_spawn (param i32 i32 i32 i32 i64) (result i64)))
                (import "nevoflux" "subagent_status" (func $subagent_status (param i64) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "task")
                (data (i32.const 10) "chat")
                (func (export "spawn") (result i64)
                    i32.const 0  i32.const 4  i32.const 10  i32.const 4  i64.const -1  call $subagent_spawn
                )
                (func (export "status") (param i64) (result i32)
                    local.get 0  call $subagent_status
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        // Spawn a subagent
        let spawn_func = instance
            .get_typed_func::<(), i64>(&mut store, "spawn")
            .expect("Failed to get spawn function");
        let id = spawn_func.call(&mut store, ()).expect("Failed to spawn");
        assert_eq!(id, 1);

        // Check status (should be running = 0)
        let status_func = instance
            .get_typed_func::<i64, i32>(&mut store, "status")
            .expect("Failed to get status function");
        let status = status_func
            .call(&mut store, id)
            .expect("Failed to get status");
        assert_eq!(status, 0); // 0 = Running

        // Check status for non-existent subagent
        let status = status_func
            .call(&mut store, 999)
            .expect("Failed to get status");
        assert_eq!(status, -1); // -1 = Not found
    }

    #[test]
    fn test_subagent_kill_wasm() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "subagent_spawn" (func $subagent_spawn (param i32 i32 i32 i32 i64) (result i64)))
                (import "nevoflux" "subagent_status" (func $subagent_status (param i64) (result i32)))
                (import "nevoflux" "subagent_kill" (func $subagent_kill (param i64) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "task")
                (data (i32.const 10) "chat")
                (func (export "spawn") (result i64)
                    i32.const 0  i32.const 4  i32.const 10  i32.const 4  i64.const -1  call $subagent_spawn
                )
                (func (export "status") (param i64) (result i32)
                    local.get 0  call $subagent_status
                )
                (func (export "kill") (param i64) (result i32)
                    local.get 0  call $subagent_kill
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        // Spawn a subagent
        let spawn_func = instance
            .get_typed_func::<(), i64>(&mut store, "spawn")
            .expect("Failed to get spawn function");
        let id = spawn_func.call(&mut store, ()).expect("Failed to spawn");

        // Kill the subagent
        let kill_func = instance
            .get_typed_func::<i64, i32>(&mut store, "kill")
            .expect("Failed to get kill function");
        let result = kill_func.call(&mut store, id).expect("Failed to kill");
        assert_eq!(result, 1); // 1 = Successfully killed

        // Check status (should be killed = 3)
        let status_func = instance
            .get_typed_func::<i64, i32>(&mut store, "status")
            .expect("Failed to get status function");
        let status = status_func
            .call(&mut store, id)
            .expect("Failed to get status");
        assert_eq!(status, 3); // 3 = Killed

        // Try to kill again (should fail)
        let result = kill_func.call(&mut store, id).expect("Failed to kill");
        assert_eq!(result, 0); // 0 = Already finished
    }

    #[test]
    fn test_subagent_list_wasm() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "subagent_spawn" (func $subagent_spawn (param i32 i32 i32 i32 i64) (result i64)))
                (import "nevoflux" "subagent_list" (func $subagent_list (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "task1")
                (data (i32.const 10) "chat")
                (data (i32.const 20) "task2")
                (func (export "spawn1") (result i64)
                    i32.const 0  i32.const 5  i32.const 10  i32.const 4  i64.const -1  call $subagent_spawn
                )
                (func (export "spawn2") (result i64)
                    i32.const 20  i32.const 5  i32.const 10  i32.const 4  i64.const -1  call $subagent_spawn
                )
                (func (export "list") (result i32)
                    i32.const 100  ;; result_ptr
                    i32.const 256  ;; result_len
                    call $subagent_list
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        // Spawn two subagents
        let spawn1_func = instance
            .get_typed_func::<(), i64>(&mut store, "spawn1")
            .expect("Failed to get spawn1 function");
        spawn1_func.call(&mut store, ()).expect("Failed to spawn1");

        let spawn2_func = instance
            .get_typed_func::<(), i64>(&mut store, "spawn2")
            .expect("Failed to get spawn2 function");
        spawn2_func.call(&mut store, ()).expect("Failed to spawn2");

        // List subagents
        let list_func = instance
            .get_typed_func::<(), i32>(&mut store, "list")
            .expect("Failed to get list function");
        let result = list_func.call(&mut store, ()).expect("Failed to list");

        // Should return JSON array "[1,2]" which is 5 bytes
        assert!(result > 0, "Expected positive bytes, got {}", result);

        // Read and verify the JSON
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read result");
        let json_str = String::from_utf8(result_buf).expect("Invalid UTF-8");
        let ids: Vec<u64> = serde_json::from_str(&json_str).expect("Invalid JSON");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn test_tool_search_no_services() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "tool_search" (func $tool_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "file")
                (func (export "test") (result i32)
                    i32.const 0    ;; query_ptr
                    i32.const 4    ;; query_len ("file")
                    i32.const 10   ;; max_results
                    i32.const 100  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $tool_search
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");
        let mut store = Store::new(&engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // No services configured, returns -1
        assert_eq!(result, -1);
    }

    #[test]
    fn test_tool_search_no_index() {
        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "tool_search" (func $tool_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "file")
                (func (export "test") (result i32)
                    i32.const 0    ;; query_ptr
                    i32.const 4    ;; query_len ("file")
                    i32.const 10   ;; max_results
                    i32.const 100  ;; result_ptr
                    i32.const 1024 ;; result_len
                    call $tool_search
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");

        // Create services without tool search index
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services = crate::wasm::services::HostServices::new(db);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);

        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        // No tool search index, returns empty array "[]" = 2 bytes
        assert_eq!(result, 2);
    }

    #[test]
    fn test_tool_search_with_results() {
        use nevoflux_mcp::{ToolDefinition, ToolSearchIndex};

        let engine = Engine::default();
        let linker = create_linker(&engine).expect("Failed to create linker");

        let wat = r#"
            (module
                (import "nevoflux" "tool_search" (func $tool_search (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "file")
                (func (export "test") (result i32)
                    i32.const 0    ;; query_ptr
                    i32.const 4    ;; query_len ("file")
                    i32.const 10   ;; max_results
                    i32.const 100  ;; result_ptr
                    i32.const 4096 ;; result_len
                    call $tool_search
                )
            )
        "#;

        let module = Module::new(&engine, wat).expect("Failed to compile test module");

        // Create tool search index with some tools
        let mut index = ToolSearchIndex::new();
        index.add(&ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        });
        index.add(&ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}}),
        });
        index.add(&ToolDefinition {
            name: "browser_navigate".to_string(),
            description: "Navigate to a URL".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"url": {"type": "string"}}}),
        });

        // Create services with tool search index
        let db = Arc::new(Database::open_in_memory().expect("Failed to open in-memory database"));
        let services = crate::wasm::services::HostServices::new(db).with_tool_search(index);
        let state = HostState::new().with_services(services);
        let mut store = Store::new(&engine, state);

        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate");

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .expect("Failed to get test function");

        let result = test_func.call(&mut store, ()).expect("Failed to call test");
        assert!(result > 2, "Expected results, got {} bytes", result);

        // Read and verify the JSON
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Failed to get memory");
        let mut result_buf = vec![0u8; result as usize];
        memory
            .read(&store, 100, &mut result_buf)
            .expect("Failed to read result");
        let json_str = String::from_utf8(result_buf).expect("Invalid UTF-8");
        let results: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).expect("Invalid JSON");

        // Should find 2 file-related tools (read_file, write_file)
        assert_eq!(results.len(), 2);

        // Verify result structure
        let names: Vec<&str> = results.iter().filter_map(|r| r["name"].as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(!names.contains(&"browser_navigate")); // Should not match "file"
    }
}
