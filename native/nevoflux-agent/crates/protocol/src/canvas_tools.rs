//! Canvas Tool protocol message types.
//!
//! Defines wire types for canvas tool invocation requests/responses,
//! tool listing, and streaming execution events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to invoke a canvas tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasToolInvokeRequest {
    pub tool_name: String,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    pub session_id: String,
    /// Caller-supplied call ID. The daemon echoes this in events and the
    /// response so the caller can correlate without owning the daemon's
    /// internal invocation tracking.
    #[serde(default)]
    pub call_id: Option<String>,
    /// Optional timeout override in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Response from a canvas tool invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasToolInvokeResponse {
    pub tool_name: String,
    pub success: bool,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
    pub duration_ms: u64,
    /// Echoes the caller's call_id (or daemon-generated id when none supplied).
    pub call_id: String,
}

/// Request to list available tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolListRequest {
    #[serde(default)]
    pub include_disabled: bool,
}

/// A tool summary in a list response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolSummary {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kind: String,
    pub args_mode: Option<String>,
    pub enabled: bool,
    pub source: String,
    /// Same string as `source`, but named for semantic clarity. Kept
    /// alongside `source` for backward compatibility.
    #[serde(default)]
    pub origin_source: String,
    /// `true` if this User-source entry shadows a Builtin of the same name.
    #[serde(default)]
    pub is_override: bool,
}

/// Response listing available tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolListResponse {
    pub tools: Vec<CanvasToolSummary>,
}

/// Canvas tool event (streamed during execution).
///
/// Wire format uses tag `event_type` so JSON looks like:
///   `{ "event_type": "stdout", "call_id": "...", "data": "..." }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum CanvasToolEvent {
    /// Tool execution has started.
    Started { call_id: String, tool_name: String },
    /// A chunk of stdout output.
    Stdout { call_id: String, data: String },
    /// A chunk of stderr output.
    Stderr { call_id: String, data: String },
    /// Optional progress update (0.0 ..= 1.0). Currently emitted only by
    /// tools whose executor knows how to parse their progress markers.
    Progress {
        call_id: String,
        progress: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Tool execution finished. Carries the final disposition.
    Finished {
        call_id: String,
        success: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        duration_ms: u64,
    },
    /// Fatal error during execution (binary missing, spawn failed, timeout, etc).
    /// On a successful spawn that produced a non-zero exit, prefer `Finished` with
    /// `success: false` instead.
    Error { call_id: String, error: String },
}

// ---------------------------------------------------------------------------
// Error payload shared by save / delete / get_raw / validate
// ---------------------------------------------------------------------------

/// Machine-readable error codes returned by Canvas Tool CRUD commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolError {
    /// Stable identifier used by the UI to branch on failure cause.
    /// Known values: `toml_parse`, `validation`, `name_changed`,
    /// `name_conflict`, `io`, `not_found`, `invalid_source`,
    /// `no_raw_for_session`.
    pub code: String,
    /// Human-readable description.
    pub message: String,
    /// Optional dotted path to the offending field (e.g. `params.foo.pattern`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

// ---------------------------------------------------------------------------
// canvas.tool.get_raw
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolGetRawRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolGetRawResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toml_text: Option<String>,
    /// Source the text was read from: `"builtin"` or `"user"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasToolError>,
}

// ---------------------------------------------------------------------------
// canvas.tool.save
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolSaveRequest {
    pub toml_text: String,
    /// When present (edit mode), the daemon enforces that the parsed
    /// `name` equals this value — prevents accidental rename.
    #[serde(default)]
    pub expected_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolSaveResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasToolError>,
}

// ---------------------------------------------------------------------------
// canvas.tool.delete
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolDeleteRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolDeleteResponse {
    pub success: bool,
    /// `true` if deleting the User file restored a shadowed Builtin
    /// (tells the UI to phrase confirmation as "revert" vs "delete").
    #[serde(default)]
    pub was_override: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasToolError>,
}

// ---------------------------------------------------------------------------
// canvas.tool.validate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolValidateRequest {
    pub toml_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasToolValidateResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasToolError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_invoke_request_roundtrip() {
        let mut params = HashMap::new();
        params.insert("file".to_string(), "main.rs".to_string());
        params.insert("line".to_string(), "42".to_string());

        let req = CanvasToolInvokeRequest {
            tool_name: "run_tests".to_string(),
            params,
            args: Some(vec!["--verbose".to_string(), "--nocapture".to_string()]),
            session_id: "sess-abc123".to_string(),
            call_id: Some("call-001".to_string()),
            timeout_ms: Some(60000),
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: CanvasToolInvokeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_invoke_request_defaults() {
        let json = r#"{"tool_name":"lint","session_id":"s1"}"#;
        let req: CanvasToolInvokeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool_name, "lint");
        assert!(req.params.is_empty());
        assert!(req.args.is_none());
        assert_eq!(req.session_id, "s1");
        assert!(req.call_id.is_none());
        assert!(req.timeout_ms.is_none());
    }

    #[test]
    fn test_invoke_response_roundtrip() {
        let resp = CanvasToolInvokeResponse {
            tool_name: "cargo_test".to_string(),
            success: true,
            stdout: Some("test result: ok".to_string()),
            stderr: None,
            exit_code: Some(0),
            error: None,
            duration_ms: 1500,
            call_id: "call-001".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasToolInvokeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_invoke_response_failure() {
        let resp = CanvasToolInvokeResponse {
            tool_name: "build".to_string(),
            success: false,
            stdout: None,
            stderr: Some("error[E0308]: mismatched types".to_string()),
            exit_code: Some(1),
            error: Some("Compilation failed".to_string()),
            duration_ms: 3200,
            call_id: "call-002".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasToolInvokeResponse = serde_json::from_str(&json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error.as_deref(), Some("Compilation failed"));
        assert_eq!(decoded.exit_code, Some(1));
    }

    #[test]
    fn test_list_request_roundtrip() {
        let req = CanvasToolListRequest {
            include_disabled: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: CanvasToolListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_list_request_default() {
        let json = r#"{}"#;
        let req: CanvasToolListRequest = serde_json::from_str(json).unwrap();
        assert!(!req.include_disabled);
    }

    #[test]
    fn test_list_response_roundtrip() {
        let resp = CanvasToolListResponse {
            tools: vec![
                CanvasToolSummary {
                    name: "cargo_test".to_string(),
                    description: Some("Run cargo tests".to_string()),
                    kind: "shell".to_string(),
                    args_mode: Some("params".to_string()),
                    enabled: true,
                    source: "builtin".to_string(),
                    origin_source: "builtin".to_string(),
                    is_override: false,
                },
                CanvasToolSummary {
                    name: "eslint".to_string(),
                    description: None,
                    kind: "shell".to_string(),
                    args_mode: None,
                    enabled: false,
                    source: "user".to_string(),
                    origin_source: "user".to_string(),
                    is_override: false,
                },
            ],
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasToolListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
        assert_eq!(decoded.tools.len(), 2);
        assert_eq!(decoded.tools[0].name, "cargo_test");
        assert!(decoded.tools[0].description.is_some());
        assert!(!decoded.tools[1].enabled);
    }

    #[test]
    fn test_event_started_roundtrip() {
        let event = CanvasToolEvent::Started {
            call_id: "call-100".to_string(),
            tool_name: "build".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"started"#));
        assert!(json.contains(r#""call_id":"call-100"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_stdout_roundtrip() {
        let event = CanvasToolEvent::Stdout {
            call_id: "call-100".to_string(),
            data: "Compiling nevoflux v0.1.0\n".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"stdout"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_stderr_roundtrip() {
        let event = CanvasToolEvent::Stderr {
            call_id: "call-100".to_string(),
            data: "warning: unused import\n".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"stderr"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_progress_roundtrip() {
        let event = CanvasToolEvent::Progress {
            call_id: "call-100".to_string(),
            progress: 0.42,
            message: Some("Encoding frame 1234".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"progress"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_finished_roundtrip() {
        let event = CanvasToolEvent::Finished {
            call_id: "call-100".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 5432,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"finished"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_error_roundtrip() {
        let event = CanvasToolEvent::Error {
            call_id: "call-100".to_string(),
            error: "Process killed by signal 9".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event_type":"error"#));
        let decoded: CanvasToolEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_event_tag_based_deserialization() {
        // Verify that the serde tag-based dispatch works correctly
        let started_json = r#"{"event_type":"started","call_id":"x","tool_name":"t"}"#;
        let stdout_json = r#"{"event_type":"stdout","call_id":"x","data":"out"}"#;
        let stderr_json = r#"{"event_type":"stderr","call_id":"x","data":"warn"}"#;
        let progress_json = r#"{"event_type":"progress","call_id":"x","progress":0.5}"#;
        let finished_json =
            r#"{"event_type":"finished","call_id":"x","success":false,"duration_ms":0}"#;
        let error_json = r#"{"event_type":"error","call_id":"x","error":"boom"}"#;

        let started: CanvasToolEvent = serde_json::from_str(started_json).unwrap();
        let stdout: CanvasToolEvent = serde_json::from_str(stdout_json).unwrap();
        let stderr: CanvasToolEvent = serde_json::from_str(stderr_json).unwrap();
        let progress: CanvasToolEvent = serde_json::from_str(progress_json).unwrap();
        let finished: CanvasToolEvent = serde_json::from_str(finished_json).unwrap();
        let error: CanvasToolEvent = serde_json::from_str(error_json).unwrap();

        assert!(matches!(started, CanvasToolEvent::Started { .. }));
        assert!(matches!(stdout, CanvasToolEvent::Stdout { .. }));
        assert!(matches!(stderr, CanvasToolEvent::Stderr { .. }));
        assert!(matches!(progress, CanvasToolEvent::Progress { .. }));
        assert!(matches!(finished, CanvasToolEvent::Finished { .. }));
        assert!(matches!(error, CanvasToolEvent::Error { .. }));
    }
}
