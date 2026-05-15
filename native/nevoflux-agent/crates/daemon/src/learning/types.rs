use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Category of a learning entry, indicating what domain the learning applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningCategory {
    /// Learnings about interacting with websites (selectors, navigation, etc.)
    SiteInteraction,
    /// Learnings about optimizing tool usage (timeouts, parameters, etc.)
    ToolOptimization,
    /// Learnings about user preferences (language, style, etc.)
    UserPreference,
}

/// Status of a learning entry in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Newly created, not yet validated.
    Pending,
    /// Validated by repeated observations or explicit confirmation.
    Validated,
    /// Promoted into a document (identity, soul, etc.).
    Promoted,
    /// No longer active, kept for history.
    Archived,
}

/// Priority level for a learning entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Privacy level controlling how a learning entry may be shared or stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    /// Can be shared freely.
    Public,
    /// Kept within the agent system.
    Internal,
    /// Contains potentially sensitive information.
    Sensitive,
    /// Strictly private, never shared.
    Private,
}

/// Target document where a promoted learning entry should be written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentTarget {
    /// Agent identity document.
    Identity,
    /// Agent soul/personality document.
    Soul,
    /// User-specific document.
    User,
    /// Tools configuration/knowledge document.
    Tools,
    /// Sub-agents configuration document.
    Agents,
}

/// Contextual information about where/when a learning was observed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearningContext {
    /// URL where the learning was observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Domain extracted from the URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// CSS selector relevant to the learning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Name of the tool involved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Session ID where the learning was observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// A single learning entry representing an observation, pattern, or preference
/// that the agent has identified during operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEntry {
    /// Unique identifier in format `LE-{YYYYMMDDHHmmSS}-{6chars}`.
    pub id: String,
    /// Category of this learning.
    pub category: LearningCategory,
    /// Optional subcategory for finer classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    /// The event that triggered this learning (e.g., "click_failed").
    pub source_event: String,
    /// Human-readable summary of what was learned.
    pub summary: String,
    /// Additional details or structured information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Contextual information about where the learning was observed.
    pub context: LearningContext,
    /// Current lifecycle status.
    pub status: EntryStatus,
    /// Priority level.
    pub priority: Priority,
    /// Privacy level controlling sharing/storage.
    pub privacy_level: PrivacyLevel,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
    /// Number of times this pattern has been observed.
    pub occurrence_count: u32,
    /// Target document for promotion (set when promoting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_target: Option<DocumentTarget>,
    /// Timestamp when this entry was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when this entry was last seen/hit.
    pub last_seen_at: DateTime<Utc>,
}

impl LearningEntry {
    /// Create a new learning entry with default values.
    ///
    /// The entry starts with `Pending` status, `Medium` priority,
    /// `Internal` privacy level, and a confidence of `0.5`.
    pub fn new(
        category: LearningCategory,
        source_event: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let timestamp = now.format("%Y%m%d%H%M%S").to_string();
        let uuid_suffix = &Uuid::new_v4().to_string()[..6];
        let id = format!("LE-{}-{}", timestamp, uuid_suffix);

        Self {
            id,
            category,
            subcategory: None,
            source_event: source_event.into(),
            summary: summary.into(),
            details: None,
            context: LearningContext::default(),
            status: EntryStatus::Pending,
            priority: Priority::Medium,
            privacy_level: PrivacyLevel::Internal,
            confidence: 0.5,
            occurrence_count: 1,
            promotion_target: None,
            created_at: now,
            last_seen_at: now,
        }
    }

    /// Set the context for this learning entry.
    pub fn with_context(mut self, context: LearningContext) -> Self {
        self.context = context;
        self
    }

    /// Set the priority level.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the privacy level.
    pub fn with_privacy(mut self, privacy_level: PrivacyLevel) -> Self {
        self.privacy_level = privacy_level;
        self
    }

    /// Set a subcategory for finer classification.
    pub fn with_subcategory(mut self, subcategory: impl Into<String>) -> Self {
        self.subcategory = Some(subcategory.into());
        self
    }

    /// Set additional details.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Set the target document for promotion.
    pub fn with_promotion_target(mut self, target: DocumentTarget) -> Self {
        self.promotion_target = Some(target);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_entry_creation() {
        let entry = LearningEntry::new(
            LearningCategory::SiteInteraction,
            "click_failed",
            "Click on .btn-submit failed: element not found",
        );
        assert_eq!(entry.category, LearningCategory::SiteInteraction);
        assert_eq!(entry.source_event, "click_failed");
        assert!(entry.id.starts_with("LE-"));
        assert_eq!(entry.status, EntryStatus::Pending);
        assert_eq!(entry.occurrence_count, 1);
        assert!(entry.confidence > 0.0);
    }

    #[test]
    fn learning_entry_serialization_roundtrip() {
        let entry = LearningEntry::new(
            LearningCategory::ToolOptimization,
            "tool_timeout",
            "web_fetch timed out after 5000ms",
        );
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: LearningEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.id, deserialized.id);
        assert_eq!(entry.category, deserialized.category);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn privacy_level_defaults() {
        let entry = LearningEntry::new(
            LearningCategory::UserPreference,
            "language_preference",
            "User prefers Chinese",
        );
        assert_eq!(entry.privacy_level, PrivacyLevel::Internal);
    }
}
