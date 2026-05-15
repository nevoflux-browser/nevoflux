//! Gemini CLI ACP configuration.
//!
//! Uses the standard `gemini` CLI with the `--acp` flag:
//! `npm install -g @google/gemini-cli`

use std::path::PathBuf;

use super::AcpProviderConfig;

/// Build an [`AcpProviderConfig`] for the Gemini CLI ACP agent.
pub fn build_config(model: &str, work_dir: PathBuf) -> AcpProviderConfig {
    let command = crate::util::resolve_program("gemini");
    let mut args = vec!["--acp".to_string()];
    if !model.is_empty() && model != "default" {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    AcpProviderConfig {
        command,
        args,
        env: vec![],
        env_remove: vec![],
        work_dir,
        session_mode: "default".to_string(),
        use_mcp_bridge: true,
        inject_mcp_url: true,
    }
}
