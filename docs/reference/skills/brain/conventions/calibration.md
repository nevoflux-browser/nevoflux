# Convention: calibration

The **takes** model — how the brain tracks claims and prediction accuracy. Adapted from gbrain's
calibration convention. Read this when working with track-records, bets, or "am I well-calibrated"
questions (brain-think). For the NevoFlux client this is **read-only**: report track-records, don't
auto-resolve bets.

## Takes: typed, weighted, attributed claims

A **take** is a claim with a kind, a confidence weight, and a holder:

- **kind** — one of:
  - `fact` — asserted as true (no probability)
  - `take` — an opinion / interpretation
  - `bet` — a falsifiable prediction with a confidence weight (the calibration unit)
  - `hunch` — a low-confidence guess
- **weight** — confidence in `(0, 1]` (mainly meaningful for `bet`).
- **holder** — whose claim it is: `world` | `garry` (the user) | `brain` | a page slug (e.g.
  `people/<x>`).

List/search with `takes_list` (filter by holder/kind/active/resolved) and `takes_search` (keyword
over claim text).

## Scoring resolved bets

Only `bet`s that have been resolved (correct ∨ incorrect) score:

- **`takes_scorecard`** — counts, accuracy, **Brier score** (lower is better), partial_rate. Scope
  by `holder`, `domain_prefix` (e.g. `companies/`), and a `since`/`until` window.
- **`takes_calibration`** — calibration curve: resolved bets binned by stated weight, showing
  observed vs predicted accuracy per bucket (well-calibrated = observed ≈ predicted).
- **`get_calibration_profile`** — the active profile for a holder: Brier, accuracy, pattern
  statements, active bias tags. Returns null on a cold brain (builds after 5+ resolved bets).

## Conversational framing

Speak track-records in plain language, not academese: "2 of your last 3 calls on X missed" beats
"Brier 0.41". Surface *patterns* ("you tend to be overconfident on timelines") when the profile
provides them. Cite the takes/pages behind a claim.

## Boundaries (client)

- **Read-only.** Report scorecards, curves, and profiles. Do **not** auto-resolve bets or apply
  verdicts — auto-resolve is a gbrain server concern, disabled by default.
- "unresolvable" verdicts never auto-apply, even at confidence 1.0.
