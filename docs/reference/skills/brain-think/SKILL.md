---
name: brain-think
description: Synthesize and analyze across the user's brain (gbrain) — multi-hop cited answers (think), expertise routing (who is knowledgeable about a topic, who to talk to), entity trajectories and trends, relationship-graph traversal, and prediction calibration (takes, bets, Brier scores). Persist syntheses on request (concept maps, strategic-reading playbooks). Use for connect the dots, who knows about, who should I talk to about, how has X trended, what is my track record, build my concept map, read this through the lens of, 谁懂, 趋势, 帮我梳理. For analysis and synthesis — not simple lookups (use brain-recall) and not saving provided content (use brain-capture).
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, think, synthesis, calibration, experts]
enabled: true
triggers:
  - "connect the dots"
  - "who knows about"
  - "who should I talk to"
  - "how has"
  - "track record"
  - "build my concept map"
  - "read this through the lens of"
  - "谁懂"
  - "趋势"
  - "帮我梳理"
allowed_tools:
  - tool_search
  - tool_call_dynamic
---

# brain-think — synthesize & analyze

Reason *across* the brain: multi-hop synthesis, expertise routing, trends, relationships, and
prediction calibration. Answers by default; **persists a synthesis only when the user asks**. Load
`skill_read('brain', 'conventions/brain-first.md')` for the page model; for calibration work also
`skill_read('brain', 'conventions/calibration.md')`; when persisting, `conventions/quality.md`.
Reach gbrain via `tool_search` + `tool_call_dynamic`.

## Answer (default — read-only)

| The user is asking… | Use | See |
| --- | --- | --- |
| "connect the dots / what's the pattern / 帮我梳理" (multi-hop, conflict, gap) | `think` | `references/think-usage.md` |
| "who knows about X / who should I talk to / 谁懂 X" | `find_experts` | — |
| "how has X trended / is X consistent / X 的趋势" | `find_trajectory` | — |
| "how are these related" (open / deep multi-hop) | `traverse_graph` (deep; shallow typed lookups like "who works at X" are `brain-recall`) | — |
| "what's my track record / am I well-calibrated" | `takes_scorecard` / `takes_calibration` / `get_calibration_profile` | `references/takes-and-calibration.md` |

Use `think` for genuinely *analytical* questions — synthesis, conflicts, gaps. A simple lookup
("what do I know about X") is cheaper in `brain-recall`; don't reach for `think` there.

## Persist on request

When the user asks to **build an artifact** from the analysis, write it via `put_page` using the hub
conventions:

- "**build / synthesize my concept map**" / "整理我的概念" → `references/concept-map.md`
  (topic-scoped; writes `concepts/`).
- "**read X through the lens of my problem Y**" / "用这个思路分析我的问题" →
  `references/strategic-reading.md` (writes a cited playbook to `concepts/` or `projects/`).

Default is answer-only. Don't write unless the user wants the artifact saved.

## Boundaries

- Persisting reuses `brain-capture`'s standards (page model + `conventions/quality.md`) — cite
  sources, reciprocal links, notability.
- Calibration is **read-only**: report scorecards/curves/profiles; never auto-resolve bets.
- Whole-brain batch concept-synthesis (thousands of stubs) is deferred — keep concept maps
  topic-scoped.
