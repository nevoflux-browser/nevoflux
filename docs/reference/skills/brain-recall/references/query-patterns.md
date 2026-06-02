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

## Relationship lookups (shallow graph)

For direct "who/what is connected" questions, walk the link graph shallowly — deep multi-hop
reasoning is `brain-think`:

- `get_links(slug)` / `get_backlinks(slug)` — direct out / in edges of a page.
- `traverse_graph(slug, link_type=..., direction="in"|"out"|"both", depth<=2)` — typed relationship
  lookup. Common `link_type`s: `works_at`, `attended`, `invested_in`, `founded`, `advises`,
  `mentions`, `source`.
  - "who works at Acme" → `traverse_graph(slug="companies/acme-corp", link_type="works_at", direction="in", depth=1)`
  - "what has Jane attended" → `traverse_graph(slug="people/jane-doe", link_type="attended", direction="out")`

Keep it shallow (`depth <= 2`). Open-ended "how is everything connected" or transitive multi-hop
reasoning → hand off to `brain-think`.

## Tuning knobs (on `query`)

- **Exhaustive** ("find everything about X"): raise `limit`, paginate with `offset`, consider the
  `tokenmax` mode (expansion on).
- **Time-scoped**: `since`/`until` (ISO date or relative like `7d`, `2w`, `1y`).
- **Recency-aware**: `recency="on"` (or `"strong"` for "today/right now"). Leave `off` for canonical
  truth.
- **Mattering-aware**: `salience="on"` to surface emotionally-weighted / take-rich pages.
- **Detail level**: `detail="low"` (compiled truth only) / `"medium"` (default) / `"high"` (all
  chunks) — use `high` only when you need raw chunk text.

## Chunks vs full page (token-aware)

`search`/`query` return **chunks**, which are often enough. Don't full-load a page when chunks
answer the question:

- **Factual / yes-no lookup** → answer from the `search`/`query` chunks (or `get_chunks(slug)` for a
  specific page's chunks). No `get_page`.
- **"Tell me about X" (complete picture)** → `get_page(slug)` for the full page.

## Answering well

- **Source precedence** when reading/synthesizing: the user's direct statements > `compiled_truth` >
  `timeline` > external sources. Lead with `compiled_truth`; pull from `timeline` for detail or
  provenance.
- **Cite the slug** beside each fact, and **propagate** any inline `[Source: …]` markers the page
  already carries so the user can trace facts to origin.
- **Conflicts:** if sources disagree, present both with citations — don't silently pick one.
- **Staleness:** flag facts that look outdated (old/ superseded timeline entries); note the date.
- If nothing relevant comes back after a hybrid + FTS attempt (and a `知识库`/`gbrain` retry on
  `tool_search`), tell the user plainly and offer to research + save via `brain-capture`. If results
  look *broken* (not merely empty), suggest `brain-care` diagnostics.
- Don't paginate forever — after a couple of pages with no better hits, summarize what you found and
  ask whether to dig deeper.
