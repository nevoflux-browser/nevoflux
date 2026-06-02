# Convention: search-modes

How to choose retrieval behavior and tune a `query`/`search` call. Adapted from gbrain's search-modes
convention.

## Two entry points

- **`search`** — keyword full-text search. Fast, literal. Good for exact terms, names, code symbols,
  and when the user's wording likely matches the page text.
- **`query`** — hybrid (vector + keyword + multi-query expansion). Better for fuzzy/semantic intent
  ("notes about pricing strategy") and multi-hop topics. **Start here** for topic/entity recall;
  fall back to `search` if hybrid misses an exact term.

## Cost presets (mode)

gbrain bundles cost knobs into a `search.mode`. Choose by how hard the question is:

| Mode | Token budget | Expansion | Result cap | Use for |
| --- | --- | --- | --- | --- |
| `conservative` | small (~4k) | off | 10 | quick lookups, cheap checks |
| `balanced` (default) | medium (~12k) | off | 25 | most recall questions |
| `tokenmax` | unbounded | on (multi-query) | 50 | hard multi-hop / "find everything about" |

Resolution order: per-call override > per-key config > mode bundle > balanced default. When unsure,
let it default to balanced.

## Per-call knobs on `query`

- `recency` — `off` (default, canonical truth) / `on` (per-prefix age decay: daily/chat/media decay,
  concepts/writing stay evergreen) / `strong` (for "today / right now"). Omit and gbrain auto-detects.
- `salience` — `off` / `on` / `strong` — surface emotionally-weighted, take-rich pages. Independent
  of `recency` (orthogonal axes; see `salience-and-recency.md`).
- `expand` — multi-query expansion (default true on `query`).
- `detail` — `low` (compiled truth only) / `medium` (default) / `high` (all chunks).
- `since` / `until` — temporal window; ISO date or relative (`7d`, `2w`, `1y`).
- `limit` / `offset` — page size + pagination.
- `cross_modal` — `text` (default) / `image` / `both` / `auto` for image-intent queries.

## Guidance

- Topic/entity recall → `query` (balanced), answer from `compiled_truth`, cite the slug.
- "Find everything / be exhaustive" → `tokenmax`, raise `limit`, paginate with `offset`.
- "What's recent / what did I touch" → don't tune search; use `list_pages sort=updated_desc`.
- Personal-state ("what's notable / 最近") → don't use search at all; use salience/anomalies
  (`salience-and-recency.md`).
- Exact term or code symbol → `search` (FTS), optionally `lang`/`symbol_kind` filters.
