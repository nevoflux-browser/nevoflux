-- Migration 014: add is_persistent + nullable session_id to artifacts
--
-- Steps:
--   1. Add three new columns to artifacts via ALTER TABLE ADD COLUMN.
--   2. Backfill is_persistent/persisted_at/updated_at for imported rows.
--   3. Backfill updated_at = created_at for remaining rows.
--   4. Rebuild artifacts with session_id nullable + FK ON DELETE SET NULL.
--   5. Create new indexes.

-- Step 1: add columns
ALTER TABLE artifacts ADD COLUMN is_persistent INTEGER NOT NULL DEFAULT 0;
ALTER TABLE artifacts ADD COLUMN persisted_at  INTEGER;
ALTER TABLE artifacts ADD COLUMN updated_at    INTEGER;

-- Step 2: backfill imported artifacts → persistent
UPDATE artifacts
SET
    is_persistent = 1,
    persisted_at  = COALESCE(imported_at, created_at),
    updated_at    = COALESCE(imported_at, created_at)
WHERE imported_from_share_id IS NOT NULL;

-- Step 3: backfill updated_at for all remaining rows
UPDATE artifacts
SET updated_at = created_at
WHERE updated_at IS NULL;

-- Step 4: rebuild artifacts table so session_id is nullable and FK is ON DELETE SET NULL.
-- PRAGMA foreign_keys must be OFF for the table-rebuild trick.
PRAGMA foreign_keys = OFF;

CREATE TABLE artifacts_new (
    id                    TEXT    PRIMARY KEY,
    session_id            TEXT    REFERENCES sessions(id) ON DELETE SET NULL,
    title                 TEXT    NOT NULL,
    description           TEXT,
    content_type          TEXT    NOT NULL,
    content               TEXT    NOT NULL DEFAULT '',
    files                 TEXT,
    entry                 TEXT,
    created_at            INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    imported_from_url     TEXT,
    imported_from_share_id TEXT,
    imported_at           INTEGER,
    is_persistent         INTEGER NOT NULL DEFAULT 0,
    persisted_at          INTEGER,
    updated_at            INTEGER
);

INSERT INTO artifacts_new (
    id,
    session_id,
    title,
    description,
    content_type,
    content,
    files,
    entry,
    created_at,
    imported_from_url,
    imported_from_share_id,
    imported_at,
    is_persistent,
    persisted_at,
    updated_at
)
SELECT
    id,
    session_id,
    title,
    description,
    content_type,
    content,
    files,
    entry,
    created_at,
    imported_from_url,
    imported_from_share_id,
    imported_at,
    is_persistent,
    persisted_at,
    updated_at
FROM artifacts;

DROP TABLE artifacts;
ALTER TABLE artifacts_new RENAME TO artifacts;

PRAGMA foreign_keys = ON;

-- Step 5: create indexes
-- Replaces old idx_artifacts_session_id and idx_artifacts_created_at.
CREATE INDEX IF NOT EXISTS idx_artifacts_persistent ON artifacts(is_persistent, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifacts_session    ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_imported   ON artifacts(imported_from_share_id);
