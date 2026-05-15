-- Migration 018: /loop skill — switch from class-based tool authorization
-- (allowed_tool_classes JSON array) to mode-based (chat | browser | agent).
--
-- The new `mode` column maps 1:1 to `nevoflux_builtin_wasm::AgentMode`, so
-- the iteration executor can pass it directly to `AgentInput.mode` and let
-- builtin-wasm's `Agent::get_tools_for_mode` pick the canonical tool set.
-- This removes a parallel taxonomy that drifted out of sync with the
-- real ToolDefinition catalog (e.g. fictitious "browser_query" / "dom_query"
-- names that never existed).
--
-- All existing loop rows are wiped — pre-MVP migration. Iterations are
-- session-scoped so dropping the parent rows + ON DELETE CASCADE clears
-- everything cleanly.

DROP INDEX IF EXISTS loop_iterations_loop_idx;
DROP INDEX IF EXISTS loops_state_idx;
DROP INDEX IF EXISTS loops_session_idx;
DROP TABLE IF EXISTS loop_iterations;
DROP TABLE IF EXISTS loops;

CREATE TABLE loops (
    id                     TEXT    PRIMARY KEY,
    session_id             TEXT    NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    trigger_expr           TEXT    NOT NULL,
    prompt_text            TEXT,
    wrapped_skill          TEXT,
    mode                   TEXT    NOT NULL DEFAULT 'chat',
    scratchpad             TEXT    NOT NULL DEFAULT '',
    state                  TEXT    NOT NULL,
    consecutive_failures   INTEGER NOT NULL DEFAULT 0,
    skipped_triggers       INTEGER NOT NULL DEFAULT 0,
    iteration_count        INTEGER NOT NULL DEFAULT 0,
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL,
    CHECK (LENGTH(scratchpad) <= 4096),
    CHECK ((prompt_text IS NOT NULL) <> (wrapped_skill IS NOT NULL)),
    CHECK (mode IN ('chat', 'browser', 'agent'))
);

CREATE INDEX loops_session_idx ON loops(session_id);
CREATE INDEX loops_state_idx   ON loops(state);

CREATE TABLE loop_iterations (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    loop_id            TEXT    NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
    sequence_number    INTEGER NOT NULL,
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER,
    status             TEXT    NOT NULL,
    error_message      TEXT,
    tool_calls_json    TEXT,
    UNIQUE (loop_id, sequence_number)
);

CREATE INDEX loop_iterations_loop_idx ON loop_iterations(loop_id);
