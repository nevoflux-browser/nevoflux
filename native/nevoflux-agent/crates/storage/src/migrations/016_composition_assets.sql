-- Migration 016: composition assets in a dedicated table.
--
-- Background: canvas_attach_asset writes binary assets (images, audio,
-- video, fonts) into `artifacts.files['assets/<name>']` as base64-encoded
-- TEXT. This co-locates two materially different storage classes in one
-- JSON column:
--   * Editable text files (DESIGN.md, index.html, composition.meta.json)
--     — round-tripped through ContentStore on every Canvas Editor save.
--   * Binary assets (assets/hero.png, assets/clip.mp4, ...) — written
--     once by canvas_attach_asset and never modified.
--
-- The dual-source-of-truth caused recurring bugs: ContentStore writes
-- (text-only) would round-trip through `mirror_canvas_to_artifacts_table`
-- and threaten to clobber assets unless the mirror added a defensive
-- merge for `assets/*` keys (server.rs §138-161). That merge is fragile
-- — any new write surface that bypasses it (canvas_apply_design_md,
-- ext-nevoflux.js editArtifact, etc.) silently wipes assets.
--
-- This migration moves binary assets into their own table so:
--   * `artifacts.files` only carries text files; ContentStore mirroring
--     becomes a 1:1 mapping with no merge-or-wipe risk.
--   * `composition_assets` stores raw bytes as BLOB (saving the 33 %
--     base64 expansion + faster page reads).
--   * FK CASCADE means deleting an artifact cleans its assets without
--     a separate sweep.
--
-- Step 1 (SQL, this file): create the table + indexes.
-- Step 2 (Rust, post-migration): scan existing `assets/*` entries in
--   `artifacts.files`, decode base64, insert into composition_assets,
--   strip the keys from files JSON. Tracked under _migrations name
--   "016b_composition_assets_data".

CREATE TABLE IF NOT EXISTS composition_assets (
    composition_id TEXT    NOT NULL,
    name           TEXT    NOT NULL,
    bytes          BLOB    NOT NULL,
    mime_type      TEXT,
    created_at     INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (composition_id, name),
    FOREIGN KEY (composition_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_composition_assets_id
    ON composition_assets(composition_id);
