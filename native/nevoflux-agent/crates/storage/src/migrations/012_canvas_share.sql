-- Canvas Share: track active shares and import provenance

-- 1. Add import-tracking columns to artifacts table
ALTER TABLE artifacts ADD COLUMN imported_from_url TEXT;
ALTER TABLE artifacts ADD COLUMN imported_from_share_id TEXT;
ALTER TABLE artifacts ADD COLUMN imported_at INTEGER;

-- 2. Active shares table
CREATE TABLE IF NOT EXISTS artifact_shares (
    artifact_id         TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    share_id            TEXT NOT NULL UNIQUE,
    share_url           TEXT NOT NULL,
    encrypted_password  TEXT NOT NULL,
    encrypted_owner_token TEXT NOT NULL,
    expires_at          INTEGER NOT NULL,
    view_count          INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (artifact_id, share_id)
);
CREATE INDEX IF NOT EXISTS idx_artifact_shares_artifact_id ON artifact_shares(artifact_id);
CREATE INDEX IF NOT EXISTS idx_artifact_shares_share_id ON artifact_shares(share_id);
CREATE INDEX IF NOT EXISTS idx_artifact_shares_expires_at ON artifact_shares(expires_at);

-- 3. Share history
CREATE TABLE IF NOT EXISTS artifact_share_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id TEXT NOT NULL,
    share_id    TEXT NOT NULL,
    reason      TEXT NOT NULL,
    expired_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_artifact_share_history_artifact_id ON artifact_share_history(artifact_id);
