-- Unified knowledge store (pending + validated entries)
CREATE TABLE IF NOT EXISTS knowledge (
    id TEXT PRIMARY KEY,              -- K-{date}-{seq}
    category TEXT NOT NULL,           -- site_interaction | tool_optimization | user_preference
    subcategory TEXT,
    domain TEXT,                      -- associated domain (NULL = universal)
    summary TEXT NOT NULL,
    details TEXT NOT NULL,
    resolution TEXT,
    confidence REAL DEFAULT 0.5,
    hit_count INTEGER DEFAULT 1,
    success_count INTEGER DEFAULT 0,
    fail_count INTEGER DEFAULT 0,
    effectiveness REAL GENERATED ALWAYS AS (
        CASE WHEN (success_count + fail_count) > 0
        THEN CAST(success_count AS REAL) / (success_count + fail_count)
        ELSE 0.5 END
    ) STORED,
    priority TEXT DEFAULT 'medium',
    status TEXT DEFAULT 'pending',    -- pending | validated | promoted | archived
    source_ids TEXT,                  -- source LearningEntry IDs (JSON array)
    related_ids TEXT,
    tags TEXT,                        -- JSON array
    privacy_level TEXT DEFAULT 'internal',
    promotion_target TEXT,            -- IDENTITY | SOUL | USER | TOOLS | AGENTS
    promoted_section TEXT,            -- target section in file
    source_type TEXT DEFAULT 'system', -- system | manual
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_hit_at TEXT,
    promoted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_knowledge_category ON knowledge(category);
CREATE INDEX IF NOT EXISTS idx_knowledge_domain ON knowledge(domain);
CREATE INDEX IF NOT EXISTS idx_knowledge_status ON knowledge(status);

-- Site adaptation graph (promotes to TOOLS.md)
CREATE TABLE IF NOT EXISTS site_adaptations (
    id TEXT PRIMARY KEY,
    domain TEXT NOT NULL,
    url_pattern TEXT,
    adaptation_type TEXT NOT NULL,    -- selector_result | spa_behavior | api_pattern | anti_bot | automation_outcome
    content TEXT NOT NULL,            -- JSON
    verified BOOLEAN DEFAULT FALSE,
    last_verified_at TEXT,
    success_rate REAL DEFAULT 0.0,
    sample_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_site_domain ON site_adaptations(domain);
CREATE INDEX IF NOT EXISTS idx_site_type ON site_adaptations(adaptation_type);

-- MCP tool effectiveness stats (promotes to TOOLS.md)
CREATE TABLE IF NOT EXISTS tool_stats (
    id TEXT PRIMARY KEY,
    tool_name TEXT NOT NULL,
    intent_category TEXT,
    call_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    avg_latency_ms REAL,
    avg_token_cost REAL,
    common_params TEXT,              -- JSON
    failure_patterns TEXT,           -- JSON
    best_combinations TEXT,          -- JSON
    updated_at TEXT NOT NULL
);

-- Learning system effectiveness metrics
CREATE TABLE IF NOT EXISTS learning_metrics (
    id TEXT PRIMARY KEY,
    metric_type TEXT NOT NULL,       -- success_rate | retry_rate | knowledge_hit | promotion_rate
    domain TEXT,                     -- NULL = global
    period TEXT NOT NULL,            -- YYYY-MM-DD (daily aggregation)
    value REAL NOT NULL,
    sample_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_metrics_type ON learning_metrics(metric_type);
CREATE INDEX IF NOT EXISTS idx_metrics_period ON learning_metrics(period);

-- Knowledge health view (lazy decay calculation)
CREATE VIEW IF NOT EXISTS knowledge_health AS
SELECT
    id, category, summary, confidence, effectiveness,
    hit_count, promotion_target, status,
    julianday('now') - julianday(last_hit_at) AS days_since_last_hit,
    julianday('now') - julianday(created_at) AS age_days
FROM knowledge
WHERE status IN ('pending', 'validated');
