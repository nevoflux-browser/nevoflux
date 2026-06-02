# Reference: briefing (read-only digest)

A "catch me up / daily briefing / 日报" is a **read-only synthesis** of what's recent and notable.
No brain pages are created or modified unless the user explicitly asks. Cite slugs throughout.

## Gather (read-only)

1. **Pulse** — what's notable and what's off:
   - `get_recent_salience(days=14)` — salient recent pages.
   - `find_anomalies()` — cohorts that spiked.
   - `find_contradictions(limit=5)` — surface (do not resolve) recent inconsistencies. Resolving is
     `brain-care`'s call, on the user's instruction.
2. **Recent touches** — `list_pages(sort="updated_desc", limit=20)` (optionally `updated_after`).
3. **Entity context** — for the people/companies that recur above, pull `get_page` /
   `get_timeline` to add a line of context.

## Assemble

Present a compact, scannable digest — scale sections to what actually has content:

```
# Briefing — <date>

## Pulse
- <notable / salient item> — [slug]
- ⚠ possible inconsistency: <a> vs <b> — [slugs]  (say it's unresolved)

## Recently touched
- <page> — <one line> — [slug]

## Worth your attention
- <time-sensitive item / anomaly> — [slug]
```

## Rules

- **Read-only.** If something looks worth saving or fixing, *offer* to (hand off to `brain-capture`
  or `brain-care`) — don't do it inside the briefing.
- Every line cites a slug so the user can jump in.
- Keep it tight: a briefing is a scan, not a report. Omit empty sections.
