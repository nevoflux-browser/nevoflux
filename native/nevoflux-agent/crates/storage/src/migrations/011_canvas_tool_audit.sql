-- Canvas Tool Whitelist: audit log for every tool invocation.
CREATE TABLE IF NOT EXISTS canvas_tool_invocations (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    backend_kind TEXT NOT NULL,
    args_json   TEXT NOT NULL DEFAULT '[]',
    cwd         TEXT,
    status      TEXT NOT NULL DEFAULT 'started',
    exit_code   INTEGER,
    stdout_len  INTEGER,
    stderr_len  INTEGER,
    error_msg   TEXT,
    duration_ms INTEGER,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_canvas_invocations_session
    ON canvas_tool_invocations(session_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_canvas_invocations_tool
    ON canvas_tool_invocations(tool_name, started_at DESC);
