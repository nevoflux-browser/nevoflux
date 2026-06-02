# Reference: personal-state questions

For "what's going on with me / what's notable / 最近有什么 / anything unusual" — questions about the
user's own current state. The key rule: **do not run a semantic search.** Semantic search returns
polished pages and misses the recent activity bursts these questions are actually about. Background:
`skill_read('brain', 'conventions/salience-and-recency.md')`.

## Pick the tool by intent

| The user asks… | Use | Notes |
| --- | --- | --- |
| "what's going on with me / what have I been thinking about" / "我最近在想什么" | `get_recent_transcripts` | raw conversation summaries — the canonical source for the user's own state. Local-only (rejects remote callers). Start here for personal/emotional questions. |
| "what's notable / hot / anything crazy happening" / "最近有什么值得注意的" | `get_recent_salience` | recent pages ranked by emotional + activity salience; `days` window, optional `slugPrefix`. |
| "what stood out / what changed / what's unusual" / "有什么异常" | `find_anomalies` | statistical anomalies by cohort (tag/type) with explanatory baselines, e.g. "15 pages tagged wedding on 2026-04-28, baseline 0.3/day". |

## Interpreting results

- Words like "crazy / notable / big / 厉害" frequently mean **difficult or emotionally charged**, not
  impressive. Read the salient pages before assuming tone, and mirror the user's own framing.
- Lead with the salient or anomalous items, **cite their slugs**, and offer to open any of them
  (`get_page`) or to dig into a thread.
- These tools need **no search term** — that's the point. If you catch yourself composing a `query`
  for a personal-state question, stop and use salience/anomalies instead.

## Recent activity (pure recency)

"What did I touch this week" is *recency*, not salience — use:

```
list_pages(sort="updated_desc", updated_after="2026-05-26")
```
