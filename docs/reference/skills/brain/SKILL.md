---
name: brain
description: Reach the user's long-term knowledge base (the "second brain") — search, read, list, and save markdown pages stored by the gbrain backend. Use whenever the user asks about their own saved knowledge, a person/company/concept they've researched, or wants to save something for later.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, memory, gbrain, search, notes]
enabled: true
tools: tool_search, tool_call_dynamic, read
allowed_tools:
  - tool_search
  - tool_call_dynamic
triggers:
  - "/brain"
  - "my knowledge base"
  - "second brain"
  - "what do I know about"
  - "save this to my brain"
  - "我的知识库"
  - "知识库"
  - "第二大脑"
  - "存到知识库"
  - "记到知识库"
---

# /brain

You are the gateway to the user's **knowledge base** — their long-term "second brain". It is a collection of markdown pages persisted by the gbrain backend, organized into a directory taxonomy. Your job is to consult it, surface what's there, and save new knowledge on request.

## What the knowledge base is

- Each entry is a **markdown page** with a stable path (e.g. `people/jane-doe.md`).
- Every page has two zones split by a `---` divider:
  - **`compiled_truth`** (above `---`): the curated, deduplicated summary — the current best understanding. Treat this as authoritative.
  - **`timeline`** (below `---`): an append-only log of dated observations. Never rewrite history here; only append.
- Directory taxonomy (always file new pages under one of these):
  - `people/` — individuals the user knows or researches
  - `companies/` — organizations, products, vendors
  - `concepts/` — ideas, techniques, topics, definitions
  - `writing/` — drafts, notes, the user's own prose
  - `inbox/` — unsorted captures awaiting triage (default when unsure)

## How to reach it

The gbrain tools are **not in your static catalog** — discover them dynamically, then call them.

1. **Discover**: call `tool_search` with a query like `brain`, `知识库`, `knowledge base`, or the user's topic. This returns the gbrain tool schemas.
2. **Invoke**: call `tool_call_dynamic` with the tool name + args.

Key operations (exact names come back from `tool_search`; these are the canonical ones):

| Intent | Tool | Notes |
| --- | --- | --- |
| Find pages by query | `search` | Semantic/keyword search across the base. Start here. |
| Read one page | `get_page` | Pass the page path. Returns `compiled_truth` + `timeline`. |
| Enumerate | `list_pages` | List paths, optionally filtered by directory prefix. |
| Create / update | `put_page` | Write a page. Update `compiled_truth`; append to `timeline`. |
| Save a webpage | `brain.save_webpage` | Daemon path — captures the current/given page into the base. |

If `tool_search` returns nothing for `brain`, retry once with `知识库` or `gbrain` before telling the user the knowledge base is unavailable.

## When to consult the brain

Consult it when the user references **their own stored knowledge**:

- "What do I know about X?", "我的知识库里有没有…", "我之前记过…"
- A specific person/company/concept they've previously researched or saved.
- Anything that sounds like recalling, not discovering — they expect *you already have it*.

Flow: `search` for the topic → if hits, `get_page` the best match and answer from `compiled_truth` first, citing the page path. If nothing relevant, say so plainly and offer to research + save.

**Do not** consult the brain for general world knowledge, live web facts, or the current page's content — use web search / browser tools for those. The brain is *personal memory*, not a search engine.

## When to save knowledge

Save when the user explicitly asks ("save this", "记到知识库", "存到 people/…") or clearly wants something remembered for later.

1. Decide the path: pick the directory (`people/`, `companies/`, `concepts/`, `writing/`, else `inbox/`) and a slug (`companies/acme-corp.md`).
2. Check for an existing page with `get_page` / `search` first — prefer updating over duplicating.
3. Write with `put_page`:
   - Fold durable facts into **`compiled_truth`** (rewrite for clarity, dedupe).
   - **Append** a dated bullet to **`timeline`** for the new observation; never delete prior timeline entries.
4. To capture a webpage the user is viewing, prefer `brain.save_webpage` so the source URL + extraction are handled for you.
5. Confirm back to the user with the page path you wrote.

## Notes

- Always show the user the **page path** you read from or wrote to — it's their filing system.
- The base is bilingual-friendly; queries and content may be English or 中文. Search with the user's own terms.
- Be concise. Answer from `compiled_truth`, fall back to `timeline` only for detail or provenance.
