-- Composite index for efficient session message queries.
-- Covers the common pattern: WHERE session_id = ? ORDER BY created_at [ASC|DESC] LIMIT N
-- Replaces separate single-column indexes for this query pattern.
CREATE INDEX IF NOT EXISTS idx_messages_session_created
    ON messages(session_id, created_at DESC);
