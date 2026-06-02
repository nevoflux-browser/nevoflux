---
name: brain-care
description: Diagnose, sync, and recover the user's brain (gbrain) — health and doctor reports, stats, find contradictions and orphans (report only), sync the index, and recover pages (restore soft-deletes within 72 hours, revert to a previous version). This is read-only plus recovery; it never rewrites pages to fix issues and never deletes or purges. Use for is my brain healthy, what is inconsistent, orphan pages, sync my brain, undo that delete, restore a page, 知识库健康, 同步, 恢复.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, care, health, maintenance, recovery]
enabled: true
triggers:
  - "is my brain healthy"
  - "brain health"
  - "what's inconsistent"
  - "orphan"
  - "sync my brain"
  - "undo that delete"
  - "restore"
  - "知识库健康"
  - "同步"
  - "恢复"
allowed_tools:
  - tool_search
  - tool_call_dynamic
---

# brain-care — diagnose, sync, recover

Tend the brain: report on its health, sync the index, and recover from mistakes. **Read-only plus
recovery** — it never rewrites pages to "fix" issues, and never deletes or purges. Load
`skill_read('brain', 'conventions/brain-first.md')` (light). Reach gbrain via `tool_search` +
`tool_call_dynamic`.

## Diagnose (read-only)

| The user asks… | Use |
| --- | --- |
| "is my brain healthy / 知识库健康吗" | `get_health` (embed coverage, stale, orphans) · `run_doctor` (DoctorReport) · `get_stats` |
| "what's running / version" | `get_brain_identity` |
| "what's inconsistent / contradictions / 有没有矛盾" | `find_contradictions` — **report only** |
| "what's orphaned / unlinked / 孤立页面" | `find_orphans` |

How to read these reports and present findings: `references/diagnostics.md`. For contradictions,
surface each finding with its severity and the suggested `resolution_command`, then **let the user
decide** — do not apply fixes.

## Sync

`sync_brain` re-indexes (incremental; git pull). Suggest `dry_run=true` first for a preview,
especially on a large repo. Options: `full` (ignore checkpoint), `no_embed` (skip embeddings),
`no_pull` (skip git pull).

## Recover

Undo mistakes — see `references/recovery.md`:
- A page was **soft-deleted** (within 72h) → `restore_page(slug)`; verify with
  `get_page(slug, include_deleted=true)`.
- A page has a **bad edit** → `get_versions(slug)` then `revert_version(slug, version_id)`.

## Boundaries (by design)

- **No auto-fix.** Never `put_page` to resolve a contradiction, fix frontmatter, or repair
  citations — report and hand off to the user / `brain-capture`.
- **No destructive ops.** Never `delete_page` or `purge_deleted_pages`.
- **No background jobs** (Minion queue) and **no dream-cycle** — those are deferred / operator scope.
- **Runtime restart is the daemon's job.** If the brain is down/unhealthy, report it and point the
  user at the kb.wizard restart — don't try to restart services from here.
