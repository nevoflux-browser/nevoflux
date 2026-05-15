use serde::{Deserialize, Serialize};

/// Type of trace span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    LlmCall,
    ToolExec,
}

/// Lightweight span record for SQLite (pattern detection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub session_id: String,
    pub iteration: u32,
    pub span_type: SpanType,
    pub tool_name: Option<String>,
    pub tool_params: Option<String>,
    pub success: bool,
    pub error_code: Option<String>,
    pub error_msg: Option<String>,
    pub duration_ms: u64,
}

/// Full span record for JSONL file (developer debugging).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullTraceSpan {
    pub ts: String,
    pub session: String,
    pub iter: u32,
    #[serde(rename = "type")]
    pub span_type: SpanType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub duration_ms: u64,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_span_creation() {
        let span = TraceSpan {
            session_id: "sess-001".to_string(),
            iteration: 3,
            span_type: SpanType::ToolExec,
            tool_name: Some("write_file".to_string()),
            tool_params: Some(r#"{"path":"/etc/config"}"#.to_string()),
            success: false,
            error_code: Some("PERMISSION_DENIED".to_string()),
            error_msg: Some("Permission denied".to_string()),
            duration_ms: 12,
        };
        assert_eq!(span.iteration, 3);
        assert!(!span.success);
    }

    #[test]
    fn test_full_span_serializes_to_jsonl() {
        let span = FullTraceSpan {
            ts: "2026-02-04T10:00:01Z".to_string(),
            session: "abc123".to_string(),
            iter: 1,
            span_type: SpanType::ToolExec,
            tool: Some("write_file".to_string()),
            params: Some(serde_json::json!({"path": "/etc/config"})),
            request: None,
            response: None,
            result: Some(serde_json::json!({"success": false, "error": "PERMISSION_DENIED"})),
            duration_ms: 12,
            success: false,
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(json.contains("write_file"));
        assert!(!json.contains("request")); // skip_serializing_if works
    }

    #[test]
    fn test_span_type_serialization() {
        assert_eq!(
            serde_json::to_string(&SpanType::LlmCall).unwrap(),
            r#""llm_call""#
        );
        assert_eq!(
            serde_json::to_string(&SpanType::ToolExec).unwrap(),
            r#""tool_exec""#
        );
    }
}
