# Reference: recovery (restore vs revert)

Undo mistakes safely. `brain-care` recovers but never hard-deletes.

## Decision tree

- **A page was deleted by accident** (soft-deleted, within the 72h recovery window):
  1. Confirm it's recoverable: `get_page(slug, include_deleted=true)` — should show `deleted_at`.
  2. `restore_page(slug)` — clears the soft-delete; the page reappears in search/get/list.
  3. Verify: `get_page(slug)` (without the flag) now returns it.
  > After ~72h the autopilot purge hard-deletes soft-deleted pages — restore is only possible inside
  > the window. If past it, say so plainly.

- **A page has a bad edit** (content was overwritten/wrong):
  1. `get_versions(slug)` — list the version history.
  2. Identify the good version with the user (show dates/ids).
  3. `revert_version(slug, version_id)` — restore that version.
  4. Confirm the slug + which version it's now on.

## Rules

- Always **confirm with the user** which page/version before reverting — reverting is itself a
  change.
- Report the slug and the outcome.
- `brain-care` never `delete_page` / `purge_deleted_pages`. If the user wants to *delete* something,
  that's outside this skill's recover-only mandate — surface that and let them confirm the path
  explicitly elsewhere.
