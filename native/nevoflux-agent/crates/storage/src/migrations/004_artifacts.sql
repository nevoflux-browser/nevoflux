-- Artifact persistence: store artifacts created by create_artifact tool
CREATE TABLE IF NOT EXISTS artifacts (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    description TEXT,
    content_type TEXT NOT NULL,
    content     TEXT NOT NULL DEFAULT '',
    files       TEXT,
    entry       TEXT,
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session_id ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_created_at ON artifacts(created_at DESC);
