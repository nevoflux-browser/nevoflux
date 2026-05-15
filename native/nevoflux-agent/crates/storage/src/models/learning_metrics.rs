//! Learning metrics data model.

use serde::{Deserialize, Serialize};

/// A learning system effectiveness metric record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningMetric {
    /// Unique identifier (LM-{hash}).
    pub id: String,
    /// Type of metric: success_rate, retry_rate, knowledge_hit, promotion_rate.
    pub metric_type: String,
    /// Associated domain (None = global).
    pub domain: Option<String>,
    /// Period for aggregation (YYYY-MM-DD).
    pub period: String,
    /// The metric value.
    pub value: f64,
    /// Number of samples in this aggregation.
    pub sample_count: i64,
    /// When the record was created (RFC 3339).
    pub created_at: String,
}

/// Parameters for creating a new learning metric record.
#[derive(Debug, Clone)]
pub struct CreateLearningMetricParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// Type of metric.
    pub metric_type: String,
    /// Associated domain (None = global).
    pub domain: Option<String>,
    /// Period for aggregation (YYYY-MM-DD).
    pub period: String,
    /// The metric value.
    pub value: f64,
    /// Number of samples.
    pub sample_count: i64,
}

impl CreateLearningMetricParams {
    /// Create new params with required fields.
    pub fn new(metric_type: &str, period: &str, value: f64) -> Self {
        Self {
            id: None,
            metric_type: metric_type.to_string(),
            domain: None,
            period: period.to_string(),
            value,
            sample_count: 0,
        }
    }

    /// Set a custom ID.
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set the domain.
    pub fn with_domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }

    /// Set the sample count.
    pub fn with_sample_count(mut self, count: i64) -> Self {
        self.sample_count = count;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_learning_metric_params() {
        let params = CreateLearningMetricParams::new("success_rate", "2026-02-17", 0.85);

        assert_eq!(params.metric_type, "success_rate");
        assert_eq!(params.period, "2026-02-17");
        assert!((params.value - 0.85).abs() < f64::EPSILON);
        assert!(params.id.is_none());
        assert!(params.domain.is_none());
        assert_eq!(params.sample_count, 0);
    }

    #[test]
    fn test_create_learning_metric_params_builder() {
        let params = CreateLearningMetricParams::new("retry_rate", "2026-02-17", 0.12)
            .with_id("LM-abc123")
            .with_domain("example.com")
            .with_sample_count(50);

        assert_eq!(params.id, Some("LM-abc123".to_string()));
        assert_eq!(params.domain, Some("example.com".to_string()));
        assert_eq!(params.sample_count, 50);
    }
}
