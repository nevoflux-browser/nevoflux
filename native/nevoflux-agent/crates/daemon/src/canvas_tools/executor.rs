//! Whitelisted tool executor.
//!
//! Provides functions to execute [`CanvasTool`] definitions safely:
//!
//! - **Template mode**: render `{{param}}` placeholders into concrete arguments.
//! - **Free mode**: pass through LLM-supplied arguments with subcommand checks.
//! - **Internal tools**: return a stub result (the daemon handles them directly).
//!
//! All external commands are spawned via [`tokio::process::Command`] (no shell),
//! with timeout enforcement and output truncation.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::debug;

use crate::canvas_tools::param_validator::{expand_session_dir, validate_params};
use crate::canvas_tools::types::{ArgsMode, BackendKind, CanvasTool};
use crate::error::{DaemonError, Result};

// ---------------------------------------------------------------------------
// Streaming events
// ---------------------------------------------------------------------------

/// An event emitted while a tool is running.
///
/// The streaming executor sends these as stdout / stderr data arrives so the
/// caller can forward them to the client without buffering the whole run.
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    /// A chunk of stdout (UTF-8 lossy converted from raw bytes).
    Stdout(String),
    /// A chunk of stderr.
    Stderr(String),
}

// ---------------------------------------------------------------------------
// ToolExecResult
// ---------------------------------------------------------------------------

/// The result of executing a whitelisted tool.
#[derive(Debug, Clone)]
pub struct ToolExecResult {
    /// Captured standard output (may be truncated).
    pub stdout: String,
    /// Captured standard error (may be truncated).
    pub stderr: String,
    /// Process exit code (`None` if the process was killed by a signal).
    pub exit_code: Option<i32>,
    /// Whether the tool execution succeeded (exit code 0).
    pub success: bool,
    /// Human-readable error message, if any.
    pub error: Option<String>,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Template rendering
// ---------------------------------------------------------------------------

/// Substitute `{{param}}` placeholders in a template argument list.
///
/// Each element of `template` is scanned for `{{key}}` patterns. If `key`
/// exists in `values`, it is replaced with the corresponding value. If a
/// placeholder references a key not present in `values`, an error is returned.
///
/// Literal text (without placeholders) is preserved as-is.
pub fn render_template_args(
    template: &[String],
    values: &HashMap<String, String>,
) -> Result<Vec<String>> {
    let mut result = Vec::with_capacity(template.len());

    for fragment in template {
        let mut rendered = fragment.clone();
        // Find all {{key}} placeholders.
        let mut start = 0;
        while let Some(open) = rendered[start..].find("{{") {
            let abs_open = start + open;
            if let Some(close) = rendered[abs_open..].find("}}") {
                let abs_close = abs_open + close;
                let key = &rendered[abs_open + 2..abs_close];
                if let Some(val) = values.get(key) {
                    rendered = format!(
                        "{}{}{}",
                        &rendered[..abs_open],
                        val,
                        &rendered[abs_close + 2..]
                    );
                    // Continue scanning from after the replacement.
                    start = abs_open + val.len();
                } else {
                    return Err(DaemonError::InvalidRequest(format!(
                        "template placeholder '{{{{{}}}}}' has no value",
                        key
                    )));
                }
            } else {
                // No closing braces — treat as literal, move past.
                start = abs_open + 2;
            }
        }
        result.push(rendered);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Free-mode subcommand check
// ---------------------------------------------------------------------------

/// Verify that the first argument (subcommand) is in the allowed list.
///
/// If `allowed` is empty, all subcommands are permitted. If `args` is empty
/// and `allowed` is non-empty, an error is returned.
pub fn check_free_mode_subcommand(
    args: &[String],
    allowed: &[String],
    tool_name: &str,
) -> Result<()> {
    if allowed.is_empty() {
        return Ok(());
    }

    let subcmd = args.first().ok_or_else(|| {
        DaemonError::InvalidRequest(format!(
            "tool '{tool_name}': free-mode requires at least one argument (subcommand)"
        ))
    })?;

    if !allowed.iter().any(|a| a == subcmd) {
        return Err(DaemonError::PermissionDenied(format!(
            "tool '{tool_name}': subcommand '{}' is not allowed (permitted: {})",
            subcmd,
            allowed.join(", ")
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Command execution
// ---------------------------------------------------------------------------

/// Execute an external command with timeout and output truncation.
///
/// The binary is resolved via `which` before spawning. Stdin is set to null,
/// stdout and stderr are piped and captured. If the process exceeds the
/// tool's configured timeout, it is killed and a [`DaemonError::Timeout`] is
/// returned.
pub async fn execute_command_tool(
    tool: &CanvasTool,
    args: &[String],
    session_dir: &Path,
) -> Result<ToolExecResult> {
    let binary = tool.binary.as_deref().ok_or_else(|| {
        DaemonError::InvalidRequest(format!(
            "tool '{}': command backend requires a 'binary' field",
            tool.name
        ))
    })?;

    // Resolve binary on $PATH.
    let binary_path = which::which(binary).map_err(|e| {
        DaemonError::InvalidRequest(format!(
            "tool '{}': binary '{}' not found: {}",
            tool.name, binary, e
        ))
    })?;

    debug!(
        tool = %tool.name,
        binary = %binary_path.display(),
        args = ?args,
        "Executing command tool"
    );

    let mut cmd = Command::new(&binary_path);
    cmd.args(args);

    // Set working directory if configured.
    if let Some(ref cwd) = tool.constraints.cwd {
        let expanded_cwd = expand_session_dir(cwd, session_dir);
        cmd.current_dir(&expanded_cwd);
    }

    // Pipe stdout/stderr, null stdin.
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let start = Instant::now();
    let timeout_duration = std::time::Duration::from_secs(tool.constraints.timeout_seconds);

    // Spawn and wait with timeout.
    let output = match tokio::time::timeout(timeout_duration, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(io_err)) => {
            let elapsed = start.elapsed();
            return Ok(ToolExecResult {
                stdout: String::new(),
                stderr: io_err.to_string(),
                exit_code: None,
                success: false,
                error: Some(format!("failed to spawn process: {io_err}")),
                duration_ms: elapsed.as_millis() as u64,
            });
        }
        Err(_elapsed) => {
            return Err(DaemonError::Timeout(format!(
                "tool '{}': exceeded {}s timeout",
                tool.name, tool.constraints.timeout_seconds
            )));
        }
    };

    let elapsed = start.elapsed();

    // Truncate stdout.
    let stdout_raw = output.stdout;
    let stdout = truncate_output(&stdout_raw, tool.constraints.max_stdout_bytes);

    // Truncate stderr.
    let stderr_raw = output.stderr;
    let stderr = truncate_output(&stderr_raw, tool.constraints.max_stderr_bytes);

    let exit_code = output.status.code();
    let success = output.status.success();

    let error = if !success {
        Some(format!(
            "process exited with status {}",
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    } else {
        None
    };

    Ok(ToolExecResult {
        stdout,
        stderr,
        exit_code,
        success,
        error,
        duration_ms: elapsed.as_millis() as u64,
    })
}

/// Truncate raw bytes to the specified limit and convert to a UTF-8 string.
///
/// If the output exceeds `max_bytes`, it is truncated and a marker is appended.
fn truncate_output(raw: &[u8], max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        String::from_utf8_lossy(raw).into_owned()
    } else {
        let truncated = String::from_utf8_lossy(&raw[..max_bytes]).into_owned();
        format!(
            "{truncated}\n... [truncated, {}/{} bytes shown]",
            max_bytes,
            raw.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Streaming command execution
// ---------------------------------------------------------------------------

/// Execute an external command and stream stdout/stderr chunks via `event_tx`.
///
/// Behaves like [`execute_command_tool`] (timeout, output truncation, exit code)
/// but emits each chunk of output through the supplied channel as it arrives,
/// instead of buffering until completion. The returned [`ToolExecResult`] still
/// contains the full captured output for callers that want both — useful for
/// the audit log and the final response message.
///
/// Channel send failures are silently ignored: if the consumer dropped the
/// receiver, output is still captured into the result.
pub async fn execute_command_tool_streaming(
    tool: &CanvasTool,
    args: &[String],
    session_dir: &Path,
    event_tx: mpsc::Sender<ExecutionEvent>,
) -> Result<ToolExecResult> {
    let binary = tool.binary.as_deref().ok_or_else(|| {
        DaemonError::InvalidRequest(format!(
            "tool '{}': command backend requires a 'binary' field",
            tool.name
        ))
    })?;

    let binary_path = which::which(binary).map_err(|e| {
        DaemonError::InvalidRequest(format!(
            "tool '{}': binary '{}' not found: {}",
            tool.name, binary, e
        ))
    })?;

    debug!(
        tool = %tool.name,
        binary = %binary_path.display(),
        args = ?args,
        "Executing command tool (streaming)"
    );

    let mut cmd = Command::new(&binary_path);
    cmd.args(args);

    if let Some(ref cwd) = tool.constraints.cwd {
        let expanded_cwd = expand_session_dir(cwd, session_dir);
        cmd.current_dir(&expanded_cwd);
    }

    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let start = Instant::now();
    let timeout_duration = std::time::Duration::from_secs(tool.constraints.timeout_seconds);
    let max_stdout = tool.constraints.max_stdout_bytes;
    let max_stderr = tool.constraints.max_stderr_bytes;

    // Spawn the child.
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(io_err) => {
            return Ok(ToolExecResult {
                stdout: String::new(),
                stderr: io_err.to_string(),
                exit_code: None,
                success: false,
                error: Some(format!("failed to spawn process: {io_err}")),
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    // Spawn readers that emit events and accumulate the full buffer.
    let stdout_tx = event_tx.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = [0u8; 4096];
        let mut accumulated = Vec::new();
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    accumulated.extend_from_slice(&buf[..n]);
                    let _ = stdout_tx.send(ExecutionEvent::Stdout(chunk)).await;
                }
                Err(_) => break,
            }
        }
        accumulated
    });

    let stderr_tx = event_tx.clone();
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = [0u8; 4096];
        let mut accumulated = Vec::new();
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    accumulated.extend_from_slice(&buf[..n]);
                    let _ = stderr_tx.send(ExecutionEvent::Stderr(chunk)).await;
                }
                Err(_) => break,
            }
        }
        accumulated
    });

    // Drop the original sender so readers can finish even if no chunks remain.
    drop(event_tx);

    // Await child + reader tasks under the overall timeout.
    let wait_result = tokio::time::timeout(timeout_duration, async {
        let status = child.wait().await?;
        let stdout_buf = stdout_handle.await.unwrap_or_default();
        let stderr_buf = stderr_handle.await.unwrap_or_default();
        Ok::<_, std::io::Error>((status, stdout_buf, stderr_buf))
    })
    .await;

    let elapsed = start.elapsed();

    let (status, stdout_buf, stderr_buf) = match wait_result {
        Ok(Ok(triple)) => triple,
        Ok(Err(io_err)) => {
            return Ok(ToolExecResult {
                stdout: String::new(),
                stderr: io_err.to_string(),
                exit_code: None,
                success: false,
                error: Some(format!("io error during execution: {io_err}")),
                duration_ms: elapsed.as_millis() as u64,
            });
        }
        Err(_elapsed) => {
            // Timeout — kill the child if still running.
            let _ = child.start_kill();
            return Err(DaemonError::Timeout(format!(
                "tool '{}': exceeded {}s timeout",
                tool.name, tool.constraints.timeout_seconds
            )));
        }
    };

    let stdout = truncate_output(&stdout_buf, max_stdout);
    let stderr = truncate_output(&stderr_buf, max_stderr);

    let exit_code = status.code();
    let success = status.success();
    let error = if !success {
        Some(format!(
            "process exited with status {}",
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    } else {
        None
    };

    Ok(ToolExecResult {
        stdout,
        stderr,
        exit_code,
        success,
        error,
        duration_ms: elapsed.as_millis() as u64,
    })
}

/// Streaming variant of [`execute_whitelisted_tool`].
///
/// Internal tools still return the same instant-stub result (no events emitted).
/// Command tools dispatch to [`execute_command_tool_streaming`].
pub async fn execute_whitelisted_tool_streaming(
    tool: &CanvasTool,
    params: &HashMap<String, String>,
    free_args: &[String],
    session_dir: &Path,
    event_tx: mpsc::Sender<ExecutionEvent>,
) -> Result<ToolExecResult> {
    if tool.kind == BackendKind::Internal {
        // Drop the channel — internal tools have no streaming output.
        drop(event_tx);
        return Ok(ToolExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            success: true,
            error: None,
            duration_ms: 0,
        });
    }

    match tool.args_mode {
        ArgsMode::Template => {
            validate_params(&tool.params, params, session_dir)?;
            let mut effective = params.clone();
            for (name, spec) in &tool.params {
                if !effective.contains_key(name) {
                    if let Some(ref default) = spec.default {
                        effective.insert(name.clone(), default.clone());
                    }
                }
            }
            let args = render_template_args(&tool.args, &effective)?;
            execute_command_tool_streaming(tool, &args, session_dir, event_tx).await
        }
        ArgsMode::Free => {
            check_free_mode_subcommand(free_args, &tool.allowed_subcommands, &tool.name)?;
            execute_command_tool_streaming(tool, free_args, session_dir, event_tx).await
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Validate parameters and execute a whitelisted tool.
///
/// Dispatches to the appropriate execution path based on the tool's backend
/// kind and args mode:
///
/// - **Internal** tools return a stub result immediately; the daemon routes
///   them to built-in handlers.
/// - **Command + Template**: validates params, renders the template, spawns.
/// - **Command + Free**: validates the subcommand allowlist, spawns with the
///   provided free arguments.
pub async fn execute_whitelisted_tool(
    tool: &CanvasTool,
    params: &HashMap<String, String>,
    free_args: &[String],
    session_dir: &Path,
) -> Result<ToolExecResult> {
    // Internal tools are handled by the daemon, not by spawning a process.
    if tool.kind == BackendKind::Internal {
        return Ok(ToolExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            success: true,
            error: None,
            duration_ms: 0,
        });
    }

    match tool.args_mode {
        ArgsMode::Template => {
            // Validate parameters against spec.
            validate_params(&tool.params, params, session_dir)?;

            // Build effective values: defaults filled in for missing optional params.
            let mut effective = params.clone();
            for (name, spec) in &tool.params {
                if !effective.contains_key(name) {
                    if let Some(ref default) = spec.default {
                        effective.insert(name.clone(), default.clone());
                    }
                }
            }

            // Render template arguments.
            let args = render_template_args(&tool.args, &effective)?;

            execute_command_tool(tool, &args, session_dir).await
        }
        ArgsMode::Free => {
            // Check subcommand allowlist.
            check_free_mode_subcommand(free_args, &tool.allowed_subcommands, &tool.name)?;

            execute_command_tool(tool, free_args, session_dir).await
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_tools::types::{ExecutionConstraints, ParamSpec, ParamType, ToolSource};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn session_dir() -> PathBuf {
        PathBuf::from("/tmp/nevoflux-test-session")
    }

    /// Verify that the streaming executor emits stdout chunks via the channel
    /// AND returns the same buffered output in the result.
    #[tokio::test]
    async fn streaming_emits_stdout_chunks() {
        let tool = CanvasTool {
            name: "echo_stream".into(),
            description: "echo for streaming test".into(),
            kind: BackendKind::Command,
            binary: Some("echo".into()),
            api: None,
            args_mode: ArgsMode::Template,
            args: vec!["streaming-test-output".into()],
            allowed_subcommands: vec![],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ExecutionEvent>(16);
        let result = tokio::spawn(async move {
            execute_command_tool_streaming(
                &tool,
                &["streaming-test-output".into()],
                &session_dir(),
                tx,
            )
            .await
        });

        // Collect events until channel closes.
        let mut chunks = Vec::new();
        while let Some(evt) = rx.recv().await {
            chunks.push(evt);
        }
        let res = result.await.unwrap().unwrap();

        assert!(res.success, "echo should succeed");
        assert!(res.stdout.contains("streaming-test-output"));
        // At least one stdout chunk should have been emitted on the channel.
        let stdout_chunks: Vec<_> = chunks
            .iter()
            .filter_map(|e| match e {
                ExecutionEvent::Stdout(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert!(
            !stdout_chunks.is_empty(),
            "expected ≥1 stdout chunk via channel"
        );
        let joined = stdout_chunks.join("");
        assert!(joined.contains("streaming-test-output"));
    }

    /// Verify that the streaming executor's high-level dispatch (template mode)
    /// validates params before spawning anything.
    #[tokio::test]
    async fn streaming_validates_params_in_template_mode() {
        let tool = make_echo_tool();
        let (tx, _rx) = tokio::sync::mpsc::channel::<ExecutionEvent>(8);
        let params = HashMap::new(); // missing required 'message'
        let res = execute_whitelisted_tool_streaming(&tool, &params, &[], &session_dir(), tx).await;
        assert!(res.is_err(), "should reject missing required param");
    }

    /// Helper: build a minimal command tool.
    fn make_echo_tool() -> CanvasTool {
        CanvasTool {
            name: "echo_test".into(),
            description: "Echo test tool".into(),
            kind: BackendKind::Command,
            binary: Some("echo".into()),
            api: None,
            args_mode: ArgsMode::Template,
            args: vec!["{{message}}".into()],
            allowed_subcommands: vec![],
            params: {
                let mut m = HashMap::new();
                m.insert(
                    "message".into(),
                    ParamSpec {
                        param_type: ParamType::Text { pattern: None },
                        optional: false,
                        default: None,
                    },
                );
                m
            },
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        }
    }

    // -----------------------------------------------------------------------
    // render_template_args
    // -----------------------------------------------------------------------

    #[test]
    fn test_render_template_basic() {
        let template = vec![
            "--input".into(),
            "{{file}}".into(),
            "--count".into(),
            "{{n}}".into(),
        ];
        let mut values = HashMap::new();
        values.insert("file".into(), "test.txt".into());
        values.insert("n".into(), "5".into());

        let result = render_template_args(&template, &values).unwrap();
        assert_eq!(result, vec!["--input", "test.txt", "--count", "5"]);
    }

    #[test]
    fn test_render_template_missing_value() {
        let template = vec!["{{missing}}".into()];
        let values = HashMap::new();

        let err = render_template_args(&template, &values).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_render_template_preserves_literals() {
        let template = vec!["--verbose".into(), "literal-text".into()];
        let values = HashMap::new();

        let result = render_template_args(&template, &values).unwrap();
        assert_eq!(result, vec!["--verbose", "literal-text"]);
    }

    // -----------------------------------------------------------------------
    // check_free_mode_subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_subcommand_allowed() {
        let args = vec!["status".into()];
        let allowed = vec!["status".into(), "log".into()];
        check_free_mode_subcommand(&args, &allowed, "git").unwrap();
    }

    #[test]
    fn test_check_subcommand_blocked() {
        let args = vec!["push".into()];
        let allowed = vec!["status".into(), "log".into()];
        let err = check_free_mode_subcommand(&args, &allowed, "git").unwrap_err();
        assert!(err.to_string().contains("not allowed"));
        assert!(err.to_string().contains("push"));
    }

    #[test]
    fn test_check_subcommand_empty_args() {
        let args: Vec<String> = vec![];
        let allowed = vec!["status".into()];
        let err = check_free_mode_subcommand(&args, &allowed, "git").unwrap_err();
        assert!(err.to_string().contains("requires at least one argument"));
    }

    #[test]
    fn test_check_subcommand_empty_allowed_permits_all() {
        let args = vec!["anything".into()];
        let allowed: Vec<String> = vec![];
        check_free_mode_subcommand(&args, &allowed, "tool").unwrap();
    }

    // -----------------------------------------------------------------------
    // execute_command_tool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_echo() {
        let tool = make_echo_tool();
        let args = vec!["hello world".into()];
        let result = execute_command_tool(&tool, &args, &session_dir())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello world"));
        assert!(result.error.is_none());
        assert!(result.duration_ms < 5000); // should be fast
    }

    #[tokio::test]
    async fn test_execute_nonexistent_binary() {
        let mut tool = make_echo_tool();
        tool.binary = Some("nonexistent_binary_xyz_12345".into());

        let err = execute_command_tool(&tool, &[], &session_dir())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_timeout() {
        let mut tool = make_echo_tool();
        tool.binary = Some("sleep".into());
        tool.constraints.timeout_seconds = 1;

        let args = vec!["60".into()];
        let err = execute_command_tool(&tool, &args, &session_dir())
            .await
            .unwrap_err();
        assert!(
            matches!(err, DaemonError::Timeout(_)),
            "expected Timeout, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_execute_failing_exit_code() {
        let mut tool = make_echo_tool();
        tool.binary = Some("false".into());

        let result = execute_command_tool(&tool, &[], &session_dir())
            .await
            .unwrap();

        assert!(!result.success);
        assert_ne!(result.exit_code, Some(0));
        assert!(result.error.is_some());
    }

    // -----------------------------------------------------------------------
    // execute_whitelisted_tool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_whitelisted_template_mode() {
        let tool = make_echo_tool();
        let mut params = HashMap::new();
        params.insert("message".into(), "template works".into());

        let result = execute_whitelisted_tool(&tool, &params, &[], &session_dir())
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("template works"));
    }

    #[tokio::test]
    async fn test_whitelisted_free_mode_allowed() {
        let tool = CanvasTool {
            name: "git_test".into(),
            description: "Git test".into(),
            kind: BackendKind::Command,
            binary: Some("echo".into()),
            api: None,
            args_mode: ArgsMode::Free,
            args: vec![],
            allowed_subcommands: vec!["status".into(), "log".into()],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        };

        let free_args = vec!["status".into(), "--short".into()];
        let result = execute_whitelisted_tool(&tool, &HashMap::new(), &free_args, &session_dir())
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("status"));
    }

    #[tokio::test]
    async fn test_whitelisted_free_mode_blocked() {
        let tool = CanvasTool {
            name: "git_test".into(),
            description: "Git test".into(),
            kind: BackendKind::Command,
            binary: Some("echo".into()),
            api: None,
            args_mode: ArgsMode::Free,
            args: vec![],
            allowed_subcommands: vec!["status".into(), "log".into()],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        };

        let free_args = vec!["push".into(), "--force".into()];
        let err = execute_whitelisted_tool(&tool, &HashMap::new(), &free_args, &session_dir())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_whitelisted_internal_returns_stub() {
        let tool = CanvasTool {
            name: "read_file".into(),
            description: "Internal read file".into(),
            kind: BackendKind::Internal,
            binary: None,
            api: Some("builtin://read_file".into()),
            args_mode: ArgsMode::Template,
            args: vec![],
            allowed_subcommands: vec![],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        };

        let result = execute_whitelisted_tool(&tool, &HashMap::new(), &[], &session_dir())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
        assert_eq!(result.duration_ms, 0);
    }
}
