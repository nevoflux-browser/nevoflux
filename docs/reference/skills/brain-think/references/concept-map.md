# Reference: concept map (persist on request)

Build or refresh an intellectual map for a **topic/cluster** the user names. Topic-scoped, not a
whole-brain sweep (that batch job is deferred). Writes pages — apply `conventions/quality.md`.

## Workflow

1. **Gather the cluster.** `query`/`list_pages` to pull the concept stubs/pages around the topic
   (e.g. `concepts/` pages matching the theme, or pages tagged accordingly).
2. **Dedupe + merge.** Collapse near-duplicate stubs into canonical concepts using: Jaccard overlap
   on title + first paragraph; substring containment ("founder mode" inside "founder mode vs manager
   mode"); and a semantic pass ("are these the same idea?"). Pick one canonical slug; preserve other
   titles as `aliases` in frontmatter. Spot-check that the count actually dropped.
3. **Score + tier** each concept by frequency (distinct sources), timespan (first→last mention),
   breadth (distinct months):

   | Tier | Label | Threshold | Output |
   | --- | --- | --- | --- |
   | T1 | Canon | ≥6 mentions, ≥4 months span | full synthesis |
   | T2 | Developing | 3–5 mentions, 1–3 months | full synthesis |
   | T3 | Speculative | 2–3 mentions, short span | stub only |
   | T4 | Riff | 1 mention | stub only |

   Under 4 months span isn't T1; over 3 months isn't T4.
4. **Synthesize (T1/T2 only).** For each, write an evolution narrative (how the idea developed over
   time), the best articulation, related links, and counter-positions — each point cited.
5. **Cluster + map.** Group related concepts into domains; optionally write a `concepts/README.md`
   (or a topic index page) linking the map by tier and cluster.
6. **Write** with `put_page` (canonical concept pages + the index); reciprocal links between related
   concepts; confirm the slugs.

## T1/T2 page template

````markdown
---
title: <concept>
type: concept
tier: 1            # 1 or 2
mention_count: <N>
distinct_months: <M>
first_mention: <YYYY-MM-DD>
last_mention: <YYYY-MM-DD>
aliases: [<other titles>]
related: [<concept slugs>]
---

# <concept>

**Tier 1 — Canon** | <N> mentions across <M> months

## Synthesis
<2–4 paragraph analytical narrative — how the idea sharpened, not repetition>

## Best articulation
> "<verbatim quote>" — [<date>](<source-url>)

## Evolution
| Period | Expression | Signal |

## Related concepts
- [<concept>](<slug>) — <relationship>

## Timeline
<deduped dated entries with quotes + links, newest first>
````

T3/T4 are a stub: frontmatter + one verbatim quote with its date/link. Master map at
`concepts/README.md` groups concepts by tier and named cluster.

## Anti-patterns

- Don't synthesize T3/T4 — they're ephemeral; a stub + quote is enough.
- Don't hallucinate quotes or dates — every quote/date must verify against a brain page.
- Don't use generic cluster names ("Various Topics") — if you can't name it, the cluster isn't real.
- Don't re-synthesize an already-synthesized T1 unless new source material arrived (be idempotent).
- Don't link to concept pages that don't exist in "Related concepts".
- Don't paraphrase — use verbatim quotes; the user's exact wording is the signal.

## Keep it scoped

Do this for the topic the user asked about, not the entire brain. If they want a full-brain
synthesis, say it's a larger batch job (deferred) and offer to do it cluster-by-cluster.
