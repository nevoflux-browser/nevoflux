---
name: brain
description: Entry point and shared conventions for the user's knowledge base (their second brain, backed by gbrain). Consult this FIRST for anything about the user's own saved knowledge, then route to brain-recall (look things up), brain-capture (save or ingest), brain-think (synthesize and analyze), or brain-care (health and recovery). Loads shared conventions and the gbrain tool map. Use whenever the user mentions their knowledge base, second brain, 知识库, 第二大脑, or asks about their own saved notes and research.
version: "1.0.0"
author: "NevoFlux"
tags: [brain, knowledge-base, gbrain, hub, conventions, routing]
enabled: true
triggers:
  - "/brain"
  - "my knowledge base"
  - "second brain"
  - "我的知识库"
  - "知识库"
  - "第二大脑"
allowed_tools:
  - tool_search
  - tool_call_dynamic
---

# brain — knowledge base hub

The **brain** is the user's long-term knowledge base: a collection of markdown pages persisted by
the **gbrain** backend. This skill is the *entry point* and the home of the *shared conventions*.
The actual work happens in four focused skills — this hub orients you and points you at the right
one.

/ 这是用户长期知识库（gbrain 后端）的入口与公共约定。真正的操作由下面四个技能完成。

## Consult the brain first

For any question about the user's **own** saved knowledge — a person/company/concept they've
researched, "what do I know about…", "我之前记过…", or anything that sounds like *recalling* rather
than *discovering* — check the brain before answering from scratch or from the web. Read
`skill_read('brain', 'conventions/brain-first.md')` for the full policy and the page model.

Do **not** use the brain for general world knowledge, live web facts, or the content of the page the
user is currently viewing — use web search / browser tools for those. The brain is *personal
memory*, not a search engine.

## Routing — pick the right skill

| The user wants to… | Load this skill | Examples |
| --- | --- | --- |
| **Look something up / recall** | `brain-recall` | "what do I know about X", "我的知识库里有", "what's notable lately", "catch me up" |
| **Save / ingest knowledge** | `brain-capture` | "save this", "记到知识库", "ingest this PDF/video", "process this meeting" |
| **Synthesize / analyze** | `brain-think` | "connect the dots", "who knows about X / 谁懂", "how has X trended", "build my concept map" |
| **Diagnose / sync / recover** | `brain-care` | "is my brain healthy", "what's inconsistent", "sync my brain", "undo that delete" |

`skill_load` the matching skill, then follow it. For the full intent→skill→tool dispatch table and
disambiguation rules, read `skill_read('brain', 'conventions/brain-routing.md')`.

## Reaching gbrain tools

The gbrain tools are **not** in your static catalog — discover them dynamically:

1. **Discover** — `tool_search` with a query like `brain`, `知识库`, `gbrain`, or the user's topic.
   This returns the gbrain tool schemas.
2. **Invoke** — `tool_call_dynamic` with the tool name + args.

If `tool_search` returns nothing for `brain`, retry once with `知识库` or `gbrain` before telling the
user the knowledge base is unavailable. The canonical, grouped tool map lives in
`skill_read('brain', 'gbrain-tools.md')`.

## Shared conventions index

All four skills pull these from the hub on demand (`skill_read('brain', '<path>')`). Read a file
only when you need it:

| File | Read when… |
| --- | --- |
| `gbrain-tools.md` | you need the full list of gbrain tools, grouped by intent + owner skill |
| `conventions/brain-first.md` | always — the page model (`compiled_truth`/`timeline`), slug rules, hybrid filing, tool access |
| `conventions/brain-routing.md` | disambiguating which skill/tool handles an intent |
| `conventions/quality.md` | writing pages — citations, reciprocal back-links, notability gate |
| `conventions/search-modes.md` | tuning a `query`/`search` — cost presets and per-call knobs |
| `conventions/salience-and-recency.md` | personal-state / "what's notable / 最近" questions |
| `conventions/calibration.md` | working with takes / bets / prediction track-records |

The brain is bilingual-friendly: queries and content may be English or 中文. Search with the user's
own terms, and always show the user the page **slug** you read from or wrote to — it's their filing
system.
