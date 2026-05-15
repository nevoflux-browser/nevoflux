//! Site adaptation data model.

use serde::{Deserialize, Serialize};

/// A site adaptation record tracking domain-specific behaviors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteAdaptation {
    /// Unique identifier (SA-{hash}).
    pub id: String,
    /// The domain this adaptation applies to.
    pub domain: String,
    /// Optional URL pattern for more specific matching.
    pub url_pattern: Option<String>,
    /// Type of adaptation: selector_result, spa_behavior, api_pattern, anti_bot, automation_outcome.
    pub adaptation_type: String,
    /// JSON content of the adaptation.
    pub content: String,
    /// Whether this adaptation has been verified.
    pub verified: bool,
    /// When the adaptation was last verified (RFC 3339).
    pub last_verified_at: Option<String>,
    /// Success rate of this adaptation (0.0 to 1.0).
    pub success_rate: f64,
    /// Number of times this adaptation has been sampled.
    pub sample_count: i64,
    /// When the record was created (RFC 3339).
    pub created_at: String,
    /// When the record was last updated (RFC 3339).
    pub updated_at: String,
}

/// Parameters for creating a new site adaptation.
#[derive(Debug, Clone)]
pub struct CreateSiteAdaptationParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// The domain this adaptation applies to.
    pub domain: String,
    /// Optional URL pattern for more specific matching.
    pub url_pattern: Option<String>,
    /// Type of adaptation.
    pub adaptation_type: String,
    /// JSON content of the adaptation.
    pub content: String,
    /// Whether this adaptation has been verified.
    pub verified: bool,
}

impl CreateSiteAdaptationParams {
    /// Create new params with required fields.
    pub fn new(domain: &str, adaptation_type: &str, content: &str) -> Self {
        Self {
            id: None,
            domain: domain.to_string(),
            url_pattern: None,
            adaptation_type: adaptation_type.to_string(),
            content: content.to_string(),
            verified: false,
        }
    }

    /// Set a custom ID.
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set the URL pattern.
    pub fn with_url_pattern(mut self, pattern: &str) -> Self {
        self.url_pattern = Some(pattern.to_string());
        self
    }

    /// Set the verified flag.
    pub fn with_verified(mut self, verified: bool) -> Self {
        self.verified = verified;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_site_adaptation_params() {
        let params = CreateSiteAdaptationParams::new(
            "example.com",
            "selector_result",
            r#"{"selector": ".main-content"}"#,
        );

        assert_eq!(params.domain, "example.com");
        assert_eq!(params.adaptation_type, "selector_result");
        assert!(params.id.is_none());
        assert!(params.url_pattern.is_none());
        assert!(!params.verified);
    }

    #[test]
    fn test_create_site_adaptation_params_builder() {
        let params = CreateSiteAdaptationParams::new(
            "example.com",
            "spa_behavior",
            r#"{"wait_for": ".loaded"}"#,
        )
        .with_id("SA-abc123")
        .with_url_pattern("/products/*")
        .with_verified(true);

        assert_eq!(params.id, Some("SA-abc123".to_string()));
        assert_eq!(params.url_pattern, Some("/products/*".to_string()));
        assert!(params.verified);
    }
}
