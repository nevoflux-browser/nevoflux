//! Knowledge model and related types.

use serde::{Deserialize, Serialize};

/// A knowledge entry representing learned information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Knowledge {
    /// Unique identifier (K-{YYYYMMDD}-{6hex}).
    pub id: String,
    /// Category of knowledge (e.g., site_interaction, tool_optimization).
    pub category: String,
    /// Optional subcategory for finer classification.
    pub subcategory: Option<String>,
    /// Associated domain (None = universal).
    pub domain: Option<String>,
    /// Brief summary of the knowledge.
    pub summary: String,
    /// Detailed description.
    pub details: String,
    /// Optional resolution or fix.
    pub resolution: Option<String>,
    /// Confidence score (0.0-1.0, default 0.5).
    pub confidence: f64,
    /// Number of times this knowledge was accessed.
    pub hit_count: i64,
    /// Number of successful applications.
    pub success_count: i64,
    /// Number of failed applications.
    pub fail_count: i64,
    /// Computed effectiveness (success_count / (success_count + fail_count)).
    pub effectiveness: f64,
    /// Priority level (low, medium, high).
    pub priority: String,
    /// Status (pending, validated, promoted, archived).
    pub status: String,
    /// Source LearningEntry IDs (JSON array).
    pub source_ids: Option<String>,
    /// Related knowledge IDs (JSON array).
    pub related_ids: Option<String>,
    /// Tags (JSON array).
    pub tags: Option<String>,
    /// Privacy level (internal, public).
    pub privacy_level: String,
    /// Promotion target (IDENTITY, SOUL, USER, TOOLS, AGENTS).
    pub promotion_target: Option<String>,
    /// Target section in the promoted file.
    pub promoted_section: Option<String>,
    /// Source type (system, manual).
    pub source_type: String,
    /// RFC3339 timestamp when created.
    pub created_at: String,
    /// RFC3339 timestamp when last updated.
    pub updated_at: String,
    /// RFC3339 timestamp when last accessed.
    pub last_hit_at: Option<String>,
    /// RFC3339 timestamp when promoted.
    pub promoted_at: Option<String>,
    /// Optional embedding vector for semantic search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Whether this entry is "hot" (included in system prompt Layer 1).
    #[serde(default)]
    pub hot: bool,
    /// One-line summary used when injecting hot knowledge into system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_summary: Option<String>,
}

/// Parameters for creating a new knowledge entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateKnowledgeParams {
    /// Category of knowledge (required).
    pub category: String,
    /// Optional subcategory.
    pub subcategory: Option<String>,
    /// Associated domain (None = universal).
    pub domain: Option<String>,
    /// Brief summary (required).
    pub summary: String,
    /// Detailed description (required).
    pub details: String,
    /// Optional resolution or fix.
    pub resolution: Option<String>,
    /// Priority level (defaults to "medium").
    pub priority: Option<String>,
    /// Source LearningEntry IDs (JSON array).
    pub source_ids: Option<String>,
    /// Tags (JSON array).
    pub tags: Option<String>,
    /// Privacy level (defaults to "internal").
    pub privacy_level: Option<String>,
    /// Promotion target.
    pub promotion_target: Option<String>,
    /// Source type (defaults to "system").
    pub source_type: Option<String>,
    /// Optional embedding vector for semantic search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}
