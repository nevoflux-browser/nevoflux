---
name: skill-creator
description: Create new skills, modify and improve existing skills, and measure skill performance. Use when users want to create a skill from scratch, edit or optimize an existing skill, run evals to test a skill, benchmark skill performance with variance analysis, or optimize a skill's description for better triggering accuracy. This applies to NevoFlux Agent skills, Claude Code skills, or any agent skill using the YAML frontmatter + Markdown format.
---

# Skill Creator for NevoFlux Agent

A skill for creating new skills and iteratively improving them. Works entirely without Python -- uses shell scripts for packaging/validation, and the agent's own LLM and subagent capabilities for evaluation and optimization.

At a high level, the process of creating a skill goes like this:

- Decide what you want the skill to do and roughly how it should do it
- Write a draft of the skill
- Create a few test prompts and run the agent-with-access-to-the-skill on them
- Help the user evaluate the results both qualitatively and quantitatively
  - While the runs happen, draft some quantitative evals if there aren't any. Then explain them to the user
  - Generate an HTML review page for the user to look at results, and also show quantitative metrics
- Rewrite the skill based on feedback from the user's evaluation of the results
- Repeat until you're satisfied
- Expand the test set and try again at larger scale

Your job when using this skill is to figure out where the user is in this process and then jump in and help them progress through these stages.

On the other hand, maybe they already have a draft of the skill. In this case you can go straight to the eval/iterate part of the loop.

Of course, you should always be flexible and if the user is like "I don't need to run a bunch of evaluations, just vibe with me", you can do that instead.

Then after the skill is done (but again, the order is flexible), you can also run the description optimizer to improve triggering of the skill.

## Communicating with the user

The skill creator is liable to be used by people across a wide range of familiarity with coding jargon. Pay attention to context cues to understand how to phrase your communication. In the default case:

- "evaluation" and "benchmark" are borderline, but OK
- for "JSON" and "assertion" you want to see serious cues from the user that they know what those things are before using them without explaining them

It's OK to briefly explain terms if you're in doubt.

---

## Creating a skill

### Capture Intent

Start by understanding the user's intent. The current conversation might already contain a workflow the user wants to capture (e.g., they say "turn this into a skill"). If so, extract answers from the conversation history first -- the tools used, the sequence of steps, corrections the user made, input/output formats observed. The user may need to fill the gaps, and should confirm before proceeding.

1. What should this skill enable the agent to do?
2. When should this skill trigger? (what user phrases/contexts)
3. What's the expected output format?
4. Should we set up test cases to verify the skill works? Skills with objectively verifiable outputs (file transforms, data extraction, code generation, fixed workflow steps) benefit from test cases. Skills with subjective outputs (writing style, art) often don't need them. Suggest the appropriate default based on the skill type, but let the user decide.

### Interview and Research

Proactively ask questions about edge cases, input/output formats, example files, success criteria, and dependencies. Wait to write test prompts until you've got this part ironed out.

If useful tools are available for research (searching docs, finding similar skills, looking up best practices), research in parallel via subagents if available, otherwise inline. Come prepared with context to reduce burden on the user.

### Write the SKILL.md

Based on the user interview, fill in the YAML frontmatter. Required fields are `name` and `description`; everything else is optional:

```yaml
---
name: my-skill                    # Required. kebab-case, max 64 chars
description: What it does and when to use it  # Required. Max 1024 chars. Primary trigger mechanism.
version: 1.0.0                    # Semantic version
tags:                             # Categorization keywords (used in listing)
  - keyword1
  - keyword2
enabled: true                     # Set false to disable without deleting
triggers:                         # Auto-suggest patterns (case-insensitive substring match)
  - phrase that should suggest this skill
allowed_tools:                    # Only inject skill if these tools are available (glob patterns)
  - tool_name
dependencies:                     # Other skills this depends on
  - other-skill
---
```

**Key field guidance:**

- **description**: The most important field for triggering. Include both what the skill does AND specific contexts for when to use it. Agents tend to "undertrigger" skills, so make descriptions a little "pushy". Instead of "How to build a simple fast dashboard.", write "How to build a simple fast dashboard to display data. Use this skill whenever the user mentions dashboards, data visualization, internal metrics, or wants to display any kind of data, even if they don't explicitly ask for a 'dashboard.'"
- **triggers**: Simple substring patterns matched against user messages. Use specific phrases, not generic ones like "create a" which match too broadly.
- **allowed_tools**: Glob patterns (e.g. `notion:*`, `stitch*`) that filter the skill based on tool availability. If the required tools are missing, the skill won't appear in the agent's skill list.

### Skill Writing Guide

#### Anatomy of a Skill

```
skill-name/
  SKILL.md (required)
    YAML frontmatter (name, description required)
    Markdown instructions
  Bundled Resources (optional)
    scripts/    - Executable code for deterministic/repetitive tasks
    references/ - Docs loaded into context as needed
    assets/     - Files used in output (templates, icons, fonts)
```

#### Progressive Disclosure

Skills use a three-level loading system. In NevoFlux, each level maps to a specific tool:

1. **Metadata** (name + description, ~100 words) — Always in context via `skill_list` host function. The agent sees all skill summaries in its system prompt as `- **name**: description`.
2. **SKILL.md body** (<500 lines ideal) — Loaded on demand when the agent calls `skill_load(name)`. This is triggered by the LLM deciding the skill is relevant to the user's request.
3. **Bundled resources** (unlimited) — Loaded on demand via `skill_read(name, file_path)`. Reference files, scripts, assets. Scripts can be executed via `skill_execute(name, script_path, args)` without loading into context.

This means: the **description** determines whether the skill gets loaded (Level 1 → 2), and **pointers in SKILL.md** determine which auxiliary files get read (Level 2 → 3).

**Key patterns:**
- Keep SKILL.md under 500 lines; if approaching this limit, move details to reference files with clear pointers like "load auxiliary file `references/advanced.md` for details"
- Reference files clearly from SKILL.md with guidance on when to read them
- For large reference files (>300 lines), include a table of contents

**Domain organization**: When a skill supports multiple domains/frameworks, organize by variant:
```
cloud-deploy/
  SKILL.md (workflow + selection)
  references/
    aws.md
    gcp.md
    azure.md
```
The agent reads only the relevant reference file.

#### Principle of Lack of Surprise

Skills must not contain malware, exploit code, or any content that could compromise system security. A skill's contents should not surprise the user in their intent if described. Don't go along with requests to create misleading skills or skills designed to facilitate unauthorized access, data exfiltration, or other malicious activities.

#### Writing Patterns

Prefer using the imperative form in instructions.

**Defining output formats** - You can do it like this:
```markdown
## Report structure
ALWAYS use this exact template:
# [Title]
## Executive summary
## Key findings
## Recommendations
```

**Examples pattern** - Include examples:
```markdown
## Commit message format
**Example 1:**
Input: Added user authentication with JWT tokens
Output: feat(auth): implement JWT-based authentication
```

### Writing Style

Try to explain to the model why things are important in lieu of heavy-handed MUSTs. Use theory of mind and try to make the skill general and not super-narrow to specific examples. Start by writing a draft and then look at it with fresh eyes and improve it.

### Skill Validation

Before proceeding to testing, validate the skill structure. Run the validation script bundled with this skill:

```bash
bash <this-skill-dir>/scripts/validate.sh <path-to-skill-directory>
```

This checks SKILL.md exists, frontmatter is valid YAML, name is kebab-case (max 64 chars), description is under 1024 chars, and no unexpected frontmatter keys are present.

### Test Cases

After writing the skill draft, come up with 2-3 realistic test prompts -- the kind of thing a real user would actually say. Share them with the user. Then run them.

Save test cases to `evals/evals.json`. Don't write assertions yet -- just the prompts. You'll draft assertions in the next step while the runs are in progress.

```json
{
  "skill_name": "example-skill",
  "evals": [
    {
      "id": 1,
      "prompt": "User's task prompt",
      "expected_output": "Description of expected result",
      "files": []
    }
  ]
}
```

See `references/schemas.md` for the full schema (including the `assertions` field).

## Running and evaluating test cases

This section is one continuous sequence -- don't stop partway through.

Put results in a `<skill-name>-workspace/` directory. Use the current working directory or `/tmp/` as the parent — do not create workspace directories inside the skill's config directory (e.g. `~/Library/Application Support/...` or `%APPDATA%\...`), as those are not appropriate for temporary build artifacts. Within the workspace, organize by iteration (`iteration-1/`, `iteration-2/`, etc.) and within that, each test case gets a directory (`eval-0/`, `eval-1/`, etc.). Create directories as you go.

### Step 1: Spawn all runs (with-skill AND baseline) in the same turn

For each test case, spawn two subagents in the same turn -- one with the skill, one without. Launch everything at once so it all finishes around the same time.

**With-skill run:**

Use `subagent_spawn` to create a subagent in "agent" mode with a prompt like:

```
Execute this task:
- First, load and read the skill at: <path-to-skill>/SKILL.md
- Task: <eval prompt>
- Input files: <eval files if any, or "none">
- Save outputs to: <workspace>/iteration-<N>/eval-<ID>/with_skill/outputs/
- Outputs to save: <what the user cares about>
```

**Baseline run** (same prompt, but the baseline depends on context):
- **Creating a new skill**: no skill at all. Same prompt, no skill path, save to `without_skill/outputs/`.
- **Improving an existing skill**: the old version. Before editing, snapshot the skill (`cp -r <skill-path> <workspace>/skill-snapshot/`), then point the baseline subagent at the snapshot. Save to `old_skill/outputs/`.

Write an `eval_metadata.json` for each test case (assertions can be empty for now). Give each eval a descriptive name based on what it's testing.

```json
{
  "eval_id": 0,
  "eval_name": "descriptive-name-here",
  "prompt": "The user's task prompt",
  "assertions": []
}
```

### Step 2: While runs are in progress, draft assertions

Don't just wait for the runs to finish -- use this time productively. Draft quantitative assertions for each test case and explain them to the user.

Good assertions are objectively verifiable and have descriptive names. Subjective skills (writing style, design quality) are better evaluated qualitatively -- don't force assertions onto things that need human judgment.

Update the `eval_metadata.json` files and `evals/evals.json` with the assertions once drafted.

### Step 3: As runs complete, capture timing data

When each subagent completes, save timing data to `timing.json` in the run directory:

```json
{
  "total_tokens": 84852,
  "duration_ms": 23332,
  "total_duration_seconds": 23.3
}
```

### Step 4: Grade, aggregate, and generate the review page

Once all runs are done:

1. **Grade each run** -- spawn a grader subagent (or grade inline) that reads `agents/grader.md` from this skill's directory and evaluates each assertion against the outputs. Save results to `grading.json` in each run directory. The grading.json expectations array must use the fields `text`, `passed`, and `evidence`. For assertions that can be checked programmatically, write and run a shell script rather than eyeballing it.

2. **Aggregate into benchmark** -- read all `grading.json` and `timing.json` files across the iteration, compute aggregate statistics (mean, stddev, min, max for pass_rate, time_seconds, tokens), and write `benchmark.json` and `benchmark.md`. See `references/schemas.md` for the exact schema. Put each with_skill version before its baseline counterpart.

   To compute these aggregates inline:
   - For each configuration (with_skill, without_skill), collect all pass_rate, time, token values
   - Calculate: mean = sum/count, stddev = sqrt(sum((x-mean)^2)/count), min, max
   - Delta = with_skill.mean - without_skill.mean (format as "+X.XX" or "-X.XX")

3. **Do an analyst pass** -- read the benchmark data and surface patterns the aggregate stats might hide. Read `agents/analyzer.md` for what to look for -- things like assertions that always pass regardless of skill, high-variance evals, and time/token tradeoffs.

4. **Generate the HTML review page** -- create a standalone HTML file that lets the user review results. Write it to `<workspace>/iteration-N/review.html`. The page should include:
   - A tab or section for each test case showing: prompt, output files (inline where possible), grades (pass/fail for each assertion), and a feedback textbox
   - A benchmark summary section with pass rates, timing, token usage
   - A "Submit All Reviews" button that downloads `feedback.json`

   For iteration 2+, also show previous iteration's output (collapsed) and previous feedback.

   Then open it: `open <workspace>/iteration-N/review.html` (macOS) or `xdg-open` (Linux).

5. **Tell the user** something like: "I've generated the results page. There are two sections -- 'Outputs' lets you click through each test case and leave feedback, 'Benchmark' shows the quantitative comparison. When you're done, come back here and let me know."

### What the user sees in the review page

The "Outputs" section shows one test case at a time:
- **Prompt**: the task that was given
- **Output**: the files the skill produced, rendered inline where possible
- **Previous Output** (iteration 2+): collapsed section showing last iteration's output
- **Formal Grades** (if grading was run): collapsed section showing assertion pass/fail
- **Feedback**: a textbox that auto-saves as they type
- **Previous Feedback** (iteration 2+): their comments from last time

The "Benchmark" section shows the stats summary: pass rates, timing, and token usage for each configuration, with per-eval breakdowns and analyst observations.

Navigation is via prev/next buttons or arrow keys. When done, they click "Submit All Reviews" which downloads `feedback.json`.

### Step 5: Read the feedback

When the user tells you they're done, read `feedback.json`:

```json
{
  "reviews": [
    {"run_id": "eval-0-with_skill", "feedback": "the chart is missing axis labels", "timestamp": "..."},
    {"run_id": "eval-1-with_skill", "feedback": "", "timestamp": "..."}
  ],
  "status": "complete"
}
```

Empty feedback means the user thought it was fine. Focus improvements on the test cases where the user had specific complaints.

---

## Improving the skill

This is the heart of the loop.

### How to think about improvements

1. **Generalize from the feedback.** We're trying to create skills that can be used many times across many different prompts. Rather than put in fiddly overfitty changes, or oppressively constrictive MUSTs, if there's some stubborn issue, try branching out and using different metaphors or recommending different patterns of working.

2. **Keep the prompt lean.** Remove things that aren't pulling their weight. Read the transcripts, not just the final outputs -- if the skill is making the model waste time doing unproductive things, get rid of those parts.

3. **Explain the why.** Try hard to explain the **why** behind everything. Today's LLMs are smart. They have good theory of mind and when given a good harness can go beyond rote instructions. If you find yourself writing ALWAYS or NEVER in all caps, reframe and explain the reasoning so the model understands why.

4. **Look for repeated work across test cases.** If all test cases resulted in the subagent writing a similar helper script, that's a strong signal the skill should bundle that script. Write it once, put it in `scripts/`, and tell the skill to use it.

### The iteration loop

After improving the skill:

1. Apply your improvements to the skill
2. Rerun all test cases into a new `iteration-<N+1>/` directory, including baseline runs
3. Generate the review page
4. Wait for the user to review
5. Read the new feedback, improve again, repeat

Keep going until:
- The user says they're happy
- The feedback is all empty (everything looks good)
- You're not making meaningful progress

---

## Advanced: Blind comparison

For situations where you want a more rigorous comparison between two versions of a skill, there's a blind comparison system. Read `agents/comparator.md` and `agents/analyzer.md` for the details. The basic idea is: give two outputs to an independent subagent without telling it which is which, and let it judge quality.

This is optional and most users won't need it.

---

## Description Optimization

The description field in SKILL.md frontmatter is the primary mechanism that determines whether the agent invokes a skill. After creating or improving a skill, offer to optimize the description for better triggering accuracy.

Unlike the original Claude Code skill-creator which relies on `claude -p` subprocess calls, this version uses the agent's own LLM capabilities directly. No Python required.

### Step 1: Generate trigger eval queries

Create 20 eval queries -- a mix of should-trigger and should-not-trigger. Save as JSON:

```json
[
  {"query": "the user prompt", "should_trigger": true},
  {"query": "another prompt", "should_trigger": false}
]
```

The queries must be realistic and something a user would actually type. Not abstract requests, but concrete and specific with detail -- file paths, personal context, column names, company names. Some might be in lowercase or contain abbreviations or typos. Use a mix of different lengths, and focus on edge cases rather than clear-cut.

Bad: `"Format this data"`, `"Extract text from PDF"`, `"Create a chart"`

Good: `"ok so my boss just sent me this xlsx file (its in my downloads, called something like 'Q4 sales final FINAL v2.xlsx') and she wants me to add a column that shows the profit margin as a percentage. The revenue is in column C and costs are in column D i think"`

For the **should-trigger** queries (8-10), think about coverage. Different phrasings of the same intent -- some formal, some casual. Include cases where the user doesn't explicitly name the skill but clearly needs it.

For the **should-not-trigger** queries (8-10), the most valuable ones are the near-misses -- queries that share keywords but actually need something different. Don't make them obviously irrelevant.

### Step 2: Review with user

Present the eval set to the user for review using the HTML template:

1. Read the template from `assets/eval_review.html` in this skill's directory
2. Replace the placeholders:
   - `__EVAL_DATA_PLACEHOLDER__` with the JSON array of eval items (no quotes -- it's a JS variable assignment)
   - `__SKILL_NAME_PLACEHOLDER__` with the skill's name
   - `__SKILL_DESCRIPTION_PLACEHOLDER__` with the skill's current description
3. Write to a temp file (e.g., `/tmp/eval_review_<skill-name>.html`) and open it
4. The user can edit queries, toggle should-trigger, add/remove entries, then click "Export Eval Set"
5. The file downloads to `~/Downloads/eval_set.json`

### Step 3: Run the optimization loop

This is where NevoFlux's approach differs from Claude Code's. Instead of calling an external Python script that invokes `claude -p`, you will orchestrate the optimization loop directly using your own LLM capabilities.

Tell the user: "I'll run the optimization loop now. This involves multiple iterations of testing and improving the description."

**The optimization algorithm:**

1. **Split the eval set**: 60% train, 40% test. Stratify by should_trigger (shuffle each group separately, take the first N for test).

2. **For each iteration** (up to 5):

   a. **Evaluate current description against ALL queries (train + test)**:
      For each query, determine if the current skill description would cause the agent to trigger the skill. To test this, ask yourself: "Given this query and this skill description appearing in my available_skills list, would I invoke this skill?" Run each query 3 times mentally and record the trigger rate. Be honest and simulate a fresh context each time.

      Score each query:
      - `trigger_rate` = (times triggered) / (total runs)
      - `pass` = true if (should_trigger AND trigger_rate >= 0.5) OR (NOT should_trigger AND trigger_rate < 0.5)

   b. **Check train results**: If all train queries pass, stop early.

   c. **Generate improved description**:
      Analyze the failures on the TRAIN set only (don't look at test scores when improving). Identify:
      - Failed-to-trigger queries: what intent/keywords are missing from the description?
      - False-trigger queries: what's too broad in the description?

      Then write a new description that:
      - Uses imperative language ("Use this skill for...")
      - Focuses on user intent, not implementation details
      - Is distinctive and immediately recognizable
      - Stays under 1024 characters (hard limit), ideally 100-200 words
      - Does NOT overfit to specific test queries -- generalize from failures to broader categories
      - Is structurally different from previous attempts

   d. **Record the iteration**: Save description, train scores, test scores.

3. **Select the best description** by TEST score (not train, to avoid overfitting).

4. **Present results** to the user: show each iteration's description with train/test scores, and recommend the best one.

### Step 4: Apply the result

Take the best description and update the skill's SKILL.md frontmatter. Show the user before/after and report the scores.

---

## Packaging

When the skill is ready to distribute, package it using the bundled shell script:

```bash
bash <this-skill-dir>/scripts/package.sh <path-to-skill-directory> [output-directory]
```

This creates a `.skill` file (zip format) excluding `__pycache__`, `node_modules`, `*.pyc`, `.DS_Store`, and the `evals/` directory.

**Windows note:** `validate.sh` and `package.sh` require a bash-compatible shell. On Windows, run them through WSL, Git Bash, or MSYS2. If none are available, perform validation manually (check SKILL.md exists, frontmatter has `name` and `description`, name is kebab-case) and use `zip` or a file archiver to create the `.skill` package.

---

## NevoFlux-Specific Notes

### Tool names

NevoFlux Agent uses these tool names (different from Claude Code):
- `read_file` (not `Read`)
- `write_file` (not `Write`)
- `list_files` (not `Glob`)
- `run_command` (not `Bash`)
- `skill_load` (not `Skill`)
- `subagent_spawn` + `subagent_wait_all` (not `Agent`)

When writing skills for NevoFlux, use these tool names in examples and instructions.

### Skill directories

**Where to save new skills** (platform-specific primary directory):

| Platform | Primary skill directory |
|----------|----------------------|
| **Linux** | `~/.config/nevoflux/skills/` |
| **macOS** | `~/Library/Application Support/nevoflux/skills/` |
| **Windows** | `%APPDATA%\nevoflux\skills\` |

When creating a new skill, always save it to the primary directory for the current platform. Detect the OS and use the correct path — do not hardcode `~/.config/nevoflux/skills/` on macOS or Windows.

**Additional directories** (also scanned, later directories override earlier):
- `~/.claude/skills/` (Claude Code compatible)
- `~/.gemini/skills/` (Gemini compatible)
- `~/.config/opencode/skills/` (OpenCode compatible)
- `~/.config/goose/skills/` (Goose compatible)

Skills are compatible with all these agents if they use the standard YAML frontmatter + Markdown format.

### Subagent modes

NevoFlux supports three subagent modes:
- `chat` - Dialogue mode
- `browser` - Browser automation mode
- `agent` - Full capabilities (file I/O, shell, computer use)

For skill testing, always use `agent` mode.

### Cross-agent compatibility

The SKILL.md format (YAML frontmatter + Markdown body) is a shared standard. Skills created with this tool are compatible with:
- NevoFlux Agent
- Claude Code (via `.claude/skills/`)
- Any agent that supports the Agent Skills specification

If you want maximum compatibility, avoid referencing NevoFlux-specific tool names in the skill body. Instead use generic descriptions like "read the file" or "execute the command".

---

## Reference files

The agents/ directory contains instructions for specialized subagents. Read them when you need to spawn the relevant subagent.

- `agents/grader.md` -- How to evaluate assertions against outputs
- `agents/comparator.md` -- How to do blind A/B comparison between two outputs
- `agents/analyzer.md` -- How to analyze why one version beat another

The references/ directory has additional documentation:
- `references/schemas.md` -- JSON structures for evals.json, grading.json, etc.

---

Repeating one more time the core loop here for emphasis:

- Figure out what the skill is about
- Draft or edit the skill
- Validate with `scripts/validate.sh`
- Run the agent-with-access-to-the-skill on test prompts via subagents
- With the user, evaluate the outputs:
  - Create benchmark.json and generate a review HTML page
  - Run quantitative evals
- Repeat until you and the user are satisfied
- Package the final skill with `scripts/package.sh` and return it to the user

Good luck!
