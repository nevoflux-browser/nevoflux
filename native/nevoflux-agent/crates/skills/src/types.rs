//! Skill types and metadata.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Source of a skill.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    /// Built-in skill bundled with the agent.
    Builtin,
    /// User-defined skill from the user's skills directory.
    #[default]
    User,
    /// Plugin-provided skill.
    Plugin { plugin_id: String },
    /// Remote skill loaded from a URL.
    Remote { url: String },
}

/// Skill metadata from frontmatter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill name (identifier).
    pub name: String,
    /// Brief description for skill listing.
    #[serde(default)]
    pub description: String,
    /// Skill version.
    #[serde(default)]
    pub version: Option<String>,
    /// Author of the skill.
    #[serde(default)]
    pub author: Option<String>,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this skill is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Skills this depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Trigger patterns (when to auto-suggest this skill).
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Custom data.
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Tools this skill depends on. Supports glob patterns (e.g., "stitch*:*", "Read").
    /// If specified, the skill will only be injected if these tools are available.
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for SkillMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: None,
            author: None,
            tags: Vec::new(),
            enabled: true,
            dependencies: Vec::new(),
            triggers: Vec::new(),
            extra: serde_json::Value::Null,
            allowed_tools: Vec::new(),
        }
    }
}

impl SkillMetadata {
    /// Create metadata with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Add a dependency.
    pub fn with_dependency(mut self, dep: impl Into<String>) -> Self {
        self.dependencies.push(dep.into());
        self
    }

    /// Add a trigger pattern.
    pub fn with_trigger(mut self, trigger: impl Into<String>) -> Self {
        self.triggers.push(trigger.into());
        self
    }

    /// Add a required tool pattern.
    pub fn with_allowed_tool(mut self, tool: impl Into<String>) -> Self {
        self.allowed_tools.push(tool.into());
        self
    }
}

/// A skill definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    /// Skill metadata from frontmatter.
    pub metadata: SkillMetadata,
    /// Full content of the skill (markdown).
    pub content: String,
    /// Source of the skill.
    pub source: SkillSource,
    /// File path if loaded from disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<PathBuf>,
}

impl Skill {
    /// Create a new skill.
    pub fn new(metadata: SkillMetadata, content: impl Into<String>) -> Self {
        Self {
            metadata,
            content: content.into(),
            source: SkillSource::User,
            file_path: None,
        }
    }

    /// Create a builtin skill.
    pub fn builtin(metadata: SkillMetadata, content: impl Into<String>) -> Self {
        Self {
            metadata,
            content: content.into(),
            source: SkillSource::Builtin,
            file_path: None,
        }
    }

    /// Set the source.
    pub fn with_source(mut self, source: SkillSource) -> Self {
        self.source = source;
        self
    }

    /// Set the file path.
    pub fn with_file_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Get the skill name.
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Get the skill description.
    pub fn description(&self) -> &str {
        &self.metadata.description
    }

    /// Check if the skill is enabled.
    pub fn is_enabled(&self) -> bool {
        self.metadata.enabled
    }

    /// Estimate token count (rough: 4 chars = 1 token).
    pub fn estimated_tokens(&self) -> u32 {
        let chars = self.content.len() + self.metadata.description.len();
        (chars / 4) as u32
    }

    /// Read an auxiliary file relative to the skill's directory (Level 3 loading).
    ///
    /// This allows skills to reference additional files (templates, data, scripts)
    /// that are stored alongside the main skill file.
    pub fn read_auxiliary_file(&self, relative_path: &str) -> std::io::Result<String> {
        let base_dir = match &self.file_path {
            Some(path) => path.parent().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Skill has no parent directory",
                )
            })?,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Skill has no file path (not loaded from disk)",
                ))
            }
        };

        // Security: prevent path traversal attacks
        let normalized = PathBuf::from(relative_path);
        if normalized
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path traversal not allowed",
            ));
        }

        let full_path = base_dir.join(normalized);
        std::fs::read_to_string(full_path)
    }

    /// Get the skill's base directory path.
    pub fn base_dir(&self) -> Option<&std::path::Path> {
        self.file_path.as_ref().and_then(|p| p.parent())
    }
}

/// Brief skill info for listing (Level 1 loading).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillSummary {
    /// Skill name.
    pub name: String,
    /// Brief description.
    pub description: String,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Source of the skill.
    pub source: SkillSource,
    /// Whether enabled.
    pub enabled: bool,
    /// Estimated tokens if fully loaded.
    pub estimated_tokens: u32,
}

impl From<&Skill> for SkillSummary {
    fn from(skill: &Skill) -> Self {
        Self {
            name: skill.metadata.name.clone(),
            description: skill.metadata.description.clone(),
            tags: skill.metadata.tags.clone(),
            source: skill.source.clone(),
            enabled: skill.metadata.enabled,
            estimated_tokens: skill.estimated_tokens(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_source_default() {
        let source = SkillSource::default();
        assert_eq!(source, SkillSource::User);
    }

    #[test]
    fn test_skill_source_serialization() {
        let sources = vec![
            (SkillSource::Builtin, "\"builtin\""),
            (SkillSource::User, "\"user\""),
            (
                SkillSource::Plugin {
                    plugin_id: "test".into(),
                },
                r#"{"plugin":{"plugin_id":"test"}}"#,
            ),
        ];

        for (source, expected) in sources {
            let json = serde_json::to_string(&source).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_skill_metadata_default() {
        let meta = SkillMetadata::default();
        assert!(meta.name.is_empty());
        assert!(meta.enabled);
        assert!(meta.tags.is_empty());
        assert!(meta.allowed_tools.is_empty());
    }

    #[test]
    fn test_skill_metadata_builder() {
        let meta = SkillMetadata::new("test-skill")
            .with_description("A test skill")
            .with_version("1.0.0")
            .with_author("Test Author")
            .with_tag("testing")
            .with_tag("example")
            .with_enabled(true)
            .with_dependency("base-skill")
            .with_trigger("when testing")
            .with_allowed_tool("stitch*:*");

        assert_eq!(meta.name, "test-skill");
        assert_eq!(meta.description, "A test skill");
        assert_eq!(meta.version, Some("1.0.0".into()));
        assert_eq!(meta.author, Some("Test Author".into()));
        assert_eq!(meta.tags, vec!["testing", "example"]);
        assert!(meta.enabled);
        assert_eq!(meta.dependencies, vec!["base-skill"]);
        assert_eq!(meta.triggers, vec!["when testing"]);
        assert_eq!(meta.allowed_tools, vec!["stitch*:*"]);
    }

    #[test]
    fn test_skill_metadata_yaml_serialization() {
        let meta = SkillMetadata::new("code-review")
            .with_description("Review code for best practices")
            .with_tag("code");

        let yaml = serde_yaml::to_string(&meta).unwrap();
        assert!(yaml.contains("name: code-review"));
        assert!(yaml.contains("Review code"));
    }

    #[test]
    fn test_skill_metadata_yaml_deserialization() {
        let yaml = r#"
name: code-review
description: Review code for best practices
version: "1.0.0"
tags:
  - code
  - review
enabled: true
"#;

        let meta: SkillMetadata = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(meta.name, "code-review");
        assert_eq!(meta.description, "Review code for best practices");
        assert_eq!(meta.version, Some("1.0.0".into()));
        assert_eq!(meta.tags, vec!["code", "review"]);
        assert!(meta.enabled);
    }

    #[test]
    fn test_skill_new() {
        let meta = SkillMetadata::new("test").with_description("Test skill");
        let skill = Skill::new(meta, "# Test\n\nThis is a test skill.");

        assert_eq!(skill.name(), "test");
        assert_eq!(skill.description(), "Test skill");
        assert_eq!(skill.source, SkillSource::User);
        assert!(skill.file_path.is_none());
    }

    #[test]
    fn test_skill_builtin() {
        let meta = SkillMetadata::new("builtin-skill");
        let skill = Skill::builtin(meta, "Builtin content");

        assert_eq!(skill.source, SkillSource::Builtin);
    }

    #[test]
    fn test_skill_with_source() {
        let meta = SkillMetadata::new("plugin-skill");
        let skill = Skill::new(meta, "Content").with_source(SkillSource::Plugin {
            plugin_id: "my-plugin".into(),
        });

        assert!(matches!(skill.source, SkillSource::Plugin { .. }));
    }

    #[test]
    fn test_skill_with_file_path() {
        let meta = SkillMetadata::new("file-skill");
        let skill = Skill::new(meta, "Content").with_file_path("/path/to/skill.md");

        assert_eq!(skill.file_path, Some(PathBuf::from("/path/to/skill.md")));
    }

    #[test]
    fn test_skill_is_enabled() {
        let enabled_meta = SkillMetadata::new("enabled").with_enabled(true);
        let disabled_meta = SkillMetadata::new("disabled").with_enabled(false);

        let enabled_skill = Skill::new(enabled_meta, "");
        let disabled_skill = Skill::new(disabled_meta, "");

        assert!(enabled_skill.is_enabled());
        assert!(!disabled_skill.is_enabled());
    }

    #[test]
    fn test_skill_estimated_tokens() {
        // 100 chars content + 20 chars description = 120 chars / 4 = 30 tokens
        let meta = SkillMetadata::new("test").with_description("12345678901234567890"); // 20 chars
        let skill = Skill::new(meta, "x".repeat(100));

        assert_eq!(skill.estimated_tokens(), 30);
    }

    #[test]
    fn test_skill_summary_from_skill() {
        let meta = SkillMetadata::new("summary-test")
            .with_description("Test description")
            .with_tag("test");
        let skill = Skill::new(meta, "x".repeat(400)).with_source(SkillSource::Builtin);

        let summary = SkillSummary::from(&skill);
        assert_eq!(summary.name, "summary-test");
        assert_eq!(summary.description, "Test description");
        assert_eq!(summary.tags, vec!["test"]);
        assert_eq!(summary.source, SkillSource::Builtin);
        assert!(summary.enabled);
        assert!(summary.estimated_tokens > 0);
    }

    #[test]
    fn test_skill_json_serialization() {
        let meta = SkillMetadata::new("json-test").with_description("JSON test");
        let skill = Skill::new(meta, "Content here");

        let json = serde_json::to_string(&skill).unwrap();
        let decoded: Skill = serde_json::from_str(&json).unwrap();

        assert_eq!(skill.name(), decoded.name());
        assert_eq!(skill.content, decoded.content);
    }
}
