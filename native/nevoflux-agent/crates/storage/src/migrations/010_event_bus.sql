-- Persistent event storage for EventBus
-- Events with Delivery::Persistent are written here for durability and history queries.

CREATE TABLE event_bus_persistent (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    payload TEXT NOT NULL,
    publisher_kind TEXT NOT NULL,
    publisher_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE INDEX idx_ebp_topic ON event_bus_persistent(topic);
CREATE INDEX idx_ebp_expires_at ON event_bus_persistent(expires_at);
CREATE INDEX idx_ebp_created_at ON event_bus_persistent(created_at DESC);
