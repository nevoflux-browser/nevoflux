//! Detector for repeated tool failures with similar parameters.

use super::{DetectionContext, PatternDetector};

pub struct RepeatedToolFailureDetector {
    threshold: u32,
}

impl RepeatedToolFailureDetector {
    pub fn new(threshold: u32) -> Self {
        Self { threshold }
    }
}

impl PatternDetector for RepeatedToolFailureDetector {
    fn check(&self, ctx: &DetectionContext) -> Option<String> {
        let spans = ctx.recent_tool_spans;
        if spans.len() < self.threshold as usize {
            return None;
        }

        let recent = &spans[spans.len().saturating_sub(self.threshold as usize)..];

        // All must be failures
        if !recent.iter().all(|s| !s.success) {
            return None;
        }

        // All must have the same tool name
        let first_tool = recent[0].tool_name.as_deref()?;
        if !recent
            .iter()
            .all(|s| s.tool_name.as_deref() == Some(first_tool))
        {
            return None;
        }

        // All must have similar params
        let first_params = recent[0].tool_params.as_deref();
        if !recent
            .iter()
            .all(|s| s.tool_params.as_deref() == first_params)
        {
            return None;
        }

        let start_iter = recent.first().map(|s| s.iteration).unwrap_or(0);
        let end_iter = recent.last().map(|s| s.iteration).unwrap_or(0);
        let error_code = recent
            .last()
            .and_then(|s| s.error_code.as_deref())
            .unwrap_or("unknown");
        let target = first_params.unwrap_or("unknown");

        Some(format!(
            "[Trace Summary] In iterations {}-{}, {}(\"{}\") failed {} consecutive times with error: {}. Please try a different approach or parameters.",
            start_iter, end_iter, first_tool, target, self.threshold, error_code
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::TraceSpanRecord;

    fn make_tool_span(
        iteration: u32,
        tool: &str,
        params: &str,
        success: bool,
        error_code: Option<&str>,
    ) -> TraceSpanRecord {
        TraceSpanRecord {
            id: iteration as i64,
            session_id: "sess-1".into(),
            iteration,
            span_type: "tool_exec".into(),
            tool_name: Some(tool.into()),
            tool_params: Some(params.into()),
            success,
            error_code: error_code.map(|s| s.into()),
            error_msg: None,
            duration_ms: Some(10),
        }
    }

    #[test]
    fn test_detects_repeated_failure() {
        let detector = RepeatedToolFailureDetector::new(3);
        let spans = vec![
            make_tool_span(0, "write_file", "/etc/config", false, Some("PERM")),
            make_tool_span(1, "write_file", "/etc/config", false, Some("PERM")),
            make_tool_span(2, "write_file", "/etc/config", false, Some("PERM")),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 3,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        let result = detector.check(&ctx);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("write_file"));
        assert!(msg.contains("PERM"));
        assert!(msg.contains("3 consecutive times"));
    }

    #[test]
    fn test_no_detection_when_mixed_success() {
        let detector = RepeatedToolFailureDetector::new(3);
        let spans = vec![
            make_tool_span(0, "write_file", "/etc/config", false, Some("PERM")),
            make_tool_span(1, "write_file", "/etc/config", true, None),
            make_tool_span(2, "write_file", "/etc/config", false, Some("PERM")),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 3,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(detector.check(&ctx).is_none());
    }

    #[test]
    fn test_no_detection_different_tools() {
        let detector = RepeatedToolFailureDetector::new(3);
        let spans = vec![
            make_tool_span(0, "write_file", "/etc/config", false, Some("PERM")),
            make_tool_span(1, "read_file", "/etc/config", false, Some("PERM")),
            make_tool_span(2, "write_file", "/etc/config", false, Some("PERM")),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 3,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(detector.check(&ctx).is_none());
    }

    #[test]
    fn test_no_detection_below_threshold() {
        let detector = RepeatedToolFailureDetector::new(3);
        let spans = vec![
            make_tool_span(0, "write_file", "/etc/config", false, Some("PERM")),
            make_tool_span(1, "write_file", "/etc/config", false, Some("PERM")),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 2,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(detector.check(&ctx).is_none());
    }
}
