//! Core types for the Canvas Tool Whitelist system.
//!
//! Defines [`CanvasTool`] and its supporting types that represent
//! whitelisted command-line tools available to the LLM agent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// BackendKind
// ---------------------------------------------------------------------------

/// How the tool is executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Spawn an external process (the common case).
    Command,
    /// Handled by built-in daemon logic (no subprocess).
    Internal,
}

// ---------------------------------------------------------------------------
// ArgsMode
// ---------------------------------------------------------------------------

/// How the argument string is constructed before execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgsMode {
    /// Arguments are built from a template with `{{param}}` placeholders.
    Template,
    /// The LLM may supply arbitrary arguments (subject to subcommand checks).
    Free,
}

// ---------------------------------------------------------------------------
// ParamType
// ---------------------------------------------------------------------------

/// The value-type of a single tool parameter.
///
/// Serialized as an internally-tagged enum (`"type": "..."`) so that
/// variant-specific fields live at the same level as the tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamType {
    /// A filesystem path (may carry an optional allowed-prefix constraint).
    Path {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allowed_prefix: Option<String>,
    },
    /// A duration string such as `"30s"` or `"5m"`.
    Duration,
    /// A signed integer.
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// A floating-point number.
    Float {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    /// A boolean flag.
    Bool,
    /// One of a fixed set of string values.
    Enum { values: Vec<String> },
    /// Free-form text (may carry a regex constraint).
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    /// An identifier (alphanumeric + underscores, no spaces).
    Identifier,
}

// ---------------------------------------------------------------------------
// ParamSpec
// ---------------------------------------------------------------------------

/// Full specification of a single named parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    /// The type and type-specific constraints.
    #[serde(flatten)]
    pub param_type: ParamType,

    /// Whether the parameter may be omitted.
    #[serde(default)]
    pub optional: bool,

    /// Default value used when the parameter is omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

// ---------------------------------------------------------------------------
// ExecutionConstraints
// ---------------------------------------------------------------------------

/// Resource limits applied when executing a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionConstraints {
    /// Maximum wall-clock seconds before the process is killed.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Working directory override (if `None`, inherit the daemon's cwd).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Maximum bytes captured from stdout (excess is truncated).
    #[serde(default = "default_max_bytes")]
    pub max_stdout_bytes: usize,

    /// Maximum bytes captured from stderr (excess is truncated).
    #[serde(default = "default_max_bytes")]
    pub max_stderr_bytes: usize,
}

fn default_timeout() -> u64 {
    60
}

fn default_max_bytes() -> usize {
    1_048_576 // 1 MiB
}

impl Default for ExecutionConstraints {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout(),
            max_stdout_bytes: default_max_bytes(),
            max_stderr_bytes: default_max_bytes(),
            cwd: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ToolSource
// ---------------------------------------------------------------------------

/// Where a [`CanvasTool`] definition originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// Shipped with the daemon binary.
    #[default]
    Builtin,
    /// Loaded from the user's config directory.
    User,
    /// Registered dynamically during a session.
    Session,
}

// ---------------------------------------------------------------------------
// CanvasTool
// ---------------------------------------------------------------------------

/// A whitelisted tool that the LLM agent may invoke.
///
/// Each `CanvasTool` wraps a single CLI binary (or internal handler) with
/// explicit parameter constraints so the agent cannot escape the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasTool {
    /// Unique machine-readable name (e.g. `"ripgrep"`).
    pub name: String,

    /// Human-readable description shown to the LLM.
    pub description: String,

    /// Execution backend.
    pub kind: BackendKind,

    /// Path or name of the executable (resolved via `$PATH` if relative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,

    /// Optional API endpoint (for `Internal` tools that call a service).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,

    /// How arguments are assembled.
    #[serde(default = "default_args_mode")]
    pub args_mode: ArgsMode,

    /// Template or fixed argument fragments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// If non-empty, only these subcommands are permitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_subcommands: Vec<String>,

    /// Named parameters with type/constraint info.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, ParamSpec>,

    /// Execution constraints (timeouts, output limits).
    #[serde(default)]
    pub constraints: ExecutionConstraints,

    /// Whether the tool is currently active.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Provenance of this definition (not persisted to TOML/JSON).
    #[serde(skip)]
    pub source: ToolSource,
}

fn default_args_mode() -> ArgsMode {
    ArgsMode::Template
}

fn default_enabled() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal `CanvasTool` for tests.
    fn minimal_tool() -> CanvasTool {
        CanvasTool {
            name: "echo".into(),
            description: "Echo text to stdout".into(),
            kind: BackendKind::Command,
            binary: Some("/usr/bin/echo".into()),
            api: None,
            args_mode: ArgsMode::Template,
            args: vec!["{{message}}".into()],
            allowed_subcommands: vec![],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: ToolSource::Builtin,
        }
    }

    // 1. BackendKind serde roundtrip
    #[test]
    fn test_backend_kind_serde_roundtrip() {
        for kind in [BackendKind::Command, BackendKind::Internal] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: BackendKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
        // Verify snake_case serialization
        assert_eq!(
            serde_json::to_string(&BackendKind::Command).unwrap(),
            "\"command\""
        );
        assert_eq!(
            serde_json::to_string(&BackendKind::Internal).unwrap(),
            "\"internal\""
        );
    }

    // 2. ArgsMode serde roundtrip
    #[test]
    fn test_args_mode_serde_roundtrip() {
        for mode in [ArgsMode::Template, ArgsMode::Free] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: ArgsMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
        assert_eq!(
            serde_json::to_string(&ArgsMode::Template).unwrap(),
            "\"template\""
        );
        assert_eq!(serde_json::to_string(&ArgsMode::Free).unwrap(), "\"free\"");
    }

    // 3. ParamType all 8 variants roundtrip
    #[test]
    fn test_param_type_all_variants_roundtrip() {
        let variants: Vec<ParamType> = vec![
            ParamType::Path {
                allowed_prefix: Some("/home".into()),
            },
            ParamType::Duration,
            ParamType::Int {
                min: Some(-10),
                max: Some(100),
            },
            ParamType::Float {
                min: Some(0.0),
                max: Some(1.0),
            },
            ParamType::Bool,
            ParamType::Enum {
                values: vec!["a".into(), "b".into()],
            },
            ParamType::Text {
                pattern: Some(r"^\w+$".into()),
            },
            ParamType::Identifier,
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let back: ParamType = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, back, "roundtrip failed for {json}");
        }
    }

    // 4. ParamSpec with flattened type
    #[test]
    fn test_param_spec_serde_flattened() {
        let spec = ParamSpec {
            param_type: ParamType::Int {
                min: Some(1),
                max: Some(10),
            },
            optional: true,
            default: Some("5".into()),
        };
        let json = serde_json::to_string(&spec).unwrap();

        // The "type" tag should be at the top level due to flatten
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "int");
        assert_eq!(value["optional"], true);
        assert_eq!(value["default"], "5");
        assert_eq!(value["min"], 1);
        assert_eq!(value["max"], 10);

        let back: ParamSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    // 5. ExecutionConstraints defaults
    #[test]
    fn test_execution_constraints_defaults() {
        let c = ExecutionConstraints::default();
        assert_eq!(c.timeout_seconds, 60);
        assert_eq!(c.max_stdout_bytes, 1_048_576);
        assert_eq!(c.max_stderr_bytes, 1_048_576);
        assert!(c.cwd.is_none());
    }

    // 6. ExecutionConstraints serde with defaults
    #[test]
    fn test_execution_constraints_serde_defaults() {
        // Deserialize from empty object should give defaults
        let c: ExecutionConstraints = serde_json::from_str("{}").unwrap();
        assert_eq!(c.timeout_seconds, 60);
        assert_eq!(c.max_stdout_bytes, 1_048_576);

        // Custom values roundtrip
        let custom = ExecutionConstraints {
            timeout_seconds: 120,
            cwd: Some("/tmp".into()),
            max_stdout_bytes: 512,
            max_stderr_bytes: 256,
        };
        let json = serde_json::to_string(&custom).unwrap();
        let back: ExecutionConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(custom, back);
    }

    // 7. ToolSource default is Builtin
    #[test]
    fn test_tool_source_default_is_builtin() {
        assert_eq!(ToolSource::default(), ToolSource::Builtin);
    }

    // 8. CanvasTool full serde roundtrip
    #[test]
    fn test_canvas_tool_full_serde_roundtrip() {
        let mut tool = minimal_tool();
        tool.params.insert(
            "message".into(),
            ParamSpec {
                param_type: ParamType::Text { pattern: None },
                optional: false,
                default: None,
            },
        );

        let json = serde_json::to_string_pretty(&tool).unwrap();
        let back: CanvasTool = serde_json::from_str(&json).unwrap();

        assert_eq!(back.name, tool.name);
        assert_eq!(back.description, tool.description);
        assert_eq!(back.kind, tool.kind);
        assert_eq!(back.binary, tool.binary);
        assert_eq!(back.args_mode, tool.args_mode);
        assert_eq!(back.args, tool.args);
        assert_eq!(back.enabled, tool.enabled);
        assert_eq!(back.params.len(), 1);
        assert_eq!(back.constraints, tool.constraints);
    }

    // 9. CanvasTool source is skipped in serde
    #[test]
    fn test_canvas_tool_source_skipped_in_serde() {
        let mut tool = minimal_tool();
        tool.source = ToolSource::Session;

        let json = serde_json::to_string(&tool).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        // "source" should NOT appear in serialized output
        assert!(
            value.get("source").is_none(),
            "source field should be skipped"
        );

        // Deserializing gives the default (Builtin), not Session
        let back: CanvasTool = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, ToolSource::Builtin);
    }

    // 10. CanvasTool defaults for optional fields
    #[test]
    fn test_canvas_tool_deserialize_minimal_json() {
        let json = r#"{
            "name": "ls",
            "description": "List directory contents",
            "kind": "command"
        }"#;

        let tool: CanvasTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "ls");
        assert_eq!(tool.kind, BackendKind::Command);
        assert!(tool.binary.is_none());
        assert!(tool.api.is_none());
        assert_eq!(tool.args_mode, ArgsMode::Template); // default
        assert!(tool.args.is_empty());
        assert!(tool.allowed_subcommands.is_empty());
        assert!(tool.params.is_empty());
        assert_eq!(tool.constraints.timeout_seconds, 60);
        assert!(tool.enabled); // default true
        assert_eq!(tool.source, ToolSource::Builtin); // default
    }
}
