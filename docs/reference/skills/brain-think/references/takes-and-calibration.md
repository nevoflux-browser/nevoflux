# Reference: takes & calibration

Answer "what's my track record / am I well-calibrated / what have I claimed about X" using the takes
model. The model itself is in `skill_read('brain', 'conventions/calibration.md')`; this file is about
*which tool* answers *which question*. Read-only — report, never auto-resolve.

## Listing & searching claims

- `takes_list(holder?, kind?, active?, resolved?, page_slug?, sort_by?)` — list typed claims
  (`fact|take|bet|hunch`). E.g. unresolved bets by the user: `takes_list(holder="garry", kind="bet",
  resolved=false)`.
- `takes_search(query)` — keyword search across claim text ("what have I said about runway?").

## Track record (resolved bets only)

- `takes_scorecard(holder?, domain_prefix?, since?, until?)` — counts, accuracy, **Brier** (lower is
  better), partial_rate. Scope by domain: `takes_scorecard(domain_prefix="companies/")`.
- `takes_calibration(holder?, bucket_size?)` — calibration curve: resolved bets binned by stated
  weight, observed vs predicted per bucket. Well-calibrated ⇒ observed ≈ predicted.
- `get_calibration_profile(holder?)` — active profile: Brier, accuracy, pattern statements, bias
  tags. Returns null on a cold brain (builds after 5+ resolved bets).

## Answering well

- Speak it plainly: "2 of your last 3 calls on timelines missed — you tend to be optimistic there",
  not "Brier 0.41". Use the profile's pattern statements when present.
- Cite the takes/pages behind the numbers (slugs).
- If there aren't enough resolved bets yet, say so (the profile is null) rather than over-reading
  noise.

## Boundary

Client-side calibration is **read-only**. Don't resolve bets or apply verdicts — that's a gbrain
server concern (auto-resolve is off by default; "unresolvable" never auto-applies).
