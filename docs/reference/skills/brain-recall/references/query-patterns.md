# Reference: query patterns

Worked patterns for reading the brain. Cost presets and the full knob list live in
`skill_read('brain', 'conventions/search-modes.md')`; this file is about *applying* them.

## Hybrid first, FTS fallback

```
query(query="acme pricing strategy", detail="medium")     # semantic + keyword, start here
# if it misses an exact term/name:
search(query="Acme Corp", limit=20)                        # full-text, literal
```

Then read the best hit:

```
get_page(slug="companies/acme-corp")                       # answer from compiled_truth, cite slug
```

## Resolving a fuzzy slug

When the user names something but you don't know the exact slug:

```
resolve_slugs(partial="acme")        # → candidate slugs
# or
get_page(slug="acme", fuzzy=true)    # fuzzy slug resolution
```

## Tuning knobs (on `query`)

- **Exhaustive** ("find everything about X"): raise `limit`, paginate with `offset`, consider the
  `tokenmax` mode (expansion on).
- **Time-scoped**: `since`/`until` (ISO date or relative like `7d`, `2w`, `1y`).
- **Recency-aware**: `recency="on"` (or `"strong"` for "today/right now"). Leave `off` for canonical
  truth.
- **Mattering-aware**: `salience="on"` to surface emotionally-weighted / take-rich pages.
- **Detail level**: `detail="low"` (compiled truth only) / `"medium"` (default) / `"high"` (all
  chunks) — use `high` only when you need raw chunk text.

## Answering well

- Lead with `compiled_truth`; pull from `timeline` only for detail or provenance.
- **Cite the slug** beside each fact so the user can navigate/verify.
- If nothing relevant comes back after a hybrid + FTS attempt (and a `知识库`/`gbrain` retry on
  `tool_search`), tell the user plainly and offer to research + save via `brain-capture`.
- Don't paginate forever — after a couple of pages with no better hits, summarize what you found and
  ask whether to dig deeper.
