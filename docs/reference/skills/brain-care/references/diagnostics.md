# Reference: diagnostics (read-only)

How to read gbrain's health surfaces and present findings. Everything here is read-only — you
**report**; the user (or `brain-capture`) acts.

## The health surfaces

- `get_health` — dashboard: embedding coverage, stale pages, orphan counts. Good first glance at
  overall state.
- `run_doctor` — a structured DoctorReport (checks + status). Use for a thorough check-up.
- `get_stats` — page/chunk counts. Use for "how big is my brain" and to frame other numbers.
- `get_brain_identity` — version, engine kind, counters. Use to confirm what's running / for a
  status banner, and when the user suspects the brain is down.

## Quality dimensions to surface (read-only audit)

When the user wants a health pass, walk these and **report** (don't fix):
- **Stale pages** — not updated in a long time (from `get_health`).
- **Orphans** — `find_orphans` (pages with no inbound links). Suggest the user link or retire them.
- **Contradictions** — `find_contradictions(slug?, severity?)`: present each as
  "A vs B — <axis>, severity <low|medium|high>" with its `resolution_command`, and note it's
  **unresolved**. Let the user choose whether to act.
- **Missing cross-references / citations / filing** — note pages that violate `conventions/quality.md`
  (unlinked mentions, uncited facts, mis-filed slugs) so the user can decide on cleanup.

## Presenting

- Group by severity; lead with what actually needs attention. Cite slugs.
- For each issue, state the **suggested action** (e.g. the `resolution_command`, or "link these two
  pages") but make clear `brain-care` won't perform rewrites — offer to hand off to `brain-capture`
  if the user wants a fix made.
- Don't alarm: a few orphans/stale pages are normal. Frame numbers against `get_stats` totals.
