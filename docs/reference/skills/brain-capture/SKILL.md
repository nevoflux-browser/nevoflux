---
name: brain-capture
description: Save and ingest knowledge into the user's brain (gbrain). File new pages with a hybrid taxonomy, update compiled_truth and append dated timeline entries, tag and link pages, jot a quick note or fact, capture webpages, and ingest documents and media (PDF via markitdown or docling, office and ebook and zip via markitdown, YouTube and Bilibili subtitles via yt-dlp, images via vision). Enrich entity pages with web research on request. Use for save this, 记一下, 记到知识库, 存到, remember that, ingest this, process this meeting or PDF or video, 帮我记录.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, capture, ingest, enrich, media]
enabled: true
triggers:
  - "save this"
  - "remember that"
  - "ingest this"
  - "process this"
  - "enrich"
  - "记一下"
  - "记到知识库"
  - "存到"
  - "帮我记录"
  - "保存到知识库"
allowed_tools:
  - tool_search
  - tool_call_dynamic
  - run_command
  - read_file
---

# brain-capture — save & ingest

Write knowledge into the user's brain. Before writing, load the page model and filing rules
(`skill_read('brain', 'conventions/brain-first.md')`) and the writing standards
(`skill_read('brain', 'conventions/quality.md')` — citations, reciprocal back-links, notability).
Reach gbrain via `tool_search` + `tool_call_dynamic`. Page anatomy + a worked example:
`references/page-format.md`.

## Input-type router

Detect what the user handed you and route:

| Input | Go to |
| --- | --- |
| A **URL / article / pasted text / quick idea** | `references/ingest.md` |
| A **file / document / video** (PDF, docx, pptx, xlsx, csv, epub, zip, image, YouTube/Bilibili) | `references/media.md` |
| A **meeting transcript / notes** | `references/meeting.md` |
| "**Enrich** this / look them up" (entity dossier) | `references/enrich.md` (research **on request**) |
| A **standalone fact/preference** ("remember I prefer…") | `extract_facts` (hot memory) |
| The **webpage the user is viewing** | `save_webpage` (handles source URL + extraction) |

## Write protocol (every write)

1. **Check first.** `get_page` / `search` / `resolve_slugs` — prefer updating an existing page over
   a near-duplicate.
2. **Decide the slug** via hybrid filing (match existing structure, else default taxonomy — see
   `conventions/brain-first.md`).
3. **Fold durable facts into `compiled_truth`** (rewrite for clarity, dedupe).
4. **Append a dated `timeline` entry** for the new observation. Never rewrite/delete history.
5. **Tag and link.** `add_tag`; create reciprocal links (`add_link` + back-link on the other page)
   per `conventions/quality.md`. Cite sources inline.
6. **Confirm the slug** back to the user.

## Output format

After capturing, report concisely:
- **Page** — the slug written (and created or updated)
- **Type** — person / company / concept / meeting / media / inbox
- **Entities linked** — which pages got reciprocal back-links
- **Timeline** — dated entries added, and on which pages
- **Raw source** — where the original was preserved (if any)

## Anti-patterns

Don't:
- **Append to `compiled_truth`** — rewrite it to the current best understanding, discarding stale
  contradictions. (The newest-first `timeline` is the append-only log.)
- **Leave a mention unlinked** — every mentioned entity with a page gets a reciprocal back-link.
- **Skip raw-source preservation** — a page without provenance is unverifiable.
- **Bulk-ingest without a sample test** — verify 3–5 first (`references/ingest.md`).
- **Paraphrase the user's original thinking** — quote their exact words.
- **Write ambiently on every message** — capture is intent-triggered here; don't silently mutate the
  brain on turns where the user didn't ask to save.

## Boundaries

- **Enrichment is on request only** — save what's provided by default; do autonomous web research
  only when the user explicitly asks ("enrich / look them up"). See `references/enrich.md`.
- Media converters (`markitdown`/`docling`/`yt-dlp`) are external CLIs run via `run_command` — check
  availability first and tell the user how to install if missing (`references/media.md`).
- Don't analyze/synthesize here — that's `brain-think`. Don't delete/rewrite history.
