# Record & Replay (browser use)

This file is the detailed companion to the **Record & Replay** section in
`SKILL.md`. Read it when you are turning a browser recording into a skill, or
when you need the trace schema and the replay tool mapping.

Scope today is **browser use only** — interactions inside web pages plus
navigation. Full computer use (OS-level windows, native apps) is out of scope;
do not author recorded skills against computer-use tools.

---

## Where the recording comes from

The user demonstrates a workflow by driving the browser themselves. A passive
recorder in the page actor observes their real interactions (it never blocks
them) and a daemon-side collector writes a normalized, ordered, lossless trace.

You drive the recording yourself from inside the skill-creator session, in
agent mode. `start_recording` (with a one-line `goal_hint`) arms the recorder on
the active tab and returns `{ recording_id, trace_path }`; you then tell the
user to demonstrate the workflow, and when they say they are done you call
`stop_recording` with the `recording_id`. `trace_path` is the **absolute path**
to the recording — a JSONL file at `{recording_id}.jsonl` — which you read
directly (no further path resolution needed).

Read the whole file with `read_file`. It is **NDJSON**: the first line is a
`header` record and every other line is a normalized `step`. **Sort the step
lines by `ts_ms`** before using them — order in the file is arrival order, which
is not guaranteed to be event order (see Trace schema). There is **no separate
`detected_inputs` record**; derive the candidate inputs yourself from the step
lines' `input_ref`. Treat the recording as the raw material for **Capture
Intent** — it is a demonstrated workflow, exactly the case the Capture Intent
step was written for.

---

## Trace schema

The file is **NDJSON**, not a single object. The first line is a `header`;
every other line is a `step`.

`header` (first line):

| Field | Meaning |
| --- | --- |
| `type` | `"header"` |
| `recording_id` | Stable id; also the filename and the skill's working handle |
| `created_at` | Epoch ms when recording started |
| `start_url` | Where the demonstration began |
| `goal_hint` | The user's own one-line description of the task |

Each `step` line:

| Field | Meaning |
| --- | --- |
| `i` | Step index |
| `action` | `navigate` \| `click` \| `fill` \| `select` \| `scroll` |
| `target` | Element descriptor (omitted for `navigate` / `scroll`) |
| `target.role` | Accessible role, e.g. `button`, `textbox` |
| `target.name` | Accessible name |
| `target.text` | Trimmed text content (≤200 chars) — optional; today the snapshot captures only `name` |
| `target.tag` | Lowercased tag name |
| `target.landmark` | Nearest landmark `role "name"` (e.g. `region "Billing"`) — used to disambiguate |
| `target.selectors[]` | **Ranked** locators `{type, strategy, value}` — durable first |
| `target.element_kind` | Present only for special cases: `select`, `file` |
| `value` | Observed value for `fill`; `null` if redacted |
| `redacted` | `true` when the value was a secret (password / token-shaped) |
| `input_ref` | Candidate placeholder like `{{email}}` if this value may vary |
| `url` / `title` | Page context when the step happened |
| `ts_ms` | **Event-time** timestamp — sort steps by this; file/arrival order ≠ event order |
| `wait_after` | `navigation` \| `interaction` \| `scroll` hint for inserting a wait |

There is **no `detected_inputs` record**. Build the candidate list yourself by
scanning the step lines — each candidate is
`{ ref: input_ref, from_step: i, sample: value, secret: redacted }`. These are
candidates only; the user confirms which are real (decision 2).

The `selectors[]` array is ranked by durability: `role` (a11y / aria) →
`aria-label` → `placeholder` → `label` → `testid` → stable `id` → CSS path →
last-resort attribute. **Prefer the top entries.** A bare CSS path is a
fallback, not a first choice.

> The recorder strips its ephemeral per-snapshot ids (`e0`, `e1`, …). They are
> never in the trace and must never end up in a skill — they are reassigned on
> every snapshot, so a skill that hardcodes one breaks on the next run.

---

## From recording to skill

This is Capture Intent, specialized for a recording.

1. **Read the trace.** Summarize the workflow back to the user in plain language
   from `goal_hint` + the step sequence, so they can confirm you understood the
   demonstration.

2. **Confirm the variables.** The recorder only *guesses* which values vary
   (the `input_ref` on step lines). You decide nothing on its behalf — present
   the candidates and let the user confirm which are real inputs and which are
   fixed. A value typed once is not automatically a parameter; a fixed login URL
   often is not. Secrets (`redacted: true`) become required inputs the user
   supplies at replay, never baked into the skill.

3. **Generalize, don't transcribe.** Replace confirmed variable values with
   named placeholders (`{{query}}`, `{{file}}`). Write the steps as
   *instructions the agent follows with browser tools*, not a fixed macro of
   selectors. The point is a reusable skill that survives DOM drift, the same
   way the rest of this skill-creator works — the agent re-locates elements
   live at replay using the durable `role` + `name` you carried over.

4. **Write the SKILL.md steps** using the replay patterns below.

---

## Replay step patterns

Author steps against these tools. The recorded `selectors[]` and `role`/`name`
are what you write into the instructions so the agent can relocate live.

| Trace action | Author the step as | Notes |
| --- | --- | --- |
| `navigate` | `browser_navigate` | Use the recorded URL, or a `{{url}}` placeholder if it varies |
| `click` | relocate ladder (see below) → `browser_click_by_id` | Snapshot ids are fresh each run; relocate, don't hardcode. Prefer the top durable `selectors[]` entry; fall back to `role`+`name`; narrow ambiguous matches by `text` / `landmark` |
| `fill` / `type` | **`browser_input`** with `mode: "fill"` (replace) or `"type"` (append), `verify: true` | This is the default for all text input, including contentEditable and rich editors (X / LinkedIn / ProseMirror / Lexical / Draft / Slate). Do **not** author against `browser_fill` / `browser_type` / `*_by_id` — they are deprecated (2026-04) and silently no-op on many editors |
| `select` (`element_kind: select`) | `browser_click` to open, then `browser_click` the option (by recorded option text) | Native `<select>` is **not** a `browser_input` target |
| `file` (`element_kind: file`, value forced to `{{file}}`) | `browser_upload_file` | File inputs are **explicitly excluded** from `browser_input` and `browser_fill`. Always a required input; the local path is never recorded |
| `scroll` | `browser_scroll` | Often droppable if it was just reading; keep only when it reveals a target |

Between steps, insert `browser_wait_for` where the trace marked `wait_after`
(navigation after a click, interaction settling). This is what keeps replay from
racing ahead of the page.

### Relocate-then-act, every time

A recorded selector is a hint, not a guarantee, and `role`+`name` is often **not
unique** (many "Delete" rows, several "Search" boxes, paginated "Next").
Uniqueness is settled at **generation time**: when you author a step, produce an
identity that uniquely matched the recorded element; if even the durable
selectors cannot make it unique, **flag it to the user** during Capture Intent
rather than baking a fragile ordinal. At replay the agent walks this ladder on
each interactive step:

```
browser_get_elements                                  # fresh snapshot
# 1. try the top durable selectors[] entry (aria-label / testid / stable id / label)
# 2. else browser_find_elements role="button" name="Sign in"
# 3. still >1 → narrow by target.text, then target.landmark, then ordinal (last resort)
browser_click_by_id e<fresh>                          # act on the id from THIS snapshot
# 4. verify after acting (browser_input verify, or a post-step assertion)
```

Write the skill so the agent does this on each interactive step rather than
trusting a stored id or a brittle CSS path.

### Verification is free — use it

`browser_input` reads the content back when `verify: true` (the default) and
reports match / mismatch. Tell the skill to check that signal before moving on,
especially on contentEditable editors where a fill can "succeed" while inserting
nothing. This is the cheapest replay-robustness you get.

---

## What `browser_input` does not cover

State these as explicit branches in the skill so it never misroutes:

- **Native `<select>`** — open and click the option; `browser_input` only
  handles input / textarea / contentEditable.
- **File inputs** — `browser_upload_file` only; the schema forbids `browser_fill`
  and `browser_input` on them.
- **Submit / Enter on a generic site** — `browser_input` has no submit
  parameter, and its internal `SendKey(Enter)` only fires when a platform
  adapter recipe matches a known host. The adapter registry is currently empty,
  so on a generic page `browser_input` fills but does not submit. Author an
  explicit submit step (`browser_click` on the submit control). A page that
  submits *only* on raw Enter with no clickable control is a genuine gap — flag
  it to the user rather than guessing.

---

## allowed_tools and replay mode

In the generated skill's frontmatter, declare the browser tools it needs in
`allowed_tools` (e.g. `browser_*`) so the skill is only injected when those
tools are present. A recorded skill is useless in a text-only run.

Remember the mode is fixed when a run starts — the tool catalog comes from
`get_tools_for_mode` at launch and cannot be escalated mid-run. So replay (and
the eval runs that test it) must **start** in a mode that already includes the
browser tools, not switch into one partway through.

---

## Test the replay

Use the normal eval loop from `SKILL.md`. The only specialization:

- Spawn the with-skill and baseline subagents in a mode that carries the
  browser tools, since replay needs them from the first turn.
- Good assertions for recorded skills are concrete end-states: the expected URL
  was reached, the confirmation text appeared, the record was created. Prefer
  checking the *outcome* of the workflow over asserting that a particular
  selector was used — the whole point is that the agent may take a slightly
  different path to the same result.
- Realistic inputs, no secrets, short complete demonstrations. The same advice
  you give users about recording applies to the test prompts you write.
