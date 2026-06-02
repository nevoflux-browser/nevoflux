# Reference: concept map (persist on request)

Build or refresh an intellectual map for a **topic/cluster** the user names. Topic-scoped, not a
whole-brain sweep (that batch job is deferred). Writes pages — apply `conventions/quality.md`.

## Workflow

1. **Gather the cluster.** `query`/`list_pages` to pull the concept stubs/pages around the topic
   (e.g. `concepts/` pages matching the theme, or pages tagged accordingly).
2. **Dedupe + merge.** Collapse near-duplicate stubs into canonical concepts (semantic overlap,
   substring matches). Decide one canonical slug per concept.
3. **Tier** each concept:
   - **T1 Canon** — recurring, well-developed, cross-referenced.
   - **T2 Developing** — emerging, some depth.
   - **T3 Speculative** / **T4 Riff** — one-off or thin (keep as minimal stubs).
4. **Synthesize (T1/T2 only).** For each, write an evolution narrative (how the idea developed over
   time), the best articulation, related links, and counter-positions — each point cited.
5. **Cluster + map.** Group related concepts into domains; optionally write a `concepts/README.md`
   (or a topic index page) linking the map by tier and cluster.
6. **Write** with `put_page` (canonical concept pages + the index); reciprocal links between related
   concepts; confirm the slugs.

## Keep it scoped

Do this for the topic the user asked about, not the entire brain. If they want a full-brain
synthesis, say it's a larger batch job (deferred) and offer to do it cluster-by-cluster.
