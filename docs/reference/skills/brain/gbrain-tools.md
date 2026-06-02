# gbrain tool map

Canonical list of gbrain MCP tools, grouped by intent. **Generated** from the daemon resource
`crates/daemon/src/resources/gbrain-tools.json` (in the `nevoflux-agent` repo), then hand-annotated
with the owning skill. Regenerate when the daemon's tool list changes.

Reach any of these via `tool_search` (query: `brain` / `知识库` / topic) then
`tool_call_dynamic(name, args)`. Names below are canonical; confirm exact schemas from `tool_search`.

Owner legend: **R**=brain-recall · **C**=brain-capture · **T**=brain-think · **Ca**=brain-care.

## Pages / CRUD
- `get_page` — read a page by slug (optional `fuzzy`; `include_deleted` to surface soft-deletes). — R, C, Ca
- `put_page` — write/update a page (markdown + frontmatter); chunks, embeds, reconciles tags/links. — C
- `delete_page` — soft-delete (recoverable within 72h). — *(not wrapped; recovery side only)*
- `list_pages` — list pages with filters; `sort=updated_desc` for "what's recent". — R, C
- `restore_page` — clear a soft-delete within the 72h window. — Ca
- `resolve_slugs` — fuzzy-resolve a partial slug to matching slugs. — R, C
- `get_chunks` — content chunks for a page. — R
- `get_versions` / `revert_version` — page version history / revert. — Ca

## Search
- `search` — keyword full-text search. — R
- `query` — hybrid vector+keyword+multi-query (recency/salience/cross_modal knobs). — R
- `search_by_image` — image-as-query retrieval. — R *(deferred unless image search added)*

## Tags
- `add_tag` / `remove_tag` / `get_tags`. — C (read tags: R)

## Graph / links
- `get_links` / `get_backlinks` — outgoing / incoming links for a page. — R
- `add_link` / `remove_link` — create / remove an edge (typed). — C
- `traverse_graph` — graph walk (depth, direction, link_type). — R (shallow typed, depth<=2) · T (deep / open multi-hop)
- `find_orphans` — pages with no inbound links. — Ca

## Timeline
- `get_timeline` — timeline entries for a page. — R
- `add_timeline_entry` — append a dated entry. — C

## Personal / hot-memory
- `get_recent_salience` — pages ranked by emotional + activity salience (no search term). — R
- `find_anomalies` — statistical anomalies in recent activity, by cohort. — R
- `get_recent_transcripts` — raw recent conversation summaries (local-only). — R
- `recall` — query per-source hot-memory facts (entity/since/session). — R
- `extract_facts` — extract structured facts from a turn into hot memory. — C
- `forget_fact` — expire a fact. — C

## Synthesis / analysis
- `think` — multi-hop synthesis across pages+takes+graph; cited answer with conflict/gap analysis. — T
- `find_experts` — who in the brain knows about a topic (expertise routing). — T
- `find_trajectory` — chronological claim trajectory for an entity (metrics, regressions, drift). — T
- `find_contradictions` — cached suspected-contradiction findings (report). — Ca (read), T (context)

## Takes / calibration
- `takes_list` / `takes_search` — list / search typed claims (fact|take|bet|hunch). — T
- `takes_scorecard` — accuracy + Brier for resolved bets. — T
- `takes_calibration` — calibration curve (observed vs predicted by weight bucket). — T
- `get_calibration_profile` — active calibration profile for a holder. — T

## Health / maintenance
- `get_health` — health dashboard (embed coverage, stale pages, orphans). — Ca
- `run_doctor` — structured DoctorReport. — Ca
- `get_stats` — page/chunk counts. — Ca
- `get_brain_identity` — version, engine kind, counters (banner). — Ca
- `sync_brain` — sync git repo to brain (incremental; `dry_run`/`full`/`no_embed`). — Ca

## Raw / files / provenance
- `put_raw_data` / `get_raw_data` — store/retrieve raw source data for a page. — C
- `file_list` / `file_upload` / `file_url` — stored-file management. — C
- `log_ingest` / `get_ingest_log` — ingestion event log. — C

## Deferred (not wrapped in v1)
- **Jobs / Minions queue**: `submit_job`, `get_job`, `list_jobs`, `cancel_job`, `retry_job`,
  `get_job_progress`, `pause_job`, `resume_job`, `replay_job`, `send_job_message`, `submit_agent`.
- **Schema packs**: `get_active_schema_pack`, `list_schema_packs`, `schema_stats`, `schema_lint`,
  `schema_graph`, `schema_explain_type`, `schema_review_orphans`, `schema_apply_mutations`,
  `reload_schema_pack`.
- **Sources / federation**: `sources_add`, `sources_list`, `sources_remove`, `sources_status`.
- **Code intelligence**: `code_callers`, `code_callees`, `code_def`, `code_refs`, `code_blast`,
  `code_flow`, `code_traversal_cache_clear`.
- **Misc**: `whoami`, `purge_deleted_pages` (admin/local-only).

These are deliberately out of scope for the v1 consumer suite (see the design spec). Add a
`brain-code` / sharing / admin skill later if needed.
