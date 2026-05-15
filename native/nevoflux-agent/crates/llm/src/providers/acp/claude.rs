//! Claude Code ACP configuration.
//!
//! The ACP binary is `claude-agent-acp`, installed via:
//! `npm install -g @zed-industries/claude-agent-acp`

use std::path::PathBuf;

use super::AcpProviderConfig;

/// Default binary name for the Claude ACP agent.
const CLAUDE_ACP_BINARY: &str = "claude-agent-acp";

/// Build an [`AcpProviderConfig`] for the Claude Code ACP agent.
pub fn build_config(work_dir: PathBuf) -> AcpProviderConfig {
    let command = crate::util::resolve_program(CLAUDE_ACP_BINARY);
    AcpProviderConfig {
        command,
        args: vec![],
        env: vec![],
        env_remove: vec!["CLAUDECODE".to_string()],
        work_dir,
        session_mode: "default".to_string(),
        use_mcp_bridge: true,
        inject_mcp_url: true,
    }
}
