# Reference: meeting ingestion

Turn a **provided** meeting transcript or notes into a structured meeting record. This processes
content the user gives you (paste, file, or a transcript produced via `references/media.md`) —
**not** live transcription.

## Output: a meeting page

File under `meetings/<YYYY-MM-DD-topic>.md` (or match existing meeting structure). Structure:

```markdown
# Meeting — <topic> (<YYYY-MM-DD>)

**Attendees:** [Jane Doe](people/jane-doe), [Sam Lee](people/sam-lee)

## Key decisions
- <decision> [Source: meeting]

## Action items
- [ ] <task> — owner: [Jane Doe](people/jane-doe), due <date>

## Notes
- <topic>: <discussion summary>

---
## Timeline
- **<YYYY-MM-DD>** | Meeting held — <one-line outcome>. [Source: meeting]
```

## Workflow

1. Parse attendees, decisions, action items (with owners), and notes by topic.
2. `put_page` the meeting page (decisions/actions cited).
3. **Propagate to entities (lightweight):** for each attendee/company with (or warranting) a page,
   add a reciprocal back-link and a dated `timeline` entry noting their participation:
   `- **<date>** | Attended [<meeting title>](meetings/<slug>) — <context>`. Apply the notability
   gate before creating a brand-new person page.
4. Cross-link the meeting ↔ related projects/companies.

## Enrich is offered, not automatic

Unlike a full server pipeline, do **not** auto-deep-research every attendee. Do the lightweight
entity linking above, then *offer*: "Want me to enrich any of these people/companies?" — deep
research runs only on request (`references/enrich.md`).
