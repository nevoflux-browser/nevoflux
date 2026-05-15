//! Error types for the skills crate.

use thiserror::Error;

/// Skills error type.
#[derive(Debug, Error)]
pub enum SkillsError {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing error.
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Skill not found.
    #[error("Skill not found: {0}")]
    NotFound(String),

    /// Invalid skill file.
    #[error("Invalid skill file: {0}")]
    InvalidSkillFile(String),

    /// Missing frontmatter.
    #[error("Missing frontmatter in skill file: {0}")]
    MissingFrontmatter(String),

    /// Invalid frontmatter.
    #[error("Invalid frontmatter: {0}")]
    InvalidFrontmatter(String),

    /// Skill already exists.
    #[error("Skill already exists: {0}")]
    AlreadyExists(String),

    /// Script execution error.
    #[error("Script execution error: {0}")]
    ScriptError(String),

    /// Execution error (for skill script execution).
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// Load error (for auxiliary file loading).
    #[error("Load error: {0}")]
    LoadError(String),

    /// Glob pattern error.
    #[error("Glob pattern error: {0}")]
    GlobError(String),
}

/// Result type for skills operations.
pub type Result<T> = std::result::Result<T, SkillsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SkillsError::NotFound("test-skill".into());
        assert_eq!(err.to_string(), "Skill not found: test-skill");
    }

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: SkillsError = io_err.into();
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_invalid_skill_file() {
        let err = SkillsError::InvalidSkillFile("missing content".into());
        assert!(err.to_string().contains("missing content"));
    }

    #[test]
    fn test_error_missing_frontmatter() {
        let err = SkillsError::MissingFrontmatter("skill.md".into());
        assert!(err.to_string().contains("skill.md"));
    }

    #[test]
    fn test_error_already_exists() {
        let err = SkillsError::AlreadyExists("my-skill".into());
        assert!(err.to_string().contains("my-skill"));
    }

    #[test]
    fn test_error_script_error() {
        let err = SkillsError::ScriptError("command not found".into());
        assert!(err.to_string().contains("command not found"));
    }

    #[test]
    fn test_error_glob() {
        let err = SkillsError::GlobError("invalid pattern".into());
        assert!(err.to_string().contains("invalid pattern"));
    }
}
