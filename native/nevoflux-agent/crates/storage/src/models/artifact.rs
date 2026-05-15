//! Artifact data model.

use std::collections::HashMap;

/// A persisted artifact record.
#[derive(Debug, Clone)]
pub struct ArtifactRecord {
    pub id: String,
    /// Nullable after migration 014: persistent artifacts survive session deletion (FK SET NULL).
    pub session_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub content_type: String,
    pub content: String,
    pub files: Option<HashMap<String, String>>,
    pub entry: Option<String>,
    pub created_at: i64,
    pub imported_from_url: Option<String>,
    pub imported_from_share_id: Option<String>,
    pub imported_at: Option<i64>,
    /// Whether this artifact has been pinned to "My Canvas".
    pub is_persistent: bool,
    /// Unix timestamp (seconds) when the artifact was pinned; None if not pinned.
    pub persisted_at: Option<i64>,
    /// Unix timestamp (seconds) of the last content update.
    pub updated_at: i64,
}

/// Parameters for creating/upserting an artifact.
pub struct CreateArtifactParams {
    pub id: String,
    /// Nullable: None when the artifact is not associated with a live session.
    pub session_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub content_type: String,
    pub content: String,
    pub files: Option<HashMap<String, String>>,
    pub entry: Option<String>,
}

impl CreateArtifactParams {
    /// Create new params with a session-bound artifact.
    ///
    /// Use [`CreateArtifactParams::new_orphan`] when the artifact has no session.
    pub fn new(id: &str, session_id: &str, title: &str, content_type: &str) -> Self {
        Self {
            id: id.to_string(),
            session_id: Some(session_id.to_string()),
            title: title.to_string(),
            description: None,
            content_type: content_type.to_string(),
            content: String::new(),
            files: None,
            entry: None,
        }
    }

    /// Create params for an artifact with no session association (e.g. imported persistent artifact).
    pub fn new_orphan(id: &str, title: &str, content_type: &str) -> Self {
        Self {
            id: id.to_string(),
            session_id: None,
            title: title.to_string(),
            description: None,
            content_type: content_type.to_string(),
            content: String::new(),
            files: None,
            entry: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Set the content.
    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    /// Set the files map.
    pub fn with_files(mut self, files: HashMap<String, String>) -> Self {
        self.files = Some(files);
        self
    }

    /// Set the entry point.
    pub fn with_entry(mut self, entry: &str) -> Self {
        self.entry = Some(entry.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_artifact_params_builder() {
        let params = CreateArtifactParams::new("art-1", "sess-1", "My Page", "text/html")
            .with_description("A simple HTML page")
            .with_content("<h1>Hello</h1>");

        assert_eq!(params.id, "art-1");
        assert_eq!(params.session_id, Some("sess-1".to_string()));
        assert_eq!(params.title, "My Page");
        assert_eq!(params.description, Some("A simple HTML page".to_string()));
        assert_eq!(params.content_type, "text/html");
        assert_eq!(params.content, "<h1>Hello</h1>");
        assert!(params.files.is_none());
        assert!(params.entry.is_none());
    }

    #[test]
    fn test_create_artifact_params_with_files() {
        let mut files = HashMap::new();
        files.insert("src/App.jsx".to_string(), "export default App;".to_string());

        let params = CreateArtifactParams::new("art-2", "sess-1", "React App", "project")
            .with_files(files)
            .with_entry("src/App.jsx");

        assert!(params.files.is_some());
        assert_eq!(params.entry, Some("src/App.jsx".to_string()));
    }

    #[test]
    fn test_create_artifact_params_orphan() {
        let params = CreateArtifactParams::new_orphan("art-3", "Orphan", "text/html");
        assert_eq!(params.session_id, None);
    }
}
