# Convention: quality

Three standards that keep the brain trustworthy and connected. Apply them whenever you **write** a
page (brain-capture, and brain-think when it persists a synthesis). Adapted from gbrain's quality
convention for the NevoFlux client.

## 1. Citations (every fact)

Every fact written to a page carries an inline source marker so its provenance and freshness are
verifiable. Format: `[Source: <where> · <YYYY-MM-DD>]`.

Authority hierarchy (highest first) — when sources conflict, prefer the higher:
1. The user's direct statements (highest authority)
2. Meetings / emails / conversations the user took part in
3. Web content (articles, filings, press releases) — include the URL
4. Social media / third-party commentary (lowest)

Synthesized claims ("based on the above…") must cite the underlying facts they were derived from.
When you capture a webpage or document, record the source URI (use `put_raw_data` / `file_upload`
for the raw artifact) so the citation can point back to it.

## 2. Reciprocal back-links (no orphan mentions)

Any mention of a person or company that **has** (or warrants) its own page must be a **bidirectional**
link. When page A mentions entity B:
- add a link A → B (`add_link`), and
- ensure B references A back — append a timeline bullet on B's page:
  `- **YYYY-MM-DD** | Referenced in [<A title>](<A slug>) — <context>`

An unlinked mention is a broken brain: the knowledge exists but the graph can't surface it. After
writing, check `get_backlinks` on the entities you touched and close any missing reciprocal link.

## 3. Notability gate (don't create noise)

Before creating a **new** standing page, ask whether the subject warrants ongoing reference:
- Will the user plausibly interact with / refer back to this again?
- Is it relevant to their work, projects, or interests?

A one-off mention of a minor entity usually does **not** clear the gate — fold it into the timeline
of an existing page, or leave it in `inbox/`, rather than minting a thin standalone page. Prefer
**depth on pages that matter** over comprehensive-but-shallow coverage.

## Quick checklist before `put_page`

- [ ] Facts carry `[Source: …]` markers, respecting the authority hierarchy.
- [ ] Mentioned entities with pages are linked both ways (reciprocal back-links closed).
- [ ] New page clears the notability gate (else fold into existing / `inbox/`).
- [ ] Durable facts in `compiled_truth`; a dated entry **appended** to `timeline` (history intact).
- [ ] The slug follows the hybrid filing convention; reported back to the user.
