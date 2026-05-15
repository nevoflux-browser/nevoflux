//! EventBus - Publish/subscribe event routing for NevoFlux Agent.
//!
//! Provides topic-based event routing with three delivery modes:
//! - Ephemeral: fire-and-forget, not retained
//! - Sticky: retained in memory, replayed to new subscribers
//! - Persistent: written to SQLite for durability and history queries

pub mod bus;
pub mod permissions;
pub mod persistent;
pub mod ring_buffer;
pub mod topic;
pub mod types;

pub use bus::{EventBus, EventBusError, SubscriptionHandle};
pub use permissions::{PermissionChecker, PermissionResult};
pub use persistent::{PersistentCleaner, PersistentWriter, PersistentWriterHandle};
pub use ring_buffer::BoundedRingBuffer;
pub use topic::{validate_pattern, validate_topic, TopicError};
pub use types::{
    BackpressurePolicy, BusEvent, Delivery, PublisherIdentity, SubscriberIdentity, Subscription,
    TopicPattern,
};
