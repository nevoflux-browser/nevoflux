//! Iteration-specific tool guards.
//!
//! Historical note: this module used to maintain a parallel "tool class"
//! taxonomy (Read / Write / DomClick / Nav / etc.) for /loop iterations.
//! That taxonomy used fictitious tool names that didn't match the real
//! `builtin-wasm::Agent::get_chat_tools()` catalog, so iterations ended
//! up with empty allowlists (no `browser_query` etc.).
//!
//! Migration 018 replaced the class system with the existing
//! `AgentMode { Chat, Browser, Agent }` enum from `builtin-wasm`, and
//! the iteration's tool catalog now comes directly from
//! `Agent::get_tools_for_mode(mode)`. The only iteration-specific filter
//! left is [`is_forbidden_in_iteration`].

/// Tools that are forbidden inside loop iterations regardless of mode.
/// `loop.create` would let an iteration spawn nested loops; `ask_user` blocks
/// on a sidebar that may be closed.
pub fn is_forbidden_in_iteration(tool_name: &str) -> bool {
    matches!(tool_name, "loop.create" | "ask_user")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_set() {
        assert!(is_forbidden_in_iteration("loop.create"));
        assert!(is_forbidden_in_iteration("ask_user"));
        assert!(!is_forbidden_in_iteration("read"));
        assert!(!is_forbidden_in_iteration("loop.scratchpad.set"));
        assert!(!is_forbidden_in_iteration("browser_get_content"));
    }
}
