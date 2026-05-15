//! Core types for the EventBus system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// A single event on the bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEvent {
    pub id: String,
    pub topic: String,
    pub payload: serde_json::Value,
    pub publisher: PublisherIdentity,
    pub delivery: Delivery,
    pub created_at: DateTime<Utc>,
    pub ttl: Option<Duration>,
}

impl BusEvent {
    pub fn ephemeral(
        topic: impl Into<String>,
        payload: serde_json::Value,
        publisher: PublisherIdentity,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            topic: topic.into(),
            payload,
            publisher,
            delivery: Delivery::Ephemeral,
            created_at: Utc::now(),
            ttl: None,
        }
    }

    pub fn sticky(
        topic: impl Into<String>,
        payload: serde_json::Value,
        publisher: PublisherIdentity,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            topic: topic.into(),
            payload,
            publisher,
            delivery: Delivery::Sticky,
            created_at: Utc::now(),
            ttl: None,
        }
    }

    pub fn persistent(
        topic: impl Into<String>,
        payload: serde_json::Value,
        publisher: PublisherIdentity,
        ttl: Option<Duration>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            topic: topic.into(),
            payload,
            publisher,
            delivery: Delivery::Persistent,
            created_at: Utc::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = Utc::now()
                .signed_duration_since(self.created_at)
                .to_std()
                .unwrap_or(Duration::ZERO);
            elapsed > ttl
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Ephemeral,
    Sticky,
    Persistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublisherIdentity {
    Internal,
    Agent { session_id: String },
    Extension { proxy_id: String },
    Wasm { plugin_id: String },
    Mcp { server_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubscriberIdentity {
    Internal,
    Agent { session_id: String },
    Extension { proxy_id: String },
    Wasm { plugin_id: String },
    Mcp { server_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TopicPattern {
    Exact(String),
    Wildcard(String),
    DoubleWildcard(String),
}

impl TopicPattern {
    pub fn exact(topic: impl Into<String>) -> Self {
        Self::Exact(topic.into())
    }

    pub fn wildcard(pattern: impl Into<String>) -> Self {
        Self::Wildcard(pattern.into())
    }

    pub fn double_wildcard(prefix: impl Into<String>) -> Self {
        Self::DoubleWildcard(prefix.into())
    }

    pub fn matches(&self, topic: &str) -> bool {
        match self {
            TopicPattern::Exact(pat) => pat == topic,
            TopicPattern::Wildcard(pat) => {
                let pat_segments: Vec<&str> = pat.split(':').collect();
                let topic_segments: Vec<&str> = topic.split(':').collect();
                if pat_segments.len() != topic_segments.len() {
                    return false;
                }
                pat_segments
                    .iter()
                    .zip(topic_segments.iter())
                    .all(|(p, t)| *p == "*" || p == t)
            }
            TopicPattern::DoubleWildcard(prefix) => {
                if prefix.is_empty() {
                    true
                } else {
                    topic == prefix || topic.starts_with(&format!("{}:", prefix))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressurePolicy {
    #[default]
    DropOldest,
    DropNewest,
    Block,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub pattern: TopicPattern,
    pub subscriber: SubscriberIdentity,
    pub policy: BackpressurePolicy,
    pub buffer_size: usize,
}

impl Subscription {
    pub fn new(pattern: TopicPattern, subscriber: SubscriberIdentity) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            pattern,
            subscriber,
            policy: BackpressurePolicy::default(),
            buffer_size: 256,
        }
    }

    pub fn with_policy(mut self, policy: BackpressurePolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_event_ephemeral_has_uuid_id() {
        let evt = BusEvent::ephemeral(
            "task:status",
            serde_json::json!({"status": "done"}),
            PublisherIdentity::Internal,
        );
        assert!(!evt.id.is_empty());
        assert!(Uuid::parse_str(&evt.id).is_ok());
        assert_eq!(evt.delivery, Delivery::Ephemeral);
        assert!(evt.ttl.is_none());
    }

    #[test]
    fn test_bus_event_persistent_with_ttl() {
        let evt = BusEvent::persistent(
            "log:error",
            serde_json::json!({"msg": "oops"}),
            PublisherIdentity::Agent {
                session_id: "s1".into(),
            },
            Some(Duration::from_secs(3600)),
        );
        assert_eq!(evt.delivery, Delivery::Persistent);
        assert_eq!(evt.ttl, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_bus_event_is_expired() {
        let mut evt = BusEvent::ephemeral(
            "test:topic",
            serde_json::json!(null),
            PublisherIdentity::Internal,
        );
        assert!(!evt.is_expired());
        evt.ttl = Some(Duration::ZERO);
        assert!(evt.is_expired());
    }

    #[test]
    fn test_delivery_serde_roundtrip() {
        let json = serde_json::to_string(&Delivery::Sticky).unwrap();
        assert_eq!(json, "\"sticky\"");
        let decoded: Delivery = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, Delivery::Sticky);
    }

    #[test]
    fn test_publisher_identity_serde_roundtrip() {
        let publisher = PublisherIdentity::Agent {
            session_id: "sess-001".into(),
        };
        let json = serde_json::to_string(&publisher).unwrap();
        assert!(json.contains("\"kind\":\"agent\""));
        assert!(json.contains("\"session_id\":\"sess-001\""));
        let decoded: PublisherIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, publisher);
    }

    #[test]
    fn test_subscriber_identity_hash_and_eq() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let sub1 = SubscriberIdentity::Agent {
            session_id: "s1".into(),
        };
        let sub2 = SubscriberIdentity::Agent {
            session_id: "s1".into(),
        };
        let sub3 = SubscriberIdentity::Extension {
            proxy_id: "p1".into(),
        };
        set.insert(sub1.clone());
        set.insert(sub2);
        set.insert(sub3);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_subscription_builder_defaults() {
        let sub = Subscription::new(
            TopicPattern::exact("task:status"),
            SubscriberIdentity::Internal,
        );
        assert_eq!(sub.policy, BackpressurePolicy::DropOldest);
        assert_eq!(sub.buffer_size, 256);
        assert!(!sub.id.is_empty());
    }

    #[test]
    fn test_subscription_builder_custom() {
        let sub = Subscription::new(
            TopicPattern::wildcard("task:*"),
            SubscriberIdentity::Agent {
                session_id: "s1".into(),
            },
        )
        .with_policy(BackpressurePolicy::Block)
        .with_buffer_size(1024);
        assert_eq!(sub.policy, BackpressurePolicy::Block);
        assert_eq!(sub.buffer_size, 1024);
    }

    #[test]
    fn test_backpressure_policy_default() {
        let policy = BackpressurePolicy::default();
        assert_eq!(policy, BackpressurePolicy::DropOldest);
    }
}
