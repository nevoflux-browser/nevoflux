//! Detector for iteration budget consumption warning.

use super::{DetectionContext, PatternDetector};

pub struct IterationBudgetDetector {
    warn_ratio: f32,
}

impl IterationBudgetDetector {
    pub fn new(warn_ratio: f32) -> Self {
        Self { warn_ratio }
    }
}

impl PatternDetector for IterationBudgetDetector {
    fn check(&self, ctx: &DetectionContext) -> Option<String> {
        if ctx.max_iterations == 0 {
            return None;
        }
        let ratio = ctx.iteration as f32 / ctx.max_iterations as f32;
        if ratio < self.warn_ratio {
            return None;
        }

        let total = ctx.recent_tool_spans.len();
        let success_count = ctx.recent_tool_spans.iter().filter(|s| s.success).count();
        let fail_count = total - success_count;

        let mut tool_names: Vec<&str> = ctx
            .recent_tool_spans
            .iter()
            .filter_map(|s| s.tool_name.as_deref())
            .collect();
        tool_names.dedup();
        let tools_summary = if tool_names.is_empty() {
            "no tools".to_string()
        } else {
            tool_names.join(", ")
        };

        Some(format!(
            "[Trace Summary] Used {}/{} iterations. Tools called: {} ({} succeeded, {} failed). Please evaluate whether to adjust strategy or conclude the task.",
            ctx.iteration, ctx.max_iterations, tools_summary, success_count, fail_count
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::TraceSpanRecord;

    fn make_tool_span(iteration: u32, tool: &str, success: bool) -> TraceSpanRecord {
        TraceSpanRecord {
            id: iteration as i64,
            session_id: "sess-1".into(),
            iteration,
            span_type: "tool_exec".into(),
            tool_name: Some(tool.into()),
            tool_params: Some("{}".into()),
            success,
            error_code: if success { None } else { Some("ERR".into()) },
            error_msg: None,
            duration_ms: Some(10),
        }
    }

    #[test]
    fn test_triggers_at_threshold() {
        let detector = IterationBudgetDetector::new(0.7);
        let spans = vec![
            make_tool_span(0, "read_file", true),
            make_tool_span(1, "write_file", false),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 35,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        let result = detector.check(&ctx);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("35/50"));
    }

    #[test]
    fn test_no_trigger_below_threshold() {
        let detector = IterationBudgetDetector::new(0.7);
        let spans = vec![make_tool_span(0, "read_file", true)];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 10,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(detector.check(&ctx).is_none());
    }

    #[test]
    fn test_summary_includes_tool_stats() {
        let detector = IterationBudgetDetector::new(0.7);
        let spans = vec![
            make_tool_span(0, "read_file", true),
            make_tool_span(1, "write_file", true),
            make_tool_span(2, "write_file", false),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 40,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        let result = detector.check(&ctx);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("2 succeeded"));
        assert!(msg.contains("1 failed"));
    }
}
