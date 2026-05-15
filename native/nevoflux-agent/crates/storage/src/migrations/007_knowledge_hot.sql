-- Add hot-flag columns for dynamic knowledge injection into system prompts.
-- hot=1 entries are included in Layer 1 of the soul context.
ALTER TABLE knowledge ADD COLUMN hot INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge ADD COLUMN hot_summary TEXT;

-- Partial index: only index hot entries for fast retrieval.
CREATE INDEX idx_knowledge_hot ON knowledge(hot) WHERE hot = 1;
