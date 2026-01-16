/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! WASM Sandbox and Security

use anyhow::Result;
use wasmtime::*;

/// Sandbox configuration for WASM plugins
pub struct SandboxConfig {
    /// Maximum memory size in bytes
    pub max_memory: u64,
    /// Maximum execution time in milliseconds
    pub max_time_ms: u64,
    /// Allowed capabilities
    pub capabilities: Vec<Capability>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory: 100 * 1024 * 1024, // 100 MB
            max_time_ms: 5000,             // 5 seconds
            capabilities: vec![],
        }
    }
}

/// Plugin capabilities
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Access to network
    Network,
    /// Access to file system
    FileSystem,
    /// Access to browser DOM
    Dom,
    /// Access to LLM
    Llm,
    /// Access to MCP services
    Mcp,
}

/// Create a sandboxed store for plugin execution
pub fn create_sandbox_store(
    engine: &Engine,
    config: &SandboxConfig,
) -> Result<Store<ResourceLimiterData>> {
    let data = ResourceLimiterData {
        memory_size: config.max_memory,
    };

    let mut store = Store::new(engine, data);

    // Set the resource limiter
    store.limiter(|data| data);

    Ok(store)
}

struct ResourceLimiterData {
    memory_size: u64,
}

impl wasmtime::ResourceLimiter for ResourceLimiterData {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, Error> {
        Ok(desired as u64 <= self.memory_size)
    }

    fn table_growing(
        &mut self,
        _current: u32,
        _desired: u32,
        _maximum: Option<u32>,
    ) -> Result<bool, Error> {
        Ok(true)
    }
}
