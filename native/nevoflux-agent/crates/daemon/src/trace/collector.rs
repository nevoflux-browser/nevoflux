//! TraceCollector — dual-track span collection.
//!
//! Writes lightweight summaries to SQLite (always on) and
//! full payloads to JSONL files (when --trace is enabled).

use crate::trace::file_writer::TraceFileWriter;
use crate::trace::models::{FullTraceSpan, SpanType};
use nevoflux_storage::{CreateTraceSpanParams, Storage, TraceSpanRecord};
use std::sync::Arc;

/// Dual-track trace collector.
pub struct TraceCollector {
    storage: Arc<Storage>,
    file_writer: Option<TraceFileWriter>,
}

impl TraceCollector {
    /// Create a collector with SQLite only (no file output).
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            file_writer: None,
        }
    }

    /// Create a collector with both SQLite and JSONL file output.
    pub fn with_file_writer(storage: Arc<Storage>, writer: TraceFileWriter) -> Self {
        Self {
            storage,
            file_writer: Some(writer),
        }
    }

    /// Record a tool execution span.
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_exec(
        &self,
        session_id: &str,
        iteration: u32,
        tool_name: &str,
        params_summary: Option<String>,
        success: bool,
        error_code: Option<String>,
        error_msg: Option<String>,
        duration_ms: u64,
        full_params: Option<serde_json::Value>,
        full_result: Option<serde_json::Value>,
    ) {
        // SQLite track (always)
        match self.storage.traces().create(CreateTraceSpanParams {
            session_id: session_id.to_string(),
            iteration,
            span_type: "tool_exec".to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_params: params_summary,
            success,
            error_code: error_code.clone(),
            error_msg: error_msg.clone(),
            duration_ms: Some(duration_ms),
        }) {
            Ok(id) => {
                tracing::debug!(
                    "Trace span written: id={}, tool={}, success={}",
                    id,
                    tool_name,
                    success
                );
            }
            Err(e) => {
                tracing::warn!("Failed to write trace span to SQLite: {}", e);
            }
        }

        // JSONL track (if enabled)
        if let Some(writer) = &self.file_writer {
            let span = FullTraceSpan {
                ts: chrono::Utc::now().to_rfc3339(),
                session: session_id.to_string(),
                iter: iteration,
                span_type: SpanType::ToolExec,
                tool: Some(tool_name.to_string()),
                params: full_params,
                request: None,
                response: None,
                result: full_result,
                duration_ms,
                success,
            };
            let _ = writer.append(&span);
        }
    }

    /// Record an LLM call span.
    #[allow(clippy::too_many_arguments)]
    pub fn record_llm_call(
        &self,
        session_id: &str,
        iteration: u32,
        success: bool,
        error_code: Option<String>,
        error_msg: Option<String>,
        duration_ms: u64,
        full_request: Option<serde_json::Value>,
        full_response: Option<serde_json::Value>,
    ) {
        // SQLite track (always)
        let _ = self.storage.traces().create(CreateTraceSpanParams {
            session_id: session_id.to_string(),
            iteration,
            span_type: "llm_call".to_string(),
            tool_name: None,
            tool_params: None,
            success,
            error_code: error_code.clone(),
            error_msg: error_msg.clone(),
            duration_ms: Some(duration_ms),
        });

        // JSONL track (if enabled)
        if let Some(writer) = &self.file_writer {
            let span = FullTraceSpan {
                ts: chrono::Utc::now().to_rfc3339(),
                session: session_id.to_string(),
                iter: iteration,
                span_type: SpanType::LlmCall,
                tool: None,
                params: None,
                request: full_request,
                response: full_response,
                result: None,
                duration_ms,
                success,
            };
            let _ = writer.append(&span);
        }
    }

    /// Get recent tool spans for pattern detection.
    pub fn recent_tool_spans(&self, session_id: &str, limit: u32) -> Vec<TraceSpanRecord> {
        self.storage
            .traces()
            .recent_tool_spans(session_id, limit)
            .unwrap_or_default()
    }

    /// Get total span count for a session.
    pub fn span_count(&self, session_id: &str) -> u32 {
        self.storage
            .traces()
            .count_by_session(session_id)
            .unwrap_or(0)
    }

    /// Cleanup traces for a completed session.
    pub fn cleanup_session(&self, session_id: &str) {
        let _ = self.storage.traces().delete_by_session(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Arc<Storage> {
        Arc::new(Storage::open_in_memory().unwrap())
    }

    #[test]
    fn test_record_tool_exec_sqlite_only() {
        let storage = setup();
        let collector = TraceCollector::new(storage.clone());

        collector.record_tool_exec(
            "sess-1",
            0,
            "write_file",
            Some(r#"{"path":"/tmp/test"}"#.to_string()),
            false,
            Some("PERMISSION_DENIED".into()),
            Some("Permission denied".into()),
            12,
            None,
            None,
        );

        let spans = storage.traces().list_by_session("sess-1").unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].tool_name.as_deref(), Some("write_file"));
        assert!(!spans[0].success);
    }

    #[test]
    fn test_record_llm_call() {
        let storage = setup();
        let collector = TraceCollector::new(storage.clone());

        collector.record_llm_call("sess-1", 0, true, None, None, 2340, None, None);

        let spans = storage.traces().list_by_session("sess-1").unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].span_type, "llm_call");
        assert!(spans[0].success);
    }

    #[test]
    fn test_cleanup_session() {
        let storage = setup();
        let collector = TraceCollector::new(storage.clone());

        collector.record_llm_call("sess-1", 0, true, None, None, 100, None, None);
        collector.record_llm_call("sess-2", 0, true, None, None, 100, None, None);

        collector.cleanup_session("sess-1");

        assert_eq!(collector.span_count("sess-1"), 0);
        assert_eq!(collector.span_count("sess-2"), 1);
    }
}
