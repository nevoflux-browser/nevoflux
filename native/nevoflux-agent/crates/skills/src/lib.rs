//! NevoFlux Skills - Skills system for NevoFlux Agent.
//!
//! This crate provides the skills management system for NevoFlux Agent.
//! Skills are markdown-based prompts with YAML frontmatter that can be
//! loaded into the LLM context to provide specialized capabilities.
//!
//! # Three-Layer Loading Model
//!
//! The skills system uses a three-layer loading model to optimize token usage:
//!
//! | Layer | Function | When | Token Cost |
//! |-------|----------|------|------------|
//! | Level 1 | `list()` | At startup | ~100 tokens/skill |
//! | Level 2 | `get()` | When LLM decides to use | <5k tokens |
//! | Level 3 | `read_file()` / `execute()` | On demand | Variable |
//!
//! # Skill File Format
//!
//! Skills are markdown files with YAML frontmatter:
//!
//! ```markdown
//! ---
//! name: code-review
//! description: Review code for best practices
//! tags:
//!   - code
//!   - review
//! ---
//!
//! # Code Review
//!
//! When reviewing code, follow these guidelines...
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_skills::{SkillRegistry, LoaderConfig};
//!
//! // Create registry with configuration
//! let config = LoaderConfig::new()
//!     .with_user_dir("~/.config/nevoflux/skills");
//! let mut registry = SkillRegistry::with_config(config);
//!
//! // Load skills from directories
//! registry.load()?;
//!
//! // Level 1: List skills (lightweight summaries)
//! for summary in registry.list() {
//!     println!("{}: {}", summary.name, summary.description);
//! }
//!
//! // Level 2: Get full skill content
//! if let Some(skill) = registry.get("code-review") {
//!     println!("Content: {}", skill.content);
//! }
//! ```

pub mod error;
pub mod loader;
pub mod parser;
pub mod registry;
pub mod tool_check;
pub mod types;

// Re-export main types
pub use error::{Result, SkillsError};
#[allow(deprecated)]
pub use loader::default_user_skills_dir;
pub use loader::{
    default_user_skills_dirs, install_default_skills, nevoflux_user_skills_dir, AsyncSkillLoader,
    LoaderConfig, SkillLoader,
};
pub use parser::{parse_skill, parse_skill_file, serialize_skill};
pub use registry::{AsyncSkillRegistry, SkillRegistry};
pub use tool_check::{check_tool_availability, format_missing_tools_message, ToolCheckResult};
pub use types::{Skill, SkillMetadata, SkillSource, SkillSummary};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports_available() {
        // Verify all re-exports are accessible
        let _ = SkillMetadata::new("test");
        let _ = SkillSource::User;
        let _ = LoaderConfig::new();
        let _ = SkillRegistry::new();
    }

    #[test]
    fn test_full_workflow() {
        // Test the full workflow: parse -> register -> list -> get
        let content = r#"---
name: test-workflow
description: Test the full workflow
tags:
  - testing
---

# Test Workflow

This tests the full workflow.
"#;

        // Parse
        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.name(), "test-workflow");

        // Register
        let mut registry = SkillRegistry::new();
        registry.register(skill).unwrap();

        // List (Level 1)
        let summaries = registry.list();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "test-workflow");

        // Get (Level 2)
        let retrieved = registry.get("test-workflow").unwrap();
        assert!(retrieved.content.contains("full workflow"));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let original = Skill::new(
            SkillMetadata::new("roundtrip")
                .with_description("Test roundtrip")
                .with_tag("test"),
            "# Content\n\nBody text.",
        );

        let serialized = serialize_skill(&original).unwrap();
        let reparsed = parse_skill(&serialized, SkillSource::User).unwrap();

        assert_eq!(original.name(), reparsed.name());
        assert_eq!(original.description(), reparsed.description());
    }
}
