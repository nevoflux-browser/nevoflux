---
name: brain-recall
description: Read and recall from the user's knowledge base (gbrain) — search and query saved pages, recall facts, see what is notable or recent, browse a page's links and backlinks (what references it), read timelines, and give a read-only catch-up digest of their brain. Use whenever the user asks what they know about a person, company, or concept, what is going on with them, what they touched recently, what links to or mentions a page in their notes, or says 我的知识库里有没有 / 我之前记过 / 最近有什么. Personal-state questions (what is notable) use salience and anomalies, not semantic search. Not for the current web page or live web facts — use browser tools for those.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, recall, search, briefing]
enabled: true
triggers:
  - "what do I know about"
  - "what's going on with me"
  - "what did I touch"
  - "daily briefing"
  - "catch me up"
  - "what links to"
  - "我的知识库里有"
  - "我之前记过"
  - "最近"
  - "日报"
allowed_tools:
  - tool_search
  - tool_call_dynamic
---

# brain-recall — read & recall

Read from the user's knowledge base and answer from it. This is the most common brain operation.
First load the shared page model: `skill_read('brain', 'conventions/brain-first.md')`. Reach gbrain
tools via `tool_search` + `tool_call_dynamic` (see that file).

## Route the question

| The user is asking… | Do this |
| --- | --- |
| About a **person/company/concept/topic** | `query` (hybrid) → if thin, `search` (FTS) → `get_page` the best match. Answer from `compiled_truth` first; cite the slug. |
| **"What's going on with me / notable / 最近 / anything big"** | Personal-state — use `get_recent_salience` / `find_anomalies` / `get_recent_transcripts`. **Not** semantic search. Read `references/personal-state.md`. |
| **"What did I touch this week / recently"** | `list_pages` with `sort=updated_desc` (optionally `updated_after`). Pure recency, not search. |
| A **quick fact / preference** ("what's my…") | `recall` (hot-memory facts), optionally filtered by `entity`/`since`. |
| **History of an entity** ("timeline of X") | `get_timeline` on the entity's slug. |
| **"What's related / what links here"** | `get_links` (outgoing) / `get_backlinks` (incoming). For multi-hop relationship reasoning, that's `brain-think` (`traverse_graph`). |
| **"Catch me up / daily briefing / 日报"** | Read `references/briefing.md` and run the read-only digest. |

## Core habits

- **Answer from `compiled_truth`**, fall back to `timeline` for detail/provenance. Always **cite the
  slug** you read from.
- If a topic returns nothing relevant, say so plainly and offer to research + save (that's
  `brain-capture`) — don't fabricate.
- Tuning a `query`/`search`? Read `skill_read('brain', 'conventions/search-modes.md')` for cost
  presets (conservative/balanced/tokenmax) and knobs (`recency`, `salience`, `since/until`,
  `limit/offset`, `detail`). See `references/query-patterns.md` for worked patterns.
- Personal-state vs recency vs salience is subtle — `skill_read('brain', 'conventions/salience-and-recency.md')`.

## Stay read-only

`brain-recall` does not write. If the user wants to save, update, or enrich what you surfaced, hand
off to `brain-capture`. Don't consult the brain for world knowledge, live web facts, or the current
page — use web/browser tools.
