-- Migration 017: /loop skill — scheduled and event-triggered prompt re-runs.
--
-- Spec: docs/superpowers/specs/2026-04-22-loop-skill-design.md §6.1.
-- Two tables:
--   * loops             — one row per loop, owns the trigger expression
--                         and ≤4KB scratchpad.
--   * loop_iterations   — per-fire history capped at 50 rows per loop
--                         (retention enforced application-side, not via
--                         a trigger, to stay within rusqlite's safe
--                         transaction boundary).
--
-- prompt_text XOR wrapped_skill is enforced by CHECK; scratchpad's 4KB
-- cap is enforced by CHECK so any writer that bypasses LoopRepository
-- still hits the boundary.

CREATE TABLE loops (
    id                     TEXT    PRIMARY KEY,
    session_id             TEXT    NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    trigger_expr           TEXT    NOT NULL,
    prompt_text            TEXT,
    wrapped_skill          TEXT,
    allowed_tool_classes   TEXT    NOT NULL,
    scratchpad             TEXT    NOT NULL DEFAULT '',
    state                  TEXT    NOT NULL,
    consecutive_failures   INTEGER NOT NULL DEFAULT 0,
    skipped_triggers       INTEGER NOT NULL DEFAULT 0,
    iteration_count        INTEGER NOT NULL DEFAULT 0,
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL,
    CHECK (LENGTH(scratchpad) <= 4096),
    CHECK ((prompt_text IS NOT NULL) <> (wrapped_skill IS NOT NULL))
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
