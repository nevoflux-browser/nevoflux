# Reference: strategic reading (persist on request)

Read a source (book, article, transcript, case study) **through the lens of a specific problem** the
user has, and produce an applied playbook — not a general summary. Writes a page; apply
`conventions/quality.md` (every recommendation cites the source).

## Inputs

- A **source** (often produced via `brain-capture/references/media.md`: PDF→markitdown/docling,
  article→web fetch, video→yt-dlp subtitles), and
- A **problem/situation** to apply it to ("read this through the lens of my pricing decision").

## Workflow

1. **Triage** the source by relevance to the problem — rate sections/chapters; don't deep-read
   everything.
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

## Output character

Actionable, not academic. The primary artifact is the brain page; lead the user to it and surface
the top recommendations inline, each with its citation.
