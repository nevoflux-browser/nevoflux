# Reference: page format

The shape of a brain page and how to write it with `put_page`.

## Anatomy

```markdown
---
type: company
tags: [vendor, ai]
created: 2026-05-30
updated: 2026-06-02
source_uri: https://acme.example/about
---

# Acme Corp

Acme builds X for Y. Series B (2025), ~120 people. Primary contact: [Jane Doe](people/jane-doe).
Pricing: usage-based, ~$0.01/req. [Source: meeting · 2026-05-30]

---

## Timeline
- **2026-05-30** | Kickoff call — agreed on pilot scope. [Source: meeting]
- **2026-06-02** | Saw their pricing page; usage-based confirmed. [Source: web https://acme.example/pricing]
```

- **Frontmatter** — `type`, `tags`, dates, optional `source_uri`. Keep keys minimal and consistent
  with existing pages.
- **`compiled_truth`** (above `---`) — the authoritative, deduplicated summary. Rewrite this for
  clarity as understanding improves. Facts carry inline `[Source: …]` markers.
- **`---` divider** then **`## Timeline`** (below) — append-only dated bullets. Never edit or remove
  prior entries.

## Writing it

```
put_page(slug="companies/acme-corp", content="<full markdown above>")
```

`put_page` takes the **full** page content (frontmatter + compiled_truth + timeline). To read first
and update in place:

```
get_page(slug="companies/acme-corp")   # read current content
# merge: update compiled_truth, append one new timeline bullet
put_page(slug="companies/acme-corp", content="<merged markdown>")
```

For incremental timeline additions without rewriting the page, `add_timeline_entry(slug, date,
summary, detail?, source?)`.

## Rules

- Durable facts → `compiled_truth`; new observation → an **appended** `timeline` bullet.
- Every fact cites a source (`conventions/quality.md`).
- Slug follows hybrid filing (`conventions/brain-first.md`); report it back to the user.
