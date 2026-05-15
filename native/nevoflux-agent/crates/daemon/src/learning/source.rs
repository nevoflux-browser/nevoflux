use super::types::LearningEntry;

/// Trait for components that produce learning entries.
/// Implemented by Agent Runner, MCP Tools, WASM Host, and Bridge.
pub trait LearningSource: Send + Sync {
    /// Human-readable name of this source (e.g., "agent_runner", "mcp_tools").
    fn source_name(&self) -> &str;

    /// Collect pending learning entries from this source.
    /// Called by the LearningCollector during its collection cycle.
    fn collect(&self) -> Vec<LearningEntry>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::types::*;

    struct MockSource {
        entries: Vec<LearningEntry>,
    }

    impl LearningSource for MockSource {
        fn source_name(&self) -> &str {
            "mock"
        }

        fn collect(&self) -> Vec<LearningEntry> {
            self.entries.clone()
        }
    }

    #[test]
    fn mock_source_produces_entries() {
        let source = MockSource {
            entries: vec![LearningEntry::new(
                LearningCategory::SiteInteraction,
                "test",
                "test summary",
            )],
        };
        assert_eq!(source.source_name(), "mock");
        assert_eq!(source.collect().len(), 1);
    }
}
