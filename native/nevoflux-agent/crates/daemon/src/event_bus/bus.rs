//! EventBus orchestrator: publish/subscribe, sticky cache, permission enforcement,
//! persistent writing, and event delivery.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::mpsc;

use super::permissions::{PermissionChecker, PermissionResult};
use super::persistent::PersistentWriterHandle;
use super::topic::{validate_pattern, validate_topic, TopicError};
use super::types::{
    BackpressurePolicy, BusEvent, Delivery, SubscriberIdentity, Subscription, TopicPattern,
};

/// Maximum total bytes for the sticky cache (64 MiB).
const STICKY_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of sticky events retained per topic.
const STICKY_MAX_PER_TOPIC: usize = 100;

/// Errors produced by EventBus operations.
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    /// The topic string failed validation.
    #[error("invalid topic: {0}")]
    InvalidTopic(#[from] TopicError),

    /// The pattern string failed validation.
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),

    /// The caller does not have permission for this operation.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// The delivery channel is closed.
    #[error("channel closed")]
    ChannelClosed,
}

/// Handle returned to a subscriber after a successful `subscribe()` call.
///
/// The caller reads events from `rx`. The `id` can be passed to
/// `EventBus::unsubscribe()` to remove the subscription.
pub struct SubscriptionHandle {
    /// Unique subscription identifier.
    pub id: String,
    /// Receiver end of the event delivery channel.
    pub rx: mpsc::Receiver<BusEvent>,
}

impl std::fmt::Debug for SubscriptionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionHandle")
            .field("id", &self.id)
            .field("rx", &"<mpsc::Receiver>")
            .finish()
    }
}

/// Internal bookkeeping for an active subscription.
struct ActiveSubscription {
    sub: Subscription,
    tx: mpsc::Sender<BusEvent>,
}

/// The main EventBus orchestrator.
///
/// Coordinates publish/subscribe, sticky caching, permission checks,
/// persistent writing, and event delivery with configurable backpressure.
pub struct EventBus {
    /// All active subscriptions.
    subscriptions: RwLock<Vec<ActiveSubscription>>,

    /// Sticky event cache: topic -> ring of recent events.
    sticky_cache: DashMap<String, VecDeque<BusEvent>>,

    /// Approximate total bytes stored in the sticky cache.
    sticky_cache_bytes: AtomicUsize,

    /// LRU tracker: topic -> monotonic counter (higher = more recent).
    sticky_lru: DashMap<String, u64>,

    /// LRU counter source.
    lru_counter: AtomicU64,

    /// Optional handle for writing persistent events.
    persistent_handle: Option<PersistentWriterHandle>,

    /// Total events successfully published.
    events_published: AtomicU64,

    /// Total events dropped due to backpressure.
    events_dropped: AtomicU64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Create a new `EventBus` without persistence.
    pub fn new() -> Self {
        Self {
            subscriptions: RwLock::new(Vec::new()),
            sticky_cache: DashMap::new(),
            sticky_cache_bytes: AtomicUsize::new(0),
            sticky_lru: DashMap::new(),
            lru_counter: AtomicU64::new(0),
            persistent_handle: None,
            events_published: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        }
    }

    /// Create a new `EventBus` with a persistence handle.
    pub fn with_persistence(handle: PersistentWriterHandle) -> Self {
        Self {
            persistent_handle: Some(handle),
            ..Self::new()
        }
    }

    // ── Public API ─────────────────────────────────────────────────

    /// Publish an event to the bus.
    ///
    /// 1. Validates the topic.
    /// 2. Checks publish permissions.
    /// 3. Stores the event in the sticky cache (if `Delivery::Sticky`).
    /// 4. Sends the event to the persistent writer (if `Delivery::Persistent`).
    /// 5. Delivers the event to all matching subscribers.
    pub async fn publish(&self, event: BusEvent) -> Result<(), EventBusError> {
        // 1. Validate topic.
        validate_topic(&event.topic)?;

        // 2. Permission check.
        let perm = PermissionChecker::check_publish(&event.topic, &event.publisher);
        if let PermissionResult::Denied(reason) = perm {
            return Err(EventBusError::PermissionDenied(reason));
        }

        // 3. Sticky caching.
        if event.delivery == Delivery::Sticky {
            self.store_sticky(&event);
        }

        // 4. Persistent writing.
        if event.delivery == Delivery::Persistent {
            if let Some(ref handle) = self.persistent_handle {
                // Best-effort: if the channel is full we still deliver in-memory.
                let _ = handle.send(event.clone());
            }
        }

        // 5. Deliver to matching subscribers.
        self.deliver(&event).await;

        self.events_published.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Subscribe to events matching `pattern`. Always replays sticky events.
    ///
    /// Kept as-is for backward compatibility and tests. Callers that want to
    /// opt out of sticky replay should use [`subscribe_with_options`].
    pub fn subscribe(
        &self,
        pattern: TopicPattern,
        subscriber: SubscriberIdentity,
        policy: BackpressurePolicy,
        buffer_size: usize,
    ) -> Result<SubscriptionHandle, EventBusError> {
        self.subscribe_with_options(pattern, subscriber, policy, buffer_size, true)
    }

    /// Subscribe to events matching `pattern`, optionally skipping sticky replay.
    ///
    /// 1. Validates the pattern.
    /// 2. Checks subscribe permissions.
    /// 3. Creates an mpsc channel with the requested buffer size.
    /// 4. Replays any matching sticky events (only if `replay_sticky`).
    /// 5. Registers the subscription.
    pub fn subscribe_with_options(
        &self,
        pattern: TopicPattern,
        subscriber: SubscriberIdentity,
        policy: BackpressurePolicy,
        buffer_size: usize,
        replay_sticky: bool,
    ) -> Result<SubscriptionHandle, EventBusError> {
        // 1. Validate pattern.
        match &pattern {
            TopicPattern::Exact(topic) => {
                validate_topic(topic)?;
            }
            TopicPattern::Wildcard(pat) => {
                validate_pattern(pat).map_err(|e| EventBusError::InvalidPattern(e.to_string()))?;
            }
            TopicPattern::DoubleWildcard(prefix) => {
                // An empty prefix means "match everything" and is valid.
                if !prefix.is_empty() {
                    // Validate the prefix as a topic (it has no wildcards).
                    validate_topic(prefix)?;
                }
            }
        }

        // 2. Permission check.
        let perm = PermissionChecker::check_subscribe(&pattern, &subscriber);
        if let PermissionResult::Denied(reason) = perm {
            return Err(EventBusError::PermissionDenied(reason));
        }

        // 3. Create channel.
        let buf = buffer_size.max(1);
        let (tx, rx) = mpsc::channel(buf);

        let sub = Subscription::new(pattern, subscriber)
            .with_policy(policy)
            .with_buffer_size(buf);
        let id = sub.id.clone();

        // 4. Replay sticky events (if requested).
        if replay_sticky {
            self.replay_sticky(&sub.pattern, &tx);
        }

        // 5. Register.
        {
            let mut subs = self
                .subscriptions
                .write()
                .expect("subscriptions lock poisoned");
            subs.push(ActiveSubscription { sub, tx });
        }

        Ok(SubscriptionHandle { id, rx })
    }

    /// Remove a subscription by its id. Returns `true` if found and removed.
    pub fn unsubscribe(&self, subscription_id: &str) -> bool {
        let mut subs = self
            .subscriptions
            .write()
            .expect("subscriptions lock poisoned");
        let before = subs.len();
        subs.retain(|a| a.sub.id != subscription_id);
        subs.len() < before
    }

    // ── Metrics ────────────────────────────────────────────────────

    /// Number of active subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions
            .read()
            .expect("subscriptions lock poisoned")
            .len()
    }

    /// Total events published since the bus was created.
    pub fn total_published(&self) -> u64 {
        self.events_published.load(Ordering::Relaxed)
    }

    /// Total events dropped due to backpressure.
    pub fn total_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }

    /// Number of distinct topics in the sticky cache.
    pub fn sticky_topic_count(&self) -> usize {
        self.sticky_cache.len()
    }

    // ── Private helpers ────────────────────────────────────────────

    /// Deliver an event to all matching subscribers, respecting backpressure policy.
    async fn deliver(&self, event: &BusEvent) {
        // Collect matching targets while holding the read lock, then release it
        // before any `.await` to avoid holding a std RwLockReadGuard across an
        // await point.
        let (targets, total_subs): (Vec<(mpsc::Sender<BusEvent>, BackpressurePolicy)>, usize) = {
            let subs = self
                .subscriptions
                .read()
                .expect("subscriptions lock poisoned");
            let total = subs.len();
            let t = subs
                .iter()
                .filter(|a| a.sub.pattern.matches(&event.topic))
                .map(|a| (a.tx.clone(), a.sub.policy))
                .collect();
            (t, total)
        };

        tracing::debug!(
            topic = %event.topic,
            matched = targets.len(),
            total_subs,
            "EventBus deliver"
        );

        for (tx, policy) in &targets {
            let sent = match policy {
                BackpressurePolicy::DropNewest | BackpressurePolicy::DropOldest => {
                    // For channel-based delivery, DropOldest degrades to DropNewest
                    // because we cannot pop from the sender side.
                    tx.try_send(event.clone()).is_ok()
                }
                BackpressurePolicy::Block => tx
                    .send_timeout(event.clone(), Duration::from_millis(100))
                    .await
                    .is_ok(),
            };
            if !sent {
                self.events_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Store an event in the sticky cache.
    ///
    /// Enforces per-topic cap (`STICKY_MAX_PER_TOPIC`) and global memory limit
    /// (`STICKY_CACHE_MAX_BYTES`) with LRU eviction.
    fn store_sticky(&self, event: &BusEvent) {
        let event_bytes = Self::estimate_event_bytes(event);

        // Evict until we are under the memory limit.
        while self.sticky_cache_bytes.load(Ordering::Relaxed) + event_bytes > STICKY_CACHE_MAX_BYTES
        {
            if !self.evict_lru_sticky() {
                break;
            }
        }

        // Update LRU timestamp for this topic.
        let tick = self.lru_counter.fetch_add(1, Ordering::Relaxed);
        self.sticky_lru.insert(event.topic.clone(), tick);

        let mut entry = self.sticky_cache.entry(event.topic.clone()).or_default();
        let queue = entry.value_mut();

        // Per-topic cap: drop the oldest event if at the limit.
        if queue.len() >= STICKY_MAX_PER_TOPIC {
            if let Some(old) = queue.pop_front() {
                let old_bytes = Self::estimate_event_bytes(&old);
                self.sticky_cache_bytes
                    .fetch_sub(old_bytes, Ordering::Relaxed);
            }
        }

        self.sticky_cache_bytes
            .fetch_add(event_bytes, Ordering::Relaxed);
        queue.push_back(event.clone());
    }

    /// Evict the least recently used sticky topic.
    ///
    /// Returns `true` if a topic was evicted, `false` if the cache is empty.
    fn evict_lru_sticky(&self) -> bool {
        // Find the topic with the smallest LRU counter.
        let victim = self
            .sticky_lru
            .iter()
            .min_by_key(|entry| *entry.value())
            .map(|entry| entry.key().clone());

        let Some(topic) = victim else {
            return false;
        };

        self.sticky_lru.remove(&topic);
        if let Some((_, queue)) = self.sticky_cache.remove(&topic) {
            let freed: usize = queue.iter().map(Self::estimate_event_bytes).sum();
            self.sticky_cache_bytes.fetch_sub(freed, Ordering::Relaxed);
        }
        true
    }

    /// Replay all matching sticky events to a new subscriber's channel.
    fn replay_sticky(&self, pattern: &TopicPattern, tx: &mpsc::Sender<BusEvent>) {
        for entry in self.sticky_cache.iter() {
            let topic = entry.key();
            if !pattern.matches(topic) {
                continue;
            }
            for event in entry.value().iter() {
                if event.is_expired() {
                    continue;
                }
                // Best-effort replay: drop if the channel is already full.
                let _ = tx.try_send(event.clone());
            }
        }
    }

    /// Rough byte estimate for a single `BusEvent` for memory tracking.
    fn estimate_event_bytes(event: &BusEvent) -> usize {
        // id + topic + serialized payload + some overhead
        event.id.len() + event.topic.len() + event.payload.to_string().len() + 128
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::types::PublisherIdentity;
    use serde_json::json;

    /// Helper: create an ephemeral event from Internal publisher.
    fn internal_ephemeral(topic: &str, payload: serde_json::Value) -> BusEvent {
        BusEvent::ephemeral(topic, payload, PublisherIdentity::Internal)
    }

    /// Helper: create a sticky event from Internal publisher.
    fn internal_sticky(topic: &str, payload: serde_json::Value) -> BusEvent {
        BusEvent::sticky(topic, payload, PublisherIdentity::Internal)
    }

    #[tokio::test]
    async fn test_publish_and_receive() {
        let bus = EventBus::new();

        let mut handle = bus
            .subscribe(
                TopicPattern::exact("task:status"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        bus.publish(internal_ephemeral("task:status", json!({"done": true})))
            .await
            .unwrap();

        let event = handle.rx.try_recv().unwrap();
        assert_eq!(event.topic, "task:status");
        assert_eq!(event.payload, json!({"done": true}));
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let bus = EventBus::new();

        let mut handle = bus
            .subscribe(
                TopicPattern::wildcard("task:*"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        bus.publish(internal_ephemeral("task:created", json!(1)))
            .await
            .unwrap();
        bus.publish(internal_ephemeral("task:completed", json!(2)))
            .await
            .unwrap();
        // This should NOT match (different prefix).
        bus.publish(internal_ephemeral("agent:heartbeat", json!(3)))
            .await
            .unwrap();

        let e1 = handle.rx.try_recv().unwrap();
        let e2 = handle.rx.try_recv().unwrap();
        assert_eq!(e1.payload, json!(1));
        assert_eq!(e2.payload, json!(2));
        assert!(handle.rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_publish_permission_denied() {
        let bus = EventBus::new();

        // An Agent cannot publish to task:* topics.
        let event = BusEvent::ephemeral(
            "task:status",
            json!({}),
            PublisherIdentity::Agent {
                session_id: "s1".into(),
            },
        );
        let result = bus.publish(event).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventBusError::PermissionDenied(_)
        ));
    }

    #[tokio::test]
    async fn test_subscribe_permission_denied() {
        let bus = EventBus::new();

        // An Extension cannot subscribe to task:* topics.
        let result = bus.subscribe(
            TopicPattern::exact("task:status"),
            SubscriberIdentity::Extension {
                proxy_id: "p1".into(),
            },
            BackpressurePolicy::DropNewest,
            16,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventBusError::PermissionDenied(_)
        ));
    }

    #[tokio::test]
    async fn test_invalid_topic_rejected() {
        let bus = EventBus::new();

        let event = BusEvent::ephemeral("bad topic!", json!(null), PublisherIdentity::Internal);
        let result = bus.publish(event).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventBusError::InvalidTopic(_)
        ));
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus = EventBus::new();

        let handle = bus
            .subscribe(
                TopicPattern::exact("task:status"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        assert_eq!(bus.subscription_count(), 1);
        assert!(bus.unsubscribe(&handle.id));
        assert_eq!(bus.subscription_count(), 0);
        // Double-unsubscribe returns false.
        assert!(!bus.unsubscribe(&handle.id));
    }

    #[tokio::test]
    async fn test_sticky_replay_on_subscribe() {
        let bus = EventBus::new();

        // Publish sticky events before subscribing.
        bus.publish(internal_sticky("task:status", json!({"step": 1})))
            .await
            .unwrap();
        bus.publish(internal_sticky("task:status", json!({"step": 2})))
            .await
            .unwrap();

        // Subscribe — should receive the two sticky events via replay.
        let mut handle = bus
            .subscribe(
                TopicPattern::exact("task:status"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        let e1 = handle.rx.try_recv().unwrap();
        let e2 = handle.rx.try_recv().unwrap();
        assert_eq!(e1.payload, json!({"step": 1}));
        assert_eq!(e2.payload, json!({"step": 2}));
        assert!(handle.rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();

        let mut h1 = bus
            .subscribe(
                TopicPattern::exact("task:status"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        let mut h2 = bus
            .subscribe(
                TopicPattern::exact("task:status"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        bus.publish(internal_ephemeral("task:status", json!("hello")))
            .await
            .unwrap();

        assert_eq!(h1.rx.try_recv().unwrap().payload, json!("hello"));
        assert_eq!(h2.rx.try_recv().unwrap().payload, json!("hello"));
    }

    #[tokio::test]
    async fn test_metrics() {
        let bus = EventBus::new();

        assert_eq!(bus.total_published(), 0);
        assert_eq!(bus.total_dropped(), 0);
        assert_eq!(bus.subscription_count(), 0);
        assert_eq!(bus.sticky_topic_count(), 0);

        bus.publish(internal_sticky("task:status", json!(1)))
            .await
            .unwrap();

        assert_eq!(bus.total_published(), 1);
        assert_eq!(bus.sticky_topic_count(), 1);
    }

    #[tokio::test]
    async fn test_double_wildcard_subscription_internal_only() {
        let bus = EventBus::new();

        // Internal subscriber can use double-wildcard.
        let mut handle = bus
            .subscribe(
                TopicPattern::double_wildcard("task"),
                SubscriberIdentity::Internal,
                BackpressurePolicy::DropNewest,
                16,
            )
            .unwrap();

        bus.publish(internal_ephemeral("task:status", json!("a")))
            .await
            .unwrap();
        bus.publish(internal_ephemeral("task:status:updated", json!("b")))
            .await
            .unwrap();
        // Should NOT match.
        bus.publish(internal_ephemeral("agent:heartbeat", json!("c")))
            .await
            .unwrap();

        assert_eq!(handle.rx.try_recv().unwrap().payload, json!("a"));
        assert_eq!(handle.rx.try_recv().unwrap().payload, json!("b"));
        assert!(handle.rx.try_recv().is_err());

        // Non-internal subscriber should be denied double-wildcard.
        let result = bus.subscribe(
            TopicPattern::double_wildcard("task"),
            SubscriberIdentity::Agent {
                session_id: "s1".into(),
            },
            BackpressurePolicy::DropNewest,
            16,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventBusError::PermissionDenied(_)
        ));
    }

    #[test]
    fn test_event_bus_default() {
        let bus = EventBus::default();
        assert_eq!(bus.subscription_count(), 0);
        assert_eq!(bus.total_published(), 0);
        assert_eq!(bus.total_dropped(), 0);
        assert_eq!(bus.sticky_topic_count(), 0);
    }
}
