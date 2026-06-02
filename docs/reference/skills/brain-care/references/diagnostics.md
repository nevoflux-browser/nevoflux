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

Walk these and **report** — brain-care never fixes. Most come from `get_health`/`run_doctor`; a few
need a quick manual look.

| Dimension | Source | What to report |
| --- | --- | --- |
| Stale pages | `get_health` / `list_pages` | `compiled_truth` older than the latest timeline entry (esp. >30d + recent activity) |
| Orphans | `find_orphans` | zero inbound links — suggest linking, not deleting |
| Dead links | inspection | links pointing to nonexistent pages |
| Missing cross-refs | inspection | entity mentions without a formal link |
| Back-link violations | `get_backlinks` | a mention without its reciprocal back-link |
| Citation gaps | spot-check 5–10 pages | facts without a `[Source: …]` marker |
| Filing violations | `search` | content misfiled in `sources/` instead of its primary dir |
| Tag inconsistencies | `get_tags` / `list_pages` | variant spellings ("vc" vs "venture-capital") |
| Contradictions | `find_contradictions` | A vs B + axis + severity + `resolution_command` (unresolved) |
| Search quality | compare `search` vs `query` | hybrid vs keyword diverge badly on known items → index issue |
| Infra | `run_doctor` | embedding staleness, schema behind, RLS off, orphan `.redirect.yaml`, large binaries in git |
| Open threads | inspection | timeline action items >30d still unresolved |

For each issue, give the **suggested action** but make clear brain-care reports only — fixes go to
the user or `brain-capture`.

## Output: Brain Health Report

Present a scannable report; lead with what needs attention, don't alarm (a few orphans/stale pages
are normal — frame counts against `get_stats` totals):

```
## Brain Health Report — <date>

| Dimension       | Issues | Suggested fix owner       |
|-----------------|--------|---------------------------|
| Stale pages     | N      | brain-capture (rewrite)   |
| Orphans         | N      | user (link or retire)     |
| Contradictions  | N      | user (resolution_command) |
| …               | …      | …                         |

### Details
<per-dimension: specific slugs + suggested action>

### Needs your call
<contradictions / deletions / anything brain-care won't do automatically>
```
