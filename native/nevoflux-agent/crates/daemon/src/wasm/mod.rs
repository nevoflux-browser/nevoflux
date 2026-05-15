//! Wasm module for WebAssembly runtime support.
//!
//! This module provides the infrastructure for loading and executing
//! WebAssembly modules within the NevoFlux daemon.

pub mod instance;
pub mod linker;
pub mod llm;
pub mod mcp_http_server;
pub mod mcp_tool_executor;
pub mod runtime;
pub mod services;
pub mod subagent;

pub use instance::WasmInstance;
pub use linker::{create_linker, HostState};
pub use llm::{LlmAttachment, LlmChatRequest, LlmChatResponse, LlmMessage, LlmUsage};
pub use runtime::{WasmConfig, WasmRuntime};
pub use services::{BrowserRequest, BrowserResponse, BrowserSender, HostServices, LlmConfig};
pub use subagent::{SubagentExecutor, SubagentHandle, SubagentStatus};
