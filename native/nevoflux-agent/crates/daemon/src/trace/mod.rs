pub mod collector;
pub mod detection;
pub mod file_writer;
pub mod models;

pub use collector::TraceCollector;
pub use detection::{
    DetectionContext, IterationBudgetDetector, PatternDetector, PatternEngine,
    RepeatedToolFailureDetector,
};
pub use file_writer::TraceFileWriter;
pub use models::{FullTraceSpan, SpanType, TraceSpan};
