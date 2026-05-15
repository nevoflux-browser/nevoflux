//! Pattern detection engine for agent self-healing.

pub mod iteration_budget;
pub mod repeated_failure;

pub use iteration_budget::IterationBudgetDetector;
pub use repeated_failure::RepeatedToolFailureDetector;

use nevoflux_storage::TraceSpanRecord;

/// Context provided to detectors.
pub struct DetectionContext<'a> {
    pub session_id: &'a str,
    pub iteration: u32,
    pub max_iterations: u32,
    pub recent_tool_spans: &'a [TraceSpanRecord],
}

/// Trait for pattern detectors.
pub trait PatternDetector: Send {
    fn check(&self, ctx: &DetectionContext) -> Option<String>;
}

/// Engine that runs all detectors and tracks firing state.
pub struct PatternEngine {
    detectors: Vec<Box<dyn PatternDetector + Send>>,
    max_injections: u32,
    fired: Vec<bool>,
    injection_count: u32,
}

impl PatternEngine {
    pub fn new(detectors: Vec<Box<dyn PatternDetector + Send>>, max_injections: u32) -> Self {
        let fired = vec![false; detectors.len()];
        Self {
            detectors,
            max_injections,
            fired,
            injection_count: 0,
        }
    }

    pub fn default_engine() -> Self {
        Self::new(
            vec![
                Box::new(RepeatedToolFailureDetector::new(3)),
                Box::new(IterationBudgetDetector::new(0.7)),
            ],
            3,
        )
    }

    /// Check all detectors. Returns first triggered summary. Each detector fires at most once per session.
    pub fn check(&mut self, ctx: &DetectionContext) -> Option<String> {
        if self.injection_count >= self.max_injections {
            return None;
        }
        for (i, detector) in self.detectors.iter().enumerate() {
            if self.fired[i] {
                continue;
            }
            if let Some(summary) = detector.check(ctx) {
                self.fired[i] = true;
                self.injection_count += 1;
                return Some(summary);
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.fired = vec![false; self.detectors.len()];
        self.injection_count = 0;
    }

    pub fn injection_count(&self) -> u32 {
        self.injection_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_failing_span(iteration: u32) -> TraceSpanRecord {
        TraceSpanRecord {
            id: iteration as i64,
            session_id: "sess-1".into(),
            iteration,
            span_type: "tool_exec".into(),
            tool_name: Some("write_file".into()),
            tool_params: Some(r#"{"path":"/etc/config"}"#.into()),
            success: false,
            error_code: Some("PERM".into()),
            error_msg: None,
            duration_ms: Some(10),
        }
    }

    #[test]
    fn test_engine_fires_once_per_detector() {
        let mut engine = PatternEngine::default_engine();
        let spans = vec![
            make_failing_span(0),
            make_failing_span(1),
            make_failing_span(2),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 3,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(engine.check(&ctx).is_some());
        assert_eq!(engine.injection_count(), 1);
        assert!(engine.check(&ctx).is_none()); // Already fired
    }

    #[test]
    fn test_engine_respects_max_injections() {
        let mut engine = PatternEngine::new(
            vec![Box::new(RepeatedToolFailureDetector::new(1))],
            0, // max 0
        );
        let spans = vec![make_failing_span(0)];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 0,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(engine.check(&ctx).is_none());
    }

    #[test]
    fn test_engine_reset() {
        let mut engine = PatternEngine::default_engine();
        let spans = vec![
            make_failing_span(0),
            make_failing_span(1),
            make_failing_span(2),
        ];
        let ctx = DetectionContext {
            session_id: "sess-1",
            iteration: 3,
            max_iterations: 50,
            recent_tool_spans: &spans,
        };
        assert!(engine.check(&ctx).is_some());
        engine.reset();
        assert!(engine.check(&ctx).is_some());
    }
}
