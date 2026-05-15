//! /loop skill: scheduled and event-triggered re-runs of prompts or wrapped skills.
//!
//! Spec: docs/superpowers/specs/2026-04-22-loop-skill-design.md

pub mod combinator;
pub mod dynamic;
pub mod events;
pub mod executor;
pub mod expression;
pub mod manager;
pub mod registry;
pub mod scheduler;
pub mod sweep;
pub mod tool_classes;
pub mod tools;
pub mod types;
pub use executor::{ExecResult, IterationExecutor};
pub use expression::{ParseError, TabRef, TriggerExpr};
pub use manager::{CreateLoopArgs, LoopManager};
pub use registry::LoopRegistry;
pub use scheduler::{LoopFireRequest, TriggerScheduler};
pub use tools::{execute_loop_tool, ToolCallContext};
pub use types::{LoopId, LoopRuntime};

/// Process-global handle to the daemon's `LoopManager`, set once at daemon
/// startup (see `server.rs` near the LoopManager construction site).
///
/// Used by `IterationExecutor::execute` to back-fill
/// `HostServices.loop_manager` into the per-iteration services clone.
/// Without this, claude-code (ACP) tool calls to `loop.scratchpad.set` etc.
/// fail with "/loop tools are not available" because the LoopManager's
/// pre-construction services snapshot has `loop_manager: None` (chicken-
/// and-egg: services builds before LoopManager exists).
pub static CURRENT_LOOP_MANAGER: std::sync::OnceLock<std::sync::Arc<manager::LoopManager>> =
    std::sync::OnceLock::new();
