# Reference: strategic reading (persist on request)

Read a source (book, article, transcript, case study) **through the lens of a specific problem** the
user has, and produce an applied playbook — not a general summary. Writes a page; apply
`conventions/quality.md` (every recommendation cites the source).

## Inputs

- A **source** (often produced via `brain-capture/references/media.md`: PDF→markitdown/docling,
  article→web fetch, video→yt-dlp subtitles), and
- A **problem/situation** to apply it to ("read this through the lens of my pricing decision").

## Workflow

1. **Triage** the source by relevance to the problem. Read the first ~2000 chars of each
   chapter/section and rate it **HIGH** (directly parallels the problem's dynamics/tactics) /
   **MEDIUM** (context or counter-example) / **LOW** (skip). Full-read HIGH, skim MEDIUM, skip LOW.
2. **Deep-read** the high-relevance sections.
3. **Map source → situation**: the core parallel between what the source teaches and the user's
   problem.
4. **Synthesize a playbook**:
   - Source tactics (what worked / what failed there).
   - Counter-tactics specific to the user's problem.
   - Applied recommendations (short / medium / long term) — **each cited** with a direct quote.
   - Key quotes worth keeping.
5. **Write** the page with `put_page` to `concepts/` (general strategy) or `projects/` (tied to a
   specific problem); link it to the source artifact and related entities; confirm the slug.

## Page template

````markdown
# <source title> — applied to <problem>

> Executive summary: how the source maps to the situation, the key insight, the bottom line.

## The core parallel
<the source's central dynamic mapped onto the user's situation>

## Chapter / section triage
- <section> — relevance HIGH/MEDIUM/LOW — "<quote if any>"

## The source's playbook
- What the protagonist DID · what WORKED (why) · what FAILED (why) · effective opponent moves

## Counter-tactics
- DO <x> (source evidence) · AVOID <y> (evidence) · WATCH FOR <z> (warning sign)

## Applied playbook
- Short-term (this week / month) · Medium-term (this quarter) · Long-term (this year+)

## Key quotes
<5–10 max, the most relevant>

## See also
<links to related concepts / projects>
````

File at `projects/<slug>/playbook.md` (problem-specific) or `concepts/<slug>.md` (general strategy).

## Output character & anti-patterns

Actionable, not academic. The primary artifact is the brain page; lead the user to it and surface
the top recommendations inline, each with its citation.

- **Not** a book summary (general "what's in this book") and **not** academic literary analysis —
  strategic utility only.
- Every recommendation cites the source with a **direct quote**, formatted like "do X, because when
  <protagonist action> (Ch N), <outcome>". No uncited or paraphrased recommendations.
- Always include the short / medium / long-term breakdown.
- Cap Key quotes at 5–10 — quality over quantity.
