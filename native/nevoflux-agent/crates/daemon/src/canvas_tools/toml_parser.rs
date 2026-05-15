//! TOML parser for canvas tool definition files.
//!
//! Reads `*.toml` files from a directory, deserializes each into a
//! [`CanvasTool`], validates the result, and returns the collection of
//! successfully parsed tools (logging warnings for any that fail).

use std::path::Path;

use tracing::warn;

use crate::canvas_tools::types::{ArgsMode, BackendKind, CanvasTool};
use crate::error::{DaemonError, Result};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a TOML string into a [`CanvasTool`], then validate it.
pub fn parse_tool_toml(toml_str: &str) -> Result<CanvasTool> {
    let tool: CanvasTool =
        toml::from_str(toml_str).map_err(|e| DaemonError::ConfigError(e.to_string()))?;
    validate_tool(&tool)?;
    Ok(tool)
}

/// Read every `*.toml` file in `dir`, parse each into a [`CanvasTool`],
/// skip (with a warning) any file that fails to parse or validate, and
/// return the successfully loaded tools.
pub async fn parse_tool_directory(dir: &Path) -> Vec<CanvasTool> {
    let pattern = dir.join("*.toml");
    let pattern_str = pattern.to_string_lossy().to_string();

    let paths = match glob::glob(&pattern_str) {
        Ok(paths) => paths,
        Err(e) => {
            warn!("Invalid glob pattern for tool directory {:?}: {}", dir, e);
            return Vec::new();
        }
    };

    let mut tools = Vec::new();

    for entry in paths {
        let path = match entry {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to read tool directory entry: {}", e);
                continue;
            }
        };

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read tool file {:?}: {}", path, e);
                continue;
            }
        };

        match parse_tool_toml(&content) {
            Ok(tool) => tools.push(tool),
            Err(e) => {
                warn!("Failed to parse tool file {:?}: {}", path, e);
            }
        }
    }

    tools
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a [`CanvasTool`] for internal consistency.
///
/// Rules:
/// - `name` must not be empty.
/// - `Command` tools must have a `binary`.
/// - `Command` tools must specify an `args_mode`.
///   - `Template` mode requires at least one entry in `args`.
///   - `Free` mode requires at least one entry in `allowed_subcommands`.
/// - `Internal` tools must have an `api` endpoint.
pub fn validate_tool(tool: &CanvasTool) -> Result<()> {
    if tool.name.trim().is_empty() {
        return Err(DaemonError::ConfigError(
            "Tool name must not be empty".into(),
        ));
    }

    match tool.kind {
        BackendKind::Command => {
            if tool.binary.is_none() {
                return Err(DaemonError::ConfigError(format!(
                    "Command tool '{}' must specify a binary",
                    tool.name
                )));
            }

            match tool.args_mode {
                ArgsMode::Template => {
                    if tool.args.is_empty() {
                        return Err(DaemonError::ConfigError(format!(
                            "Template-mode tool '{}' must have at least one args entry",
                            tool.name
                        )));
                    }
                }
                ArgsMode::Free => {
                    if tool.allowed_subcommands.is_empty() {
                        return Err(DaemonError::ConfigError(format!(
                            "Free-mode tool '{}' must have at least one allowed subcommand",
                            tool.name
                        )));
                    }
                }
            }
        }
        BackendKind::Internal => {
            if tool.api.is_none() {
                return Err(DaemonError::ConfigError(format!(
                    "Internal tool '{}' must specify an api endpoint",
                    tool.name
                )));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Parse a template-mode command tool (ffmpeg-like)
    #[test]
    fn test_parse_template_command_tool() {
        let toml = r#"
            name = "ffmpeg_convert"
            description = "Convert media files with ffmpeg"
            kind = "command"
            binary = "/usr/bin/ffmpeg"
            args_mode = "template"
            args = ["-i", "{{input}}", "-c:v", "{{codec}}", "{{output}}"]

            [params.input]
            type = "path"
            allowed_prefix = "/home"

            [params.codec]
            type = "enum"
            values = ["libx264", "libx265", "copy"]

            [params.output]
            type = "path"

            [constraints]
            timeout_seconds = 300
        "#;

        let tool = parse_tool_toml(toml).unwrap();
        assert_eq!(tool.name, "ffmpeg_convert");
        assert_eq!(tool.kind, BackendKind::Command);
        assert_eq!(tool.binary.as_deref(), Some("/usr/bin/ffmpeg"));
        assert_eq!(tool.args_mode, ArgsMode::Template);
        assert_eq!(tool.args.len(), 5);
        assert_eq!(tool.params.len(), 3);
        assert_eq!(tool.constraints.timeout_seconds, 300);
        assert!(tool.enabled); // default
    }

    // 2. Parse a free-mode command tool (git-like)
    #[test]
    fn test_parse_free_command_tool() {
        let toml = r#"
            name = "git"
            description = "Git version control"
            kind = "command"
            binary = "git"
            args_mode = "free"
            allowed_subcommands = ["status", "log", "diff", "show"]
        "#;

        let tool = parse_tool_toml(toml).unwrap();
        assert_eq!(tool.name, "git");
        assert_eq!(tool.args_mode, ArgsMode::Free);
        assert_eq!(tool.allowed_subcommands.len(), 4);
        assert!(tool.allowed_subcommands.contains(&"status".to_string()));
    }

    // 3. Parse an internal tool
    #[test]
    fn test_parse_internal_tool() {
        let toml = r#"
            name = "web_search"
            description = "Search the web via internal API"
            kind = "internal"
            api = "builtin://web_search"
        "#;

        let tool = parse_tool_toml(toml).unwrap();
        assert_eq!(tool.name, "web_search");
        assert_eq!(tool.kind, BackendKind::Internal);
        assert_eq!(tool.api.as_deref(), Some("builtin://web_search"));
        assert!(tool.binary.is_none());
    }

    // 4. Reject command tool without binary
    #[test]
    fn test_parse_rejects_command_without_binary() {
        let toml = r#"
            name = "no_binary"
            description = "Missing binary field"
            kind = "command"
            args_mode = "template"
            args = ["{{x}}"]
        "#;

        let err = parse_tool_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("binary"), "expected binary error, got: {msg}");
    }

    // 5. Reject internal tool without api
    #[test]
    fn test_parse_rejects_internal_without_api() {
        let toml = r#"
            name = "no_api"
            description = "Missing api field"
            kind = "internal"
        "#;

        let err = parse_tool_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("api"), "expected api error, got: {msg}");
    }

    // 6. Reject free-mode tool without allowed_subcommands
    #[test]
    fn test_parse_rejects_free_without_subcommands() {
        let toml = r#"
            name = "free_no_subs"
            description = "Free mode but no subcommands"
            kind = "command"
            binary = "/usr/bin/ls"
            args_mode = "free"
        "#;

        let err = parse_tool_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("subcommand"),
            "expected subcommand error, got: {msg}"
        );
    }

    // 7. Reject template-mode tool without args
    #[test]
    fn test_parse_rejects_template_without_args() {
        let toml = r#"
            name = "template_no_args"
            description = "Template mode but no args"
            kind = "command"
            binary = "/usr/bin/echo"
            args_mode = "template"
        "#;

        let err = parse_tool_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("args"), "expected args error, got: {msg}");
    }

    // 8. Reject empty name
    #[test]
    fn test_parse_rejects_empty_name() {
        let toml = r#"
            name = ""
            description = "Empty name"
            kind = "internal"
            api = "builtin://test"
        "#;

        let err = parse_tool_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("name"), "expected name error, got: {msg}");
    }

    // 9. Default enabled is true
    #[test]
    fn test_parse_defaults_enabled_true() {
        let toml = r#"
            name = "enabled_default"
            description = "No explicit enabled field"
            kind = "internal"
            api = "builtin://test"
        "#;

        let tool = parse_tool_toml(toml).unwrap();
        assert!(tool.enabled, "enabled should default to true");
    }

    // 10. Explicitly disabled tool
    #[test]
    fn test_parse_explicit_disabled() {
        let toml = r#"
            name = "disabled_tool"
            description = "Explicitly disabled"
            kind = "internal"
            api = "builtin://test"
            enabled = false
        "#;

        let tool = parse_tool_toml(toml).unwrap();
        assert!(!tool.enabled, "tool should be disabled");
    }

    // 11. parse_tool_directory loads valid files, skips invalid ones
    #[tokio::test]
    async fn test_parse_tool_directory() {
        let dir = tempfile::tempdir().unwrap();

        // Write a valid tool file
        let valid = r#"
            name = "valid_tool"
            description = "A valid tool"
            kind = "internal"
            api = "builtin://valid"
        "#;
        tokio::fs::write(dir.path().join("valid.toml"), valid)
            .await
            .unwrap();

        // Write an invalid tool file (missing required fields)
        tokio::fs::write(dir.path().join("invalid.toml"), "name = 123")
            .await
            .unwrap();

        // Write a non-toml file (should be ignored by glob)
        tokio::fs::write(dir.path().join("readme.txt"), "not a tool")
            .await
            .unwrap();

        let tools = parse_tool_directory(dir.path()).await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "valid_tool");
    }

    // 12. Parse the shipped example TOML files from examples/canvas-tools/
    #[tokio::test]
    async fn test_parse_example_tools_directory() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/canvas-tools");
        if dir.exists() {
            let tools = parse_tool_directory(&dir).await;
            assert!(
                tools.len() >= 4,
                "Expected at least 4 example tools, got {}",
                tools.len()
            );
            let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
            assert!(names.contains(&"ffmpeg.trim"));
            assert!(names.contains(&"ffmpeg.probe"));
            assert!(names.contains(&"git"));
            assert!(names.contains(&"fs.read"));
        }
    }
}
