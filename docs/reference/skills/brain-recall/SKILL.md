---
name: brain-recall
description: Read and recall from the user's knowledge base (gbrain) — search and query saved pages, recall facts, see what is notable or recent, browse a page's links and backlinks (what references it), read timelines, and give a read-only catch-up digest of their brain. Use whenever the user asks what they know about a person, company, or concept, what is going on with them, what they touched recently, what links to or mentions a page in their notes, or says 我的知识库里有没有 / 我之前记过 / 最近有什么. Personal-state questions (what is notable) use salience and anomalies, not semantic search. Not for the current web page or live web facts — use browser tools for those.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, recall, search, briefing]
enabled: true
triggers:
  - "what do I know about"
  - "tell me about"
  - "what happened with"
  - "background on"
  - "what's going on with me"
  - "what did I touch"
  - "daily briefing"
  - "catch me up"
  - "what links to"
  - "who works at"
  - "connected to"
  - "我的知识库里有"
  - "我之前记过"
  - "最近"
  - "日报"
dependencies:
  - "brain:conventions/brain-first.md"
  - "brain:conventions/search-modes.md"
  - "brain:conventions/salience-and-recency.md"
allowed_tools:
  - tool_search
  - tool_call_dynamic
---

# brain-recall — read & recall

Read from the user's knowledge base and answer from it. This is the most common brain operation.

## Brain basics (always apply)

Non-negotiable — these hold for every recall, no extra reading required:
- **Re-query every time.** Each invocation pulls fresh from the brain (`query`/`search` → `get_page`).
  Never recite from conversation history — even for a repeated question, run the retrieval again.
- **Pages** have `compiled_truth` (the authoritative summary) above a `---` divider and an
  append-only, **newest-first** `timeline` below. Answer from `compiled_truth` first.
- **Reach gbrain tools dynamically**: `tool_search` (query `brain` / `知识库` / `gbrain` / the topic)
  → `tool_call_dynamic(name, args)`; retry once with `知识库`/`gbrain` if empty.
- **Cite the slug *with its zone*** for every claim — `[slug · compiled_truth]` or `[slug · timeline]`
  — and **propagate** any `[Source: …]` markers the page carries so facts trace to origin.
- **Flag gaps and additions.** Say "your brain has nothing on X" rather than filling from general
  knowledge; if you add anything not on the page, mark it as supplemental, not recalled.
- The brain is **personal memory** — don't use it for world knowledge, live web, or the current page.

Full conventions when you need depth: `skill_read('brain', 'conventions/brain-first.md')` and the
other `brain/conventions/*.md`.

## Route the question

| The user is asking… | Do this |
| --- | --- |
| About a **person/company/concept/topic** | `query` (hybrid) → if thin, `search` (FTS) → `get_page` the best match. Answer from `compiled_truth` first; cite the slug. |
| **"What's going on with me / notable / 最近 / anything big"** | Personal-state — use `get_recent_salience` / `find_anomalies` / `get_recent_transcripts`. **Not** semantic search. Read `references/personal-state.md`. |
| **"What did I touch this week / recently"** | `list_pages` with `sort=updated_desc` (optionally `updated_after`). Pure recency, not search. |
| A **quick fact / preference** ("what's my…") | `recall` (hot-memory facts), optionally filtered by `entity`/`since`. |
| **History of an entity** ("timeline of X") | `get_timeline` on the entity's slug. |
| **"What's related / who works at / connected to"** | Direct edges: `get_links` (outgoing) / `get_backlinks` (incoming). Typed relationship lookup: `traverse_graph` with a `link_type` + `depth ≤ 2` (e.g. "who works at Acme" → `works_at`, direction `in`). See `references/query-patterns.md`. Open / deep multi-hop reasoning → `brain-think`. |
| **"Catch me up / daily briefing / 日报"** | Read `references/briefing.md` and run the read-only digest. |

## Answering — the contract

Recall answers are grounded in the brain, never invented:

- **No hallucination.** Answer only from brain content. When the brain has nothing relevant, say so
  plainly ("your brain doesn't have anything on X") and offer to research + save — don't quietly fill
  the gap from general knowledge.
- **Cite the slug for every claim**, and **propagate** a page's own inline `[Source: …]` markers so
  the user can trace each fact to its origin.
- **Source precedence** when synthesizing or resolving disagreement: the user's direct statements >
  `compiled_truth` > `timeline` > external sources. Lead with the higher.
- **Surface conflicts, don't hide them.** If sources disagree, present both with their citations
  rather than silently picking one.
- **Flag staleness.** If a fact looks outdated (old timeline date, superseded by a later entry), say
  so and note the date.

## Output format

Structure a recall answer as:

1. **Direct answer** to the question, in prose — lead with this, not the search play-by-play.
2. **Inline citations** next to each claim, tracing to the page and zone, e.g.
   `According to [people/jane-doe · compiled_truth], she joined Acme in 2025.` **Propagate** any
   `[Source: …]` markers the page already carries.
3. **Gap flags** for anything the brain doesn't cover: "Your brain has nothing on X."
4. **Conflict notes** when sources disagree: state both, each with its citation, and which one wins
   by precedence — don't silently merge.

Keep it tight and scannable.

## Retrieve efficiently

- **Decompose** the question into the strategies it needs and run them — keyword (`search`),
  semantic (`query`), relational (`get_links`/`get_backlinks`) — then read the **top 3–5** hits
  before answering. Don't stop at the first hit for a "tell me about" question.
- **Chunks vs full page (token-aware).** A factual / yes-no lookup is usually answered from
  `search`/`query` chunks (or `get_chunks`) — don't full-load. Reserve `get_page` (full page) for
  "tell me about X", where the user wants the complete picture.
- **Cross-check search quality.** If `query` (hybrid) results look off or thin, re-run with `search`
  (keyword) and compare — the two paths surface different hits. Persistently broken results (not just
  empty) → hand off to `brain-care` (`run_doctor` / `get_health`).

## Core habits

- **Answer from `compiled_truth`**, fall back to `timeline` for detail/provenance. Always **cite the
  slug** you read from.
- If a topic returns nothing relevant, say so plainly and offer to research + save (that's
  `brain-capture`) — don't fabricate.
- Tuning a `query`/`search`? Read `skill_read('brain', 'conventions/search-modes.md')` for cost
  presets (conservative/balanced/tokenmax) and knobs (`recency`, `salience`, `since/until`,
  `limit/offset`, `detail`). See `references/query-patterns.md` for worked patterns.
- Personal-state vs recency vs salience is subtle — `skill_read('brain', 'conventions/salience-and-recency.md')`.

## Anti-patterns

Don't:
- **Answer from general knowledge** when the brain has relevant content — consult it first.
- **Fabricate** facts the brain doesn't contain — flag the gap instead.
- **Silently pick one side** of a conflict — present both with citations.
- **Full-load pages when chunks suffice** — reserve `get_page` for "tell me about X".
- **Ignore source precedence** — a casual external note doesn't override the user's own statement.
- **Run a semantic `query` for personal-state** ("what's notable / 最近") — use salience / anomalies.

## Stay read-only

`brain-recall` does not write. If the user wants to save, update, or enrich what you surfaced, hand
off to `brain-capture`. Don't consult the brain for world knowledge, live web facts, or the current
page — use web/browser tools.
