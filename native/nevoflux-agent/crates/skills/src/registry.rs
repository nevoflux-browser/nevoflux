//! Skill registry for managing loaded skills.

use crate::error::{Result, SkillsError};
use crate::loader::{AsyncSkillLoader, LoaderConfig, SkillLoader};
use crate::types::{Skill, SkillSource, SkillSummary};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Skill registry for managing skills.
///
/// Supports the three-layer loading model:
/// - Level 1: `list()` - returns summaries with ~100 tokens per skill
/// - Level 2: `get()` - returns full skill content (<5k tokens)
/// - Level 3: `read_file()` / `execute()` - on-demand operations
pub struct SkillRegistry {
    /// Loaded skills by name.
    skills: HashMap<String, Skill>,
    /// Loader configuration.
    loader_config: LoaderConfig,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            loader_config: LoaderConfig::default(),
        }
    }

    /// Create a registry with a specific loader configuration.
    pub fn with_config(config: LoaderConfig) -> Self {
        Self {
            skills: HashMap::new(),
            loader_config: config,
        }
    }

    /// Get the number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Load skills from configured directories.
    pub fn load(&mut self) -> Result<usize> {
        let loader = SkillLoader::new(self.loader_config.clone());
        let skills = loader.load_all()?;
        let count = skills.len();

        for skill in skills {
            self.skills.insert(skill.name().to_string(), skill);
        }

        info!("Loaded {} skills into registry", count);
        Ok(count)
    }

    /// Reload all skills (clears and reloads).
    pub fn reload(&mut self) -> Result<usize> {
        self.skills.clear();
        self.load()
    }

    /// Register a skill manually.
    pub fn register(&mut self, skill: Skill) -> Result<()> {
        let name = skill.name().to_string();

        if self.skills.contains_key(&name) {
            return Err(SkillsError::AlreadyExists(name));
        }

        debug!("Registered skill: {}", name);
        self.skills.insert(name, skill);
        Ok(())
    }

    /// Register a skill, replacing if it already exists.
    pub fn register_or_replace(&mut self, skill: Skill) {
        let name = skill.name().to_string();
        debug!("Registered/replaced skill: {}", name);
        self.skills.insert(name, skill);
    }

    /// Unregister a skill by name.
    pub fn unregister(&mut self, name: &str) -> Option<Skill> {
        self.skills.remove(name)
    }

    /// Check if a skill exists.
    pub fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Get a skill by name (Level 2 loading).
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Get a mutable reference to a skill.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Skill> {
        self.skills.get_mut(name)
    }

    /// List all skill summaries (Level 1 loading).
    pub fn list(&self) -> Vec<SkillSummary> {
        self.skills.values().map(SkillSummary::from).collect()
    }

    /// List skill summaries filtered by source.
    pub fn list_by_source(&self, source: &SkillSource) -> Vec<SkillSummary> {
        self.skills
            .values()
            .filter(|s| &s.source == source)
            .map(SkillSummary::from)
            .collect()
    }

    /// List skill summaries filtered by tag.
    pub fn list_by_tag(&self, tag: &str) -> Vec<SkillSummary> {
        self.skills
            .values()
            .filter(|s| s.metadata.tags.contains(&tag.to_string()))
            .map(SkillSummary::from)
            .collect()
    }

    /// List enabled skill summaries.
    pub fn list_enabled(&self) -> Vec<SkillSummary> {
        self.skills
            .values()
            .filter(|s| s.is_enabled())
            .map(SkillSummary::from)
            .collect()
    }

    /// Get all skill names.
    pub fn names(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Search skills by name or description.
    pub fn search(&self, query: &str) -> Vec<SkillSummary> {
        let query_lower = query.to_lowercase();

        self.skills
            .values()
            .filter(|s| {
                s.name().to_lowercase().contains(&query_lower)
                    || s.description().to_lowercase().contains(&query_lower)
                    || s.metadata
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .map(SkillSummary::from)
            .collect()
    }

    /// Find skills matching trigger patterns.
    pub fn find_by_trigger(&self, context: &str) -> Vec<SkillSummary> {
        let context_lower = context.to_lowercase();

        self.skills
            .values()
            .filter(|s| {
                s.metadata
                    .triggers
                    .iter()
                    .any(|trigger| context_lower.contains(&trigger.to_lowercase()))
            })
            .map(SkillSummary::from)
            .collect()
    }

    /// Get total estimated tokens for all loaded skills.
    pub fn total_estimated_tokens(&self) -> u32 {
        self.skills.values().map(|s| s.estimated_tokens()).sum()
    }

    /// Read an auxiliary file from a skill's directory (Level 3 loading).
    ///
    /// # Arguments
    /// * `skill_name` - Name of the skill
    /// * `relative_path` - Path relative to the skill's directory
    ///
    /// # Returns
    /// The file contents, or an error if the skill doesn't exist or the file can't be read.
    pub fn read_auxiliary_file(&self, skill_name: &str, relative_path: &str) -> Result<String> {
        let skill = self
            .skills
            .get(skill_name)
            .ok_or_else(|| SkillsError::NotFound(skill_name.to_string()))?;

        skill
            .read_auxiliary_file(relative_path)
            .map_err(|e| SkillsError::LoadError(format!("Failed to read auxiliary file: {}", e)))
    }

    /// Execute a script from a skill's directory (Level 3 loading).
    ///
    /// # Arguments
    /// * `skill_name` - Name of the skill
    /// * `script_path` - Path to the script relative to the skill's directory
    /// * `args` - JSON arguments to pass to the script
    ///
    /// # Returns
    /// The script's stdout output, or an error if execution fails.
    pub fn execute_script(
        &self,
        skill_name: &str,
        script_path: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        let skill = self
            .skills
            .get(skill_name)
            .ok_or_else(|| SkillsError::NotFound(skill_name.to_string()))?;

        let base_dir = skill.base_dir().ok_or_else(|| {
            SkillsError::ExecutionError("Skill has no base directory".to_string())
        })?;

        // Security: prevent path traversal
        let normalized = std::path::PathBuf::from(script_path);
        if normalized
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(SkillsError::ExecutionError(
                "Path traversal not allowed".to_string(),
            ));
        }

        let script_full_path = base_dir.join(&normalized);
        if !script_full_path.exists() {
            return Err(SkillsError::NotFound(format!(
                "Script not found: {}",
                script_path
            )));
        }

        // Determine interpreter based on extension
        let extension = script_full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let interpreter = match extension {
            "sh" | "bash" => "bash",
            "py" => "python3",
            "js" => "node",
            "rb" => "ruby",
            "lua" => "lua",
            "" => {
                // Check shebang
                if let Ok(content) = std::fs::read_to_string(&script_full_path) {
                    if content.starts_with("#!") {
                        "bash" // Let the shebang handle it
                    } else {
                        return Err(SkillsError::ExecutionError(
                            "Cannot determine script interpreter".to_string(),
                        ));
                    }
                } else {
                    return Err(SkillsError::ExecutionError(
                        "Cannot read script file".to_string(),
                    ));
                }
            }
            _ => {
                return Err(SkillsError::ExecutionError(format!(
                    "Unsupported script extension: {}",
                    extension
                )))
            }
        };

        // Serialize args to pass to the script
        let args_json = serde_json::to_string(args)
            .map_err(|e| SkillsError::ExecutionError(format!("Failed to serialize args: {}", e)))?;

        // Execute the script
        let output = std::process::Command::new(interpreter)
            .arg(script_full_path)
            .arg(&args_json)
            .current_dir(base_dir)
            .output()
            .map_err(|e| SkillsError::ExecutionError(format!("Failed to execute script: {}", e)))?;

        if output.status.success() {
            String::from_utf8(output.stdout).map_err(|e| {
                SkillsError::ExecutionError(format!("Invalid UTF-8 in script output: {}", e))
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(SkillsError::ExecutionError(format!(
                "Script failed with exit code {:?}: {}",
                output.status.code(),
                stderr
            )))
        }
    }
}

/// Thread-safe async skill registry.
pub struct AsyncSkillRegistry {
    inner: Arc<RwLock<SkillRegistry>>,
    loader_config: LoaderConfig,
}

impl AsyncSkillRegistry {
    /// Create a new async registry.
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(SkillRegistry::with_config(config.clone()))),
            loader_config: config,
        }
    }

    /// Load skills asynchronously.
    pub async fn load(&self) -> Result<usize> {
        let loader = AsyncSkillLoader::new(self.loader_config.clone());
        let skills = loader.load_all().await?;
        let count = skills.len();

        let mut registry = self.inner.write().await;
        for skill in skills {
            registry.skills.insert(skill.name().to_string(), skill);
        }

        info!("Loaded {} skills into async registry", count);
        Ok(count)
    }

    /// Reload all skills asynchronously.
    pub async fn reload(&self) -> Result<usize> {
        {
            let mut registry = self.inner.write().await;
            registry.skills.clear();
        }
        self.load().await
    }

    /// Register a skill.
    pub async fn register(&self, skill: Skill) -> Result<()> {
        let mut registry = self.inner.write().await;
        registry.register(skill)
    }

    /// Get a skill by name.
    pub async fn get(&self, name: &str) -> Option<Skill> {
        let registry = self.inner.read().await;
        registry.get(name).cloned()
    }

    /// List all skill summaries.
    pub async fn list(&self) -> Vec<SkillSummary> {
        let registry = self.inner.read().await;
        registry.list()
    }

    /// Search skills.
    pub async fn search(&self, query: &str) -> Vec<SkillSummary> {
        let registry = self.inner.read().await;
        registry.search(query)
    }

    /// Get the number of loaded skills.
    pub async fn len(&self) -> usize {
        let registry = self.inner.read().await;
        registry.len()
    }

    /// Check if empty.
    pub async fn is_empty(&self) -> bool {
        let registry = self.inner.read().await;
        registry.is_empty()
    }

    /// Read an auxiliary file from a skill's directory (Level 3 loading).
    pub async fn read_auxiliary_file(
        &self,
        skill_name: &str,
        relative_path: &str,
    ) -> Result<String> {
        let registry = self.inner.read().await;
        registry.read_auxiliary_file(skill_name, relative_path)
    }

    /// Execute a script from a skill's directory (Level 3 loading).
    pub async fn execute_script(
        &self,
        skill_name: &str,
        script_path: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        // Clone necessary data while holding the lock briefly
        let (base_dir, script_full_path) = {
            let registry = self.inner.read().await;
            let skill = registry
                .skills
                .get(skill_name)
                .ok_or_else(|| SkillsError::NotFound(skill_name.to_string()))?;

            let base = skill
                .base_dir()
                .ok_or_else(|| {
                    SkillsError::ExecutionError("Skill has no base directory".to_string())
                })?
                .to_path_buf();

            // Security: prevent path traversal
            let normalized = std::path::PathBuf::from(script_path);
            if normalized
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(SkillsError::ExecutionError(
                    "Path traversal not allowed".to_string(),
                ));
            }

            let full_path = base.join(&normalized);
            (base, full_path)
        };

        if !script_full_path.exists() {
            return Err(SkillsError::NotFound(format!(
                "Script not found: {}",
                script_path
            )));
        }

        // Determine interpreter based on extension
        let extension = script_full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let interpreter = match extension {
            "sh" | "bash" => "bash",
            "py" => "python3",
            "js" => "node",
            "rb" => "ruby",
            "lua" => "lua",
            "" => {
                // Check shebang
                let content = tokio::fs::read_to_string(&script_full_path)
                    .await
                    .map_err(|e| {
                        SkillsError::ExecutionError(format!("Cannot read script file: {}", e))
                    })?;
                if content.starts_with("#!") {
                    "bash" // Let the shebang handle it
                } else {
                    return Err(SkillsError::ExecutionError(
                        "Cannot determine script interpreter".to_string(),
                    ));
                }
            }
            _ => {
                return Err(SkillsError::ExecutionError(format!(
                    "Unsupported script extension: {}",
                    extension
                )))
            }
        };

        // Serialize args to pass to the script
        let args_json = serde_json::to_string(args)
            .map_err(|e| SkillsError::ExecutionError(format!("Failed to serialize args: {}", e)))?;

        // Execute the script asynchronously
        let output = tokio::process::Command::new(interpreter)
            .arg(&script_full_path)
            .arg(&args_json)
            .current_dir(&base_dir)
            .output()
            .await
            .map_err(|e| SkillsError::ExecutionError(format!("Failed to execute script: {}", e)))?;

        if output.status.success() {
            String::from_utf8(output.stdout).map_err(|e| {
                SkillsError::ExecutionError(format!("Invalid UTF-8 in script output: {}", e))
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(SkillsError::ExecutionError(format!(
                "Script failed with exit code {:?}: {}",
                output.status.code(),
                stderr
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SkillMetadata;

    fn create_test_skill(name: &str, description: &str) -> Skill {
        let meta = SkillMetadata::new(name).with_description(description);
        Skill::new(meta, format!("Content for {}", name))
    }

    fn create_tagged_skill(name: &str, tags: Vec<&str>) -> Skill {
        let mut meta = SkillMetadata::new(name);
        for tag in tags {
            meta = meta.with_tag(tag);
        }
        Skill::new(meta, "Content")
    }

    fn create_triggered_skill(name: &str, triggers: Vec<&str>) -> Skill {
        let mut meta = SkillMetadata::new(name);
        for trigger in triggers {
            meta = meta.with_trigger(trigger);
        }
        Skill::new(meta, "Content")
    }

    #[test]
    fn test_registry_new() {
        let registry = SkillRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register() {
        let mut registry = SkillRegistry::new();
        let skill = create_test_skill("test", "Test skill");

        registry.register(skill).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("test"));
    }

    #[test]
    fn test_registry_register_duplicate() {
        let mut registry = SkillRegistry::new();
        let skill1 = create_test_skill("test", "First");
        let skill2 = create_test_skill("test", "Second");

        registry.register(skill1).unwrap();
        let result = registry.register(skill2);
        assert!(matches!(result, Err(SkillsError::AlreadyExists(_))));
    }

    #[test]
    fn test_registry_register_or_replace() {
        let mut registry = SkillRegistry::new();
        let skill1 = create_test_skill("test", "First");
        let skill2 = create_test_skill("test", "Second");

        registry.register_or_replace(skill1);
        registry.register_or_replace(skill2);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("test").unwrap().description(), "Second");
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = SkillRegistry::new();
        let skill = create_test_skill("test", "Test");

        registry.register(skill).unwrap();
        let removed = registry.unregister("test");

        assert!(removed.is_some());
        assert!(!registry.contains("test"));
    }

    #[test]
    fn test_registry_get() {
        let mut registry = SkillRegistry::new();
        let skill = create_test_skill("test", "Test skill");

        registry.register(skill).unwrap();

        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_list() {
        let mut registry = SkillRegistry::new();
        registry
            .register(create_test_skill("skill1", "First"))
            .unwrap();
        registry
            .register(create_test_skill("skill2", "Second"))
            .unwrap();

        let summaries = registry.list();
        assert_eq!(summaries.len(), 2);

        let names: Vec<_> = summaries.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"skill1"));
        assert!(names.contains(&"skill2"));
    }

    #[test]
    fn test_registry_list_by_source() {
        let mut registry = SkillRegistry::new();

        let user_skill = create_test_skill("user", "User").with_source(SkillSource::User);
        let builtin_skill =
            create_test_skill("builtin", "Builtin").with_source(SkillSource::Builtin);

        registry.register(user_skill).unwrap();
        registry.register(builtin_skill).unwrap();

        let user_skills = registry.list_by_source(&SkillSource::User);
        assert_eq!(user_skills.len(), 1);
        assert_eq!(user_skills[0].name, "user");

        let builtin_skills = registry.list_by_source(&SkillSource::Builtin);
        assert_eq!(builtin_skills.len(), 1);
        assert_eq!(builtin_skills[0].name, "builtin");
    }

    #[test]
    fn test_registry_list_by_tag() {
        let mut registry = SkillRegistry::new();

        registry
            .register(create_tagged_skill("code1", vec!["code", "rust"]))
            .unwrap();
        registry
            .register(create_tagged_skill("code2", vec!["code", "python"]))
            .unwrap();
        registry
            .register(create_tagged_skill("other", vec!["misc"]))
            .unwrap();

        let code_skills = registry.list_by_tag("code");
        assert_eq!(code_skills.len(), 2);

        let rust_skills = registry.list_by_tag("rust");
        assert_eq!(rust_skills.len(), 1);
    }

    #[test]
    fn test_registry_list_enabled() {
        let mut registry = SkillRegistry::new();

        let enabled = Skill::new(SkillMetadata::new("enabled").with_enabled(true), "Content");
        let disabled = Skill::new(
            SkillMetadata::new("disabled").with_enabled(false),
            "Content",
        );

        registry.register(enabled).unwrap();
        registry.register(disabled).unwrap();

        let enabled_list = registry.list_enabled();
        assert_eq!(enabled_list.len(), 1);
        assert_eq!(enabled_list[0].name, "enabled");
    }

    #[test]
    fn test_registry_names() {
        let mut registry = SkillRegistry::new();
        registry.register(create_test_skill("alpha", "")).unwrap();
        registry.register(create_test_skill("beta", "")).unwrap();

        let mut names = registry.names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_registry_search() {
        let mut registry = SkillRegistry::new();
        registry
            .register(create_test_skill("code-review", "Review code for issues"))
            .unwrap();
        registry
            .register(create_test_skill("testing", "Write tests"))
            .unwrap();
        registry
            .register(create_tagged_skill("debugging", vec!["code", "debug"]))
            .unwrap();

        // Search by name
        let results = registry.search("code");
        assert!(results.iter().any(|s| s.name == "code-review"));
        assert!(results.iter().any(|s| s.name == "debugging")); // has "code" tag

        // Search by description
        let results = registry.search("issues");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");

        // Search by tag
        let results = registry.search("debug");
        assert!(results.iter().any(|s| s.name == "debugging"));
    }

    #[test]
    fn test_registry_find_by_trigger() {
        let mut registry = SkillRegistry::new();

        registry
            .register(create_triggered_skill(
                "code-review",
                vec!["review code", "check code"],
            ))
            .unwrap();
        registry
            .register(create_triggered_skill(
                "testing",
                vec!["write tests", "add tests"],
            ))
            .unwrap();
        registry
            .register(create_test_skill("no-trigger", "No triggers"))
            .unwrap();

        let results = registry.find_by_trigger("Please review code for this PR");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");

        let results = registry.find_by_trigger("I need to add tests");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "testing");

        let results = registry.find_by_trigger("random context");
        assert!(results.is_empty());
    }

    #[test]
    fn test_registry_total_estimated_tokens() {
        let mut registry = SkillRegistry::new();

        // Each skill has ~25 chars content + description
        registry
            .register(create_test_skill("skill1", "Short desc"))
            .unwrap();
        registry
            .register(create_test_skill("skill2", "Another one"))
            .unwrap();

        let total = registry.total_estimated_tokens();
        assert!(total > 0);
    }

    #[tokio::test]
    async fn test_async_registry_basic() {
        let config = LoaderConfig::new();
        let registry = AsyncSkillRegistry::new(config);

        assert!(registry.is_empty().await);

        let skill = create_test_skill("async-test", "Async skill");
        registry.register(skill).await.unwrap();

        assert_eq!(registry.len().await, 1);

        let retrieved = registry.get("async-test").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "async-test");
    }

    #[tokio::test]
    async fn test_async_registry_list() {
        let config = LoaderConfig::new();
        let registry = AsyncSkillRegistry::new(config);

        registry
            .register(create_test_skill("skill1", "First"))
            .await
            .unwrap();
        registry
            .register(create_test_skill("skill2", "Second"))
            .await
            .unwrap();

        let list = registry.list().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_async_registry_search() {
        let config = LoaderConfig::new();
        let registry = AsyncSkillRegistry::new(config);

        registry
            .register(create_test_skill("code-review", "Review code"))
            .await
            .unwrap();
        registry
            .register(create_test_skill("testing", "Write tests"))
            .await
            .unwrap();

        let results = registry.search("code").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }

    // ========================================
    // Integration tests for filesystem loading
    // ========================================

    /// Helper to create a skill file in a directory.
    fn create_skill_file(dir: &std::path::Path, filename: &str, content: &str) {
        let path = dir.join(filename);
        std::fs::write(path, content).unwrap();
    }

    /// Format skill summaries the same way as builtin-wasm/agent.rs does.
    fn format_skill_summaries_for_prompt(skills: &[SkillSummary]) -> String {
        skills
            .iter()
            .map(|s| format!("- **{}**: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_registry_load_from_filesystem() {
        // Create a temporary directory with skill files
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Create skill files in the proper format (YAML frontmatter + markdown)
        create_skill_file(
            temp_dir.path(),
            "code-review.md",
            r#"---
name: code-review
description: Review code for issues
tags:
  - code
  - review
---

# Code Review

When reviewing code, follow these guidelines...
"#,
        );

        create_skill_file(
            temp_dir.path(),
            "commit.md",
            r#"---
name: commit
description: Create git commit
tags:
  - git
---

# Commit

Guidelines for creating commits...
"#,
        );

        // Configure loader to use the temp directory
        let config = LoaderConfig::new().with_user_dir(temp_dir.path());
        let mut registry = SkillRegistry::with_config(config);

        // Load skills from filesystem
        let count = registry.load().unwrap();
        assert_eq!(count, 2, "Should load 2 skills from filesystem");

        // Verify list() returns correct summaries
        let summaries = registry.list();
        assert_eq!(summaries.len(), 2);

        let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"commit"));

        // Verify descriptions are correct
        let code_review = summaries.iter().find(|s| s.name == "code-review").unwrap();
        assert_eq!(code_review.description, "Review code for issues");

        let commit = summaries.iter().find(|s| s.name == "commit").unwrap();
        assert_eq!(commit.description, "Create git commit");
    }

    #[test]
    fn test_skills_format_for_prompt_injection() {
        // Create a temporary directory with skill files
        let temp_dir = tempfile::TempDir::new().unwrap();

        create_skill_file(
            temp_dir.path(),
            "code-review.md",
            r#"---
name: code-review
description: Review code for issues
---

Content here.
"#,
        );

        create_skill_file(
            temp_dir.path(),
            "commit.md",
            r#"---
name: commit
description: Create git commit
---

Content here.
"#,
        );

        // Load skills
        let config = LoaderConfig::new().with_user_dir(temp_dir.path());
        let mut registry = SkillRegistry::with_config(config);
        registry.load().unwrap();

        // Get summaries and format for prompt
        let mut summaries = registry.list();
        // Sort for deterministic output
        summaries.sort_by(|a, b| a.name.cmp(&b.name));

        let formatted = format_skill_summaries_for_prompt(&summaries);

        // Verify the format matches what's expected in the system prompt
        assert!(
            formatted.contains("- **code-review**: Review code for issues"),
            "Should contain code-review skill in correct format. Got: {}",
            formatted
        );
        assert!(
            formatted.contains("- **commit**: Create git commit"),
            "Should contain commit skill in correct format. Got: {}",
            formatted
        );

        // Verify the full format
        let expected = "- **code-review**: Review code for issues\n- **commit**: Create git commit";
        assert_eq!(formatted, expected);

        // Simulate how it would appear in the system prompt
        let base_prompt = "You are a helpful AI assistant.";
        let full_prompt = format!(
            "{}\n\n## Available Skills\n\n{}\n\nUse skill_load(name) to load a skill's full content.",
            base_prompt, formatted
        );

        assert!(full_prompt.contains("## Available Skills"));
        assert!(full_prompt.contains("**code-review**"));
        assert!(full_prompt.contains("**commit**"));
        assert!(full_prompt.contains("Use skill_load(name)"));
    }

    #[test]
    fn test_empty_skills_directory() {
        // Create an empty temporary directory
        let temp_dir = tempfile::TempDir::new().unwrap();

        let config = LoaderConfig::new().with_user_dir(temp_dir.path());
        let mut registry = SkillRegistry::with_config(config);

        // Load from empty directory
        let count = registry.load().unwrap();
        assert_eq!(count, 0, "Should load 0 skills from empty directory");

        let summaries = registry.list();
        assert!(summaries.is_empty());

        // Format should be empty
        let formatted = format_skill_summaries_for_prompt(&summaries);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_nonexistent_skills_directory() {
        // Use a path that doesn't exist
        let config = LoaderConfig::new().with_user_dir("/nonexistent/skills/path");
        let mut registry = SkillRegistry::with_config(config);

        // Load should succeed but return 0 skills
        let count = registry.load().unwrap();
        assert_eq!(count, 0);

        let summaries = registry.list();
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_default_user_dirs_skills_loading() {
        // This test checks if skills can be loaded from the actual default directories
        // It may or may not find skills depending on the system state
        let config = LoaderConfig::default();
        let mut registry = SkillRegistry::with_config(config);

        // This should not panic regardless of whether skills exist
        let result = registry.load();
        assert!(result.is_ok(), "Loading from default dirs should not error");

        let count = result.unwrap();
        let summaries = registry.list();
        assert_eq!(summaries.len(), count);

        // If skills were found, verify they have valid names and descriptions
        for summary in &summaries {
            assert!(!summary.name.is_empty(), "Skill name should not be empty");
            // Description can be empty but name cannot
        }

        // Print what we found (for debugging)
        if count > 0 {
            println!("Found {} skills in default directories:", count);
            for s in &summaries {
                println!("  - {}: {}", s.name, s.description);
            }
        } else {
            println!("No skills found in default directories (this is expected if no .md files exist directly in skills dirs)");
        }
    }
}
