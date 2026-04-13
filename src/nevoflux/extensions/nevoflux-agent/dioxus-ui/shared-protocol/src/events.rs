/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! EventBus protocol types (frontend mirror)

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeOptions {
    pub patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub replay_sticky: bool,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

fn default_true() -> bool { true }
fn default_buffer_size() -> usize { 256 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishOptions {
    pub topic: String,
    pub payload: serde_json::Value,
    pub delivery: DeliveryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub topic: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
}

fn default_limit() -> usize { 100 }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusEventPayload {
    pub event_id: String,
    pub topic: String,
    pub payload: serde_json::Value,
    pub delivery: DeliveryMode,
    pub publisher: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EventBusRequest {
    Subscribe(SubscribeOptions),
    Unsubscribe { subscription_id: String },
    Publish(PublishOptions),
    History(HistoryQuery),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum EventBusResponse {
    Subscribed { subscription_id: String, patterns: Vec<String> },
    Unsubscribed { subscription_id: String },
    Published { event_id: String },
    HistoryResult { topic: String, events: Vec<BusEventPayload> },
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusDelivery {
    pub subscription_id: String,
    pub event: BusEventPayload,
}
