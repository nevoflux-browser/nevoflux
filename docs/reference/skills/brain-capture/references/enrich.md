# Reference: enrich (on request)

Turn a sparse entity mention into an intelligence-dossier-style page using web research. **Runs only
when the user explicitly asks** ("enrich this", "look them up", "补充资料") — not automatically.
Read+write; enforce `conventions/quality.md`.

## When to run

- The user explicitly asks to enrich / research an entity, **or**
- They confirm an offer (e.g. after meeting ingestion: "yes, enrich Jane").

Otherwise, capture only what was provided (save-only). Don't spend web calls unprompted.

## Workflow

1. **Check brain state.** `get_page` / `search` the entity — update vs create? Avoid duplication.
2. **Research** with browser/web tools (and any available MCP research tools): bio/role, company,
   projects, public statements, relationships, recent trajectory. Prioritize by the entity's
   importance to the user.
3. **Preserve provenance.** `put_raw_data(slug, source, data)` for API/web responses; `file_upload`
   for documents. Save raw **before** writing the dossier so citations point back and amounts aren't
   corrupted by working memory.
4. **Write the dossier** (`put_page`): a textured `compiled_truth` (who they are, why they matter,
   current state) — each fact cited per the authority hierarchy; append a dated `timeline` entry for
   this enrichment pass.
5. **Cross-reference.** Reciprocal links to related people/companies (`add_link` + back-links);
   update connected pages' timelines where relevant.
6. **Confirm** the slug and summarize what you added.

## Boundaries

- On request only; tell the user you're about to do web research before spending calls.
- Notability gate still applies — don't create dossiers for entities that don't warrant a standing
  page; fold a minor mention into an existing page's timeline instead.
- Building the dossier is capture; *analyzing across* entities is `brain-think`.
