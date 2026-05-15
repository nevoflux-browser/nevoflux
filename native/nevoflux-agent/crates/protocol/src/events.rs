//! EventBus protocol message types.
//!
//! These types define the wire format for EventBus messages between
//! the extension frontend and the daemon. They travel inside the Chat
//! channel as SidebarMessage::EventsRequest and AgentMessage::EventsResponse/
//! EventsDelivery variants.

use serde::{Deserialize, Serialize};

/// Delivery mode for an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    Ephemeral,
    Sticky,
    Persistent {
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl_secs: Option<u64>,
    },
}

/// Options for subscribing to events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeOptions {
    pub patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub replay_sticky: bool,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

fn default_true() -> bool {
    true
}
fn default_buffer_size() -> usize {
    256
}

/// Options for publishing an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishOptions {
    pub topic: String,
    pub payload: serde_json::Value,
    pub delivery: DeliveryMode,
}

/// Options for querying event history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub topic: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
}

fn default_limit() -> usize {
    100
}

/// A single event delivered to a subscriber or returned from history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusEventPayload {
    pub event_id: String,
    pub topic: String,
    pub payload: serde_json::Value,
    pub delivery: DeliveryMode,
    pub publisher: String,
    pub timestamp_ms: u64,
}

/// Sidebar -> Daemon EventBus request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EventBusRequest {
    Subscribe(SubscribeOptions),
    Unsubscribe { subscription_id: String },
    Publish(PublishOptions),
    History(HistoryQuery),
}

/// Daemon -> Sidebar EventBus response (one-shot replies).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum EventBusResponse {
    Subscribed {
        subscription_id: String,
        patterns: Vec<String>,
    },
    Unsubscribed {
        subscription_id: String,
    },
    Published {
        event_id: String,
    },
    HistoryResult {
        topic: String,
        events: Vec<BusEventPayload>,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Daemon -> Sidebar EventBus push delivery (async, not request-response).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBusDelivery {
    pub subscription_id: String,
    pub event: BusEventPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_request_json_roundtrip() {
        let req = EventBusRequest::Subscribe(SubscribeOptions {
            patterns: vec!["session:*:notification".into()],
            replay_sticky: true,
            buffer_size: 256,
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"action\":\"subscribe\""));
        let decoded: EventBusRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_publish_request_json_roundtrip() {
        let req = EventBusRequest::Publish(PublishOptions {
            topic: "task:progress".into(),
            payload: serde_json::json!({"percent": 42}),
            delivery: DeliveryMode::Persistent {
                ttl_secs: Some(3600),
            },
        });
        let json = serde_json::to_string(&req).unwrap();
        let decoded: EventBusRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_unsubscribe_request_json() {
        let req = EventBusRequest::Unsubscribe {
            subscription_id: "sub-001".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"subscription_id\":\"sub-001\""));
    }

    #[test]
    fn test_history_request_defaults() {
        let json = r#"{"action":"history","topic":"task:progress"}"#;
        let req: EventBusRequest = serde_json::from_str(json).unwrap();
        match req {
            EventBusRequest::History(q) => {
                assert_eq!(q.limit, 100);
                assert_eq!(q.since_ms, None);
            }
            _ => panic!("Expected History variant"),
        }
    }

    #[test]
    fn test_response_subscribed_json() {
        let resp = EventBusResponse::Subscribed {
            subscription_id: "sub-001".into(),
            patterns: vec!["session:*:notification".into()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":\"subscribed\""));
        let decoded: EventBusResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_response_error_json() {
        let resp = EventBusResponse::Error {
            code: "PERMISSION_DENIED".into(),
            message: "Cannot subscribe to agent:** topics".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("PERMISSION_DENIED"));
    }

    #[test]
    fn test_delivery_json_roundtrip() {
        let delivery = EventBusDelivery {
            subscription_id: "sub-001".into(),
            event: BusEventPayload {
                event_id: "evt-001".into(),
                topic: "session:abc:notification".into(),
                payload: serde_json::json!({"title": "Task complete"}),
                delivery: DeliveryMode::Ephemeral,
                publisher: "agent:planner".into(),
                timestamp_ms: 1700000000000,
            },
        };
        let json = serde_json::to_string(&delivery).unwrap();
        let decoded: EventBusDelivery = serde_json::from_str(&json).unwrap();
        assert_eq!(delivery, decoded);
    }

    #[test]
    fn test_delivery_mode_variants() {
        let ephemeral: DeliveryMode = serde_json::from_str(r#""ephemeral""#).unwrap();
        assert_eq!(ephemeral, DeliveryMode::Ephemeral);

        let sticky: DeliveryMode = serde_json::from_str(r#""sticky""#).unwrap();
        assert_eq!(sticky, DeliveryMode::Sticky);

        let persistent = DeliveryMode::Persistent {
            ttl_secs: Some(3600),
        };
        let json = serde_json::to_string(&persistent).unwrap();
        let decoded: DeliveryMode = serde_json::from_str(&json).unwrap();
        assert_eq!(persistent, decoded);
    }

    #[test]
    fn test_bus_event_payload_json() {
        let event = BusEventPayload {
            event_id: "evt-abc".into(),
            topic: "task:status".into(),
            payload: serde_json::json!({"done": true}),
            delivery: DeliveryMode::Sticky,
            publisher: "extension:canvas".into(),
            timestamp_ms: 1700000000000,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: BusEventPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }
}
