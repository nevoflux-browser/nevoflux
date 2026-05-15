-- Trace spans for agent self-healing pattern detection.
-- Lightweight records: no raw LLM I/O, only summaries for pattern matching.
-- Cleaned up when session ends via DELETE WHERE session_id = ?.

CREATE TABLE IF NOT EXISTS trace_spans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL,
    iteration   INTEGER NOT NULL,
    span_type   TEXT NOT NULL,
    tool_name   TEXT,
    tool_params TEXT,
    success     INTEGER NOT NULL DEFAULT 1,
    error_code  TEXT,
    error_msg   TEXT,
    duration_ms INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_trace_spans_session
    ON trace_spans(session_id, iteration);
