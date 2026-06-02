# Convention: brain-first

The core conventions every brain skill shares: what a page is, where things get filed, how to reach
gbrain, and the "consult the brain first" policy.

## The page model

Each entry in the brain is a **markdown page** with a stable slug (e.g. `people/jane-doe.md`). Every
page has two zones split by a `---` divider:

- **`compiled_truth`** (above `---`) — the curated, deduplicated summary; the current best
  understanding. Treat this as **authoritative**. Answer from here first.
- **`timeline`** (below `---`) — an **append-only** log of dated observations. Never rewrite history
  here; only append. Use it for detail and provenance.

When reading, prefer `compiled_truth`; fall back to `timeline` for detail. When writing, fold durable
facts into `compiled_truth` (rewrite for clarity, dedupe) and **append** a dated bullet to
`timeline` — never delete prior timeline entries.

## Filing — hybrid taxonomy

Decide a page's slug by **matching what already exists first**, then falling back to a default:

1. **Match existing structure.** `resolve_slugs('<name>')` and/or `list_pages` with a prefix to see
   whether similar pages exist. If you find `companies/*`, file the new company under
   `companies/<slug>.md` to match.
2. **Fall back to the default taxonomy** when nothing similar exists:
   - `people/` — individuals the user knows or researches
   - `companies/` — organizations, products, vendors
   - `concepts/` — ideas, techniques, topics, definitions
   - `writing/` — drafts, notes, the user's own prose
   - `inbox/` — unsorted captures awaiting triage (the default when unsure)

Slugs are lowercase, hyphenated, and carry the directory prefix (`companies/acme-corp.md`). Prefer
**updating** an existing page over creating a near-duplicate — always check first with
`get_page`/`search`/`resolve_slugs`.

## Reaching gbrain tools

gbrain tools are discovered dynamically, not in the static catalog:

1. `tool_search` with `brain`, `知识库`, `gbrain`, or the user's topic → returns gbrain tool schemas.
2. `tool_call_dynamic(name, args)` to invoke.

If `tool_search` returns nothing for `brain`, retry once with `知识库` or `gbrain` before declaring
the knowledge base unavailable. The full grouped tool list is in `skill_read('brain', 'gbrain-tools.md')`.

## Consult-the-brain-first policy

For any request about the user's **own** saved knowledge — recall, "what do I know about…",
"我之前记过…", a person/company/concept they've researched — check the brain **before** answering
from scratch or from the web. The brain is personal memory.

Do **not** consult the brain for: general world knowledge, live web facts, or the content of the
page the user is currently viewing. Use web search / browser tools for those.

## Always show the slug

Whenever you read from or write to the brain, tell the user the page **slug** — it's their filing
system, and it lets them navigate and verify. Cite the slug next to facts you surface.

## Bilingual

The brain is bilingual-friendly. Queries and content may be English or 中文. Search with the user's
own terms; don't translate their query before searching.
