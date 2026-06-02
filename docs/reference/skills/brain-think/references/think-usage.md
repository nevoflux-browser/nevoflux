# Reference: using `think`

`think` is gbrain's multi-hop synthesizer: it pulls relevant evidence across pages + takes + graph
and produces a **cited** answer with conflict and gap analysis. It's heavier than recall — reach for
it when the question needs reasoning, not just a lookup.

## When to use it (vs recall)

- **Use `think`**: "connect the dots", "what's the relationship between X and Y", "what's the
  pattern across my notes on Z", "where do my notes conflict on this", "what's missing".
- **Don't use `think`**: "what do I know about X", "find my note on Y" — those are direct lookups;
  `brain-recall` (`query`/`get_page`) is cheaper and faster.

## Parameters

- `question` (required) — the analytical question, in natural language.
- `anchor` — a slug to pull the entity subgraph around (focuses the synthesis on one entity's
  neighborhood).
- `since` / `until` — temporal window (YYYY-MM-DD or YYYY-MM) to scope the evidence.
- `rounds` — multi-pass depth (default 1).
- `model` — override the reasoning model (alias or full id) when needed.

```
think(question="How do my notes on Acme's pricing and their churn relate?", anchor="companies/acme-corp")
```

## Reading the result

- Lead with `think`'s synthesized answer; surface its **citations** (slugs) so the user can verify.
- Call out the **conflicts** and **gaps** it reports — these are often the most useful part ("your
  2026-03 note says X but 2026-05 says Y"; "no evidence on Z").
- If the user wants the synthesis **saved**, that's the persist-on-request path (`concept-map.md` or
  `strategic-reading.md`) — `think` itself answers; saving is a separate `put_page`.

> Note: `think`'s own `save` option is local-CLI-only and ignored over MCP — persist by writing a
> page with `put_page`, not by relying on `save`.
