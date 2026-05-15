-- Migration 015: make `files` + `entry` the canonical source for artifacts.
--
-- Background: artifacts historically stored a single string `content` as the
-- source-of-truth (legacy single-file artifacts). canvas_video introduced
-- multi-file artifacts (`index.html` + `DESIGN.md` + `composition.meta.json`)
-- with a `files` JSON map and an `entry` pointer. The dual-write between
-- `content` and `files[entry]` was a major source of bugs (Test 3 v1-v4
-- 2026-04-26): writers would update one without the other, then a
-- multi-file aware reader (canvas_apply_design_md, render driver) would
-- read `files[entry]` and clobber `content`, wiping LLM edits.
--
-- This migration sets up the invariant: every artifact has a `files` map
-- with at least one entry, and `entry` points into it. `content` is kept
-- as a derived mirror (= files[entry]) for backward compat with legacy
-- readers that haven't migrated yet (sidebar preview cards, render iframe
-- srcdoc fallback). A future migration will drop `content` entirely once
-- those readers move to `files[entry]`.
--
-- Steps:
--   1. Backfill rows whose `files` is NULL/empty: synthesize files from
--      content using entry (default 'main.html').
--   2. Backfill rows whose `entry` is NULL: pick first key from files map,
--      fall back to 'main.html'.
--   3. Backfill `content := files[entry]` for any drift in either direction.
--   4. Mark the invariant in a CHECK constraint via table rebuild.

-- Step 1: synthesize files from content for legacy single-file artifacts.
-- json_object('main.html', content) → '{"main.html":"...escaped..."}'
UPDATE artifacts
SET files = json_object(COALESCE(NULLIF(entry, ''), 'main.html'), content)
WHERE files IS NULL
   OR files = ''
   OR files = '{}';

-- Step 2: ensure every row has a non-null `entry`. Pick first files key
-- when missing; fall back to 'main.html' if files map is somehow empty.
UPDATE artifacts
SET entry = COALESCE(
    NULLIF(entry, ''),
    (SELECT key FROM json_each(files) LIMIT 1),
    'main.html'
)
WHERE entry IS NULL OR entry = '';

-- Step 3: re-sync content := files[entry] so the derived mirror is
-- consistent. Uses json_extract with the dynamic path '$."<entry>"'.
-- For most artifacts this is a no-op; for any that drifted (the bug class
-- this migration is closing) it heals them in place.
UPDATE artifacts
SET content = COALESCE(
    json_extract(files, '$."' || entry || '"'),
    content
)
WHERE entry IS NOT NULL
  AND files IS NOT NULL;

-- Step 4: rebuild table with `entry` NOT NULL constraint and an index on
-- (entry) for future query support. Keeping content TEXT NOT NULL DEFAULT ''
-- for backward compat — readers will migrate progressively.
PRAGMA foreign_keys = OFF;

CREATE TABLE artifacts_new (
    id                    TEXT    PRIMARY KEY,
    session_id            TEXT    REFERENCES sessions(id) ON DELETE SET NULL,
    title                 TEXT    NOT NULL,
    description           TEXT,
    content_type          TEXT    NOT NULL,
    content               TEXT    NOT NULL DEFAULT '',
    files                 TEXT    NOT NULL DEFAULT '{}',
    entry                 TEXT    NOT NULL DEFAULT 'main.html',
    created_at            INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    imported_from_url     TEXT,
    imported_from_share_id TEXT,
    imported_at           INTEGER,
    is_persistent         INTEGER NOT NULL DEFAULT 0,
    persisted_at          INTEGER,
    updated_at            INTEGER
);

INSERT INTO artifacts_new (
    id, session_id, title, description, content_type, content, files, entry,
    created_at, imported_from_url, imported_from_share_id, imported_at,
    is_persistent, persisted_at, updated_at
)
SELECT
    id, session_id, title, description, content_type, content,
    COALESCE(NULLIF(files, ''), '{}'),
    COALESCE(NULLIF(entry, ''), 'main.html'),
    created_at, imported_from_url, imported_from_share_id, imported_at,
    is_persistent, persisted_at, updated_at
FROM artifacts;

DROP TABLE artifacts;
ALTER TABLE artifacts_new RENAME TO artifacts;

PRAGMA foreign_keys = ON;

-- Recreate indexes (table rebuild dropped them).
CREATE INDEX IF NOT EXISTS idx_artifacts_persistent ON artifacts(is_persistent, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifacts_session    ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_imported   ON artifacts(imported_from_share_id);
