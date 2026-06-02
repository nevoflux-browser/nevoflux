# Reference: ingest (URL / article / text / idea)

Capture external content or a fleeting idea into the brain. Read+write. Enforce
`conventions/quality.md` (citations, reciprocal links, notability).

## Workflow

1. **Fetch / read the source.** A URL → browser/web fetch; pasted text → use as-is; a quick idea →
   skip fetching.
2. **Preserve provenance.** For external sources, store the raw artifact: `put_raw_data(slug, source,
   data)` (or `file_upload` for a file), so citations can point back.
3. **File by primary subject** (hybrid taxonomy):
   - About a person → `people/`; a company/product → `companies/`; an idea/topic → `concepts/`.
   - A **quick idea** with no clear subject → `inbox/<slug>.md` for later triage. Don't over-file.
4. **Write the page** (`put_page`): a genuine summary in `compiled_truth` (analysis, not a raw dump),
   each fact cited; append a dated `timeline` entry.
5. **Cross-reference + timeline merge.** Link mentioned entities both ways (`add_link` + reciprocal
   back-link). **Merge the event across timelines**: the same event belongs on *every* mentioned
   entity's timeline — a note that "Alice met Bob at Acme" adds a dated entry to Alice, Bob, *and*
   Acme. If an author/source is worth tracking, create/curate their page too — but apply the
   **notability gate**; don't mint thin pages for one-off mentions.
6. **Confirm** the slug(s) written.

## Capture original thinking verbatim

When the content is the user's **own** thesis, framework, or observation, preserve their **exact
phrasing** — the user's language *is* the insight. Quote it verbatim in `compiled_truth` rather than
paraphrasing it away (file under `concepts/` or `writing/`).

## Test before bulk

Ingesting many items at once? **Sample 3–5 first**, show the user the resulting pages, and confirm
quality before processing the rest — a bad template multiplied across 50 pages is expensive to undo.

## Quick-idea fast path

"jot this down / 记一下" with no subject → one `put_page` to `inbox/<short-slug>.md` (or
`add_timeline_entry` on a relevant existing page). Confirm and move on; triage can happen later.

## data-research (recurring tracker variant)

When the user wants to **track** something over time ("track investor updates", "build a donations
tracker", "data dig"):

1. Define what to track (fields, sources: web/filings/press).
2. Search the sources; classify/filter noise.
3. **Archive raw sources first** (`put_raw_data`/`file_upload`) — before extraction, to avoid
   working-memory errors corrupting amounts.
4. Extract structured rows (regex-first, LLM fallback); dedupe against existing entries.
5. Update the tracker page (`put_page`) with backlinks + a dated `timeline` entry; cite each row.
