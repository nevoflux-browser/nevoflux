//! Skills integration for the daemon.

use crate::error::{DaemonError, Result};
use nevoflux_skills::{LoaderConfig, Skill, SkillRegistry, SkillSummary};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Skills manager for the daemon.
pub struct SkillsManager {
    registry: Arc<RwLock<SkillRegistry>>,
    config: LoaderConfig,
}

impl SkillsManager {
    /// Create a new skills manager.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(SkillRegistry::new())),
            config: LoaderConfig::new(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: LoaderConfig) -> Self {
        Self {
            registry: Arc::new(RwLock::new(SkillRegistry::with_config(config.clone()))),
            config,
        }
    }

    /// Load skills from configured directories.
    pub async fn load(&self) -> Result<usize> {
        let mut registry = self.registry.write().await;
        registry
            .load()
            .map_err(|e| DaemonError::InternalError(format!("Failed to load skills: {}", e)))?;
        Ok(registry.len())
    }

    /// List all available skills (Level 1).
    pub async fn list(&self) -> Vec<SkillSummary> {
        let registry = self.registry.read().await;
        registry.list()
    }

    /// Get a skill by name (Level 2).
    pub async fn get(&self, name: &str) -> Option<Skill> {
        let registry = self.registry.read().await;
        registry.get(name).cloned()
    }

    /// Get the number of loaded skills.
    pub async fn len(&self) -> usize {
        let registry = self.registry.read().await;
        registry.len()
    }

    /// Check if no skills are loaded.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Get the registry (for advanced use).
    pub fn registry(&self) -> Arc<RwLock<SkillRegistry>> {
        self.registry.clone()
    }

    /// Get the loader configuration.
    pub fn config(&self) -> &LoaderConfig {
        &self.config
    }
}

impl Default for SkillsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_skills_manager_creation() {
        let manager = SkillsManager::new();
        assert!(manager.is_empty().await);
    }

    #[tokio::test]
    async fn test_skills_manager_list_empty() {
        let manager = SkillsManager::new();
        let skills = manager.list().await;
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_skills_manager_get_not_found() {
        let manager = SkillsManager::new();
        let skill = manager.get("nonexistent").await;
        assert!(skill.is_none());
    }
}
