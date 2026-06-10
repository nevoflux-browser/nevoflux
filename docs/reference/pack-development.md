<!--
   - This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/.
   -->

# Developing a NevoFlux Pack

> Protocol: `pack-protocol/0.1` · Audience: pack authors (first- and third-party)

A **pack** is a bundle of files dropped into NevoFlux's existing extension points —
skills, canvas-tools, seed knowledge-base pages, and a Canvas dashboard — plus a single
`pack.toml` manifest that declares *what* to install. The platform owns *how* to install
and, crucially, *how to cleanly take it back out*. You only write declarations + files;
the daemon handles transactional install/uninstall, rollback, an install receipt, and a
hard guarantee that **uninstall never deletes the user's data by default**.

You do **not** write installer code. A pack is "a set of files + a manifest."

---

## 1. Mental model

```
your-pack/
├── pack.toml                 ← the only declaration source
└── components/               ← the files the manifest points at
   ├── skills/                → copied (flattened) into  ~/.config/nevoflux/skills/
   ├── canvas-tools/*.toml    → copied into             ~/.config/nevoflux/canvas-tools/
   ├── seed/*.md              → seeded into the GBrain knowledge base (only if absent)
   └── canvas-app/dist/       → inserted as a persistent "My Canvas" dashboard artifact
```

What the platform guarantees for you:

- **Path safety** — files only ever land in the whitelisted extension dirs; a manifest
  can't escape them.
- **Idempotency** — re-installing the same version is a no-op; seed pages are only written
  if they don't already exist; the dashboard upserts by a fixed id.
- **Transactional install** — if any step fails, everything already done is rolled back.
- **Clean, receipt-driven uninstall** — the platform records exactly what it placed and
  reverses precisely that. It **keeps the user's knowledge-base data unless they explicitly
  ask to purge it**, and it skips deleting files the user has since edited.

---

## 2. Quick start

A minimal one-skill pack:

```
hello-pack/
├── pack.toml
└── components/
   └── skills/
      └── hello/
         └── SKILL.md
```

`pack.toml`:

```toml
[pack]
name = "hello-pack"
version = "0.1.0"
protocol = "pack-protocol/0.1"
min_nevoflux = "0.3.0"

[components.skills]
dir = "components/skills"
```

Install it (the daemon must be running):

```bash
nevoflux pack validate  hello-pack/pack.toml   # dry capability check, no writes
nevoflux pack install   hello-pack/pack.toml
nevoflux pack list                             # → hello-pack 0.1.0
nevoflux pack uninstall hello-pack             # clean removal
```

…or from the browser: **Settings → Packs → Install Pack…** (enter the path to `pack.toml`).

---

## 3. The manifest — `pack.toml`

`pack.toml` is the **single source of truth**. All component paths are relative to the
directory that contains `pack.toml`.

### 3.1 `[pack]` (required)

```toml
[pack]
name = "my-pack"                  # required. [a-z0-9-]+, unique. Also the receipt key and
                                  #   the default GBrain namespace prefix.
version = "0.1.0"                 # required. semver.
protocol = "pack-protocol/0.1"    # required. Must be a protocol version the platform supports.
min_nevoflux = "0.3.0"            # required. semver lower bound; checked against the daemon's version.
description = "One-line summary"  # optional.
license = "MIT"                   # optional.
authors = ["You <you@example>"]   # optional.
namespace = "my"                  # optional. Overrides the GBrain namespace prefix.
                                  #   Default = name. Use this to decouple a long pack name
                                  #   from a short page prefix (e.g. name "career-pack" → namespace "career").
```

### 3.2 Components overview

| Component | Lands in | Removed on uninstall? |
|---|---|---|
| `[components.skills]` | `~/.config/nevoflux/skills/` | yes (sha-guarded) |
| `[components.canvas_tools]` | `~/.config/nevoflux/canvas-tools/` | yes |
| `[[components.seed]]` | GBrain page (only if absent) | **no — kept unless `--purge-data`** |
| `[components.dashboard]` | "My Canvas" (persistent artifact) | yes |
| `[components.protected]` | (declaration only) | n/a — marks pages as user data |
| `[components.knowledge]` | — | **not supported yet — see §4.6** |

Every component is optional. A pack can ship any subset.

---

## 4. Components in depth

### 4.1 Skills — `[components.skills]`

```toml
[components.skills]
dir = "components/skills"
```

`dir` points at a directory in your pack. Its contents are **flattened one level** into the
user's skills directory. The skill loader scans exactly one level and recognizes two shapes:

```
components/skills/
├── my-evaluate/SKILL.md       → installs as  skills/my-evaluate/SKILL.md
├── my-scan/SKILL.md           → installs as  skills/my-scan/SKILL.md
└── my-quick.md                → installs as  skills/my-quick.md
```

A skill file is Markdown with YAML frontmatter:

```markdown
---
name: my-evaluate
description: Evaluate a thing and produce a report.
allowed-tools:
  - browser_navigate
  - browser_snapshot
  - brain_get_page
  - brain_put_page
---

# My Evaluate

Step 1: read the conventions …
```

**Two important rules about skills:**

1. **`allowed-tools` must use the daemon's *real* tool names** (e.g. `browser_navigate`,
   `brain_put_page`, `web_search`, `fetch_page`, `canvas_render`). A name that doesn't match a
   registered tool is **silently ignored** (the tool just won't be available). Verify against
   the live tool registry — a typo costs you a tool with no error.
2. **The `dependencies` frontmatter field is cosmetic** — it's parsed but the loader does
   nothing with it. Do **not** rely on it to auto-load other files. To share rules/contracts
   between skills, use **conventions** (next section).

#### Conventions (shared rules) via `skill_read`

To share invariants/contracts across several skills, ship them as files under a **host skill**
and read them at runtime with `skill_read`:

```
components/skills/
└── my/                         ← a "host" skill named `my`
   ├── SKILL.md
   └── conventions/
      ├── rules.md
      ├── scoring.md
      └── writing.md
```

In a skill's body, read a convention with:

```
skill_read('my', 'conventions/rules.md')
```

`skill_read(name, path)` reads files under the named skill's directory (subdirectories are
allowed; `..` traversal is blocked). Convention: have each skill's body `skill_read` its
shared rules as the first step. This is platform-supported and needs no special manifest field.

### 4.2 Canvas-tools — `[components.canvas_tools]`

```toml
[components.canvas_tools]
files = ["components/canvas-tools/pdf-render.toml"]
external_binaries = ["weasyprint"]   # optional: probed by `pack doctor`/status; never executed by the pack
```

Each file is a whitelist **TOML tool definition**, copied to
`~/.config/nevoflux/canvas-tools/<basename>` and picked up by the canvas-tools loader
(re-scanned on demand — no restart needed). Example tool TOML:

```toml
name = "pdf.render"
description = "Render an HTML file to PDF (ATS-friendly)"
kind = "command"            # "command" | "internal"
binary = "weasyprint"
args_mode = "template"
args = ["{{input}}", "{{output}}"]

[params.input]
type = "path"
allowed_prefix = "$SESSION_DIR"
must_exist = true
[params.output]
type = "path"
allowed_prefix = "$SESSION_DIR"

[constraints]
timeout_seconds = 120
cwd = "$SESSION_DIR"
```

`external_binaries` are declared so `pack status` can tell the user "weasyprint not found —
install it"; the pack engine never runs them. See the canvas-tool schema reference in
`docs/reference/skills/app/SKILL.md`.

### 4.3 Seed pages — `[[components.seed]]`

Use seed for **starter/template pages the user will edit**. Each seed is written to the GBrain
knowledge base **only if the page doesn't already exist** (idempotent — re-install never
clobbers user edits).

```toml
[[components.seed]]
slug = "my/cv"
from = "components/seed/cv.template.md"

[[components.seed]]
slug = "my/profile"
from = "components/seed/profile.template.md"
```

- `slug` must live **inside your namespace** (see §4.5) — i.e. equal the namespace or start
  with `<namespace>/`.
- **Every seed slug MUST be covered by `[components.protected]`** (§4.4) or the pack is
  rejected at validation. This is the platform's guarantee that scaffolding you seed is
  treated as user data and never auto-deleted.
- Seed pages are **kept on uninstall by default**; only `--purge-data` removes them.

### 4.4 Protected — `[components.protected]`

Declares which GBrain pages/prefixes are **user data** that uninstall must never delete by
default. This is a declaration only (no files placed).

```toml
[components.protected]
slugs    = ["my/cv", "my/profile"]
prefixes = ["my/reports/", "my/companies/"]
```

- Everything listed must be inside your namespace.
- A page matches if its slug equals a `slugs` entry or starts with a `prefixes` entry.
- Rule of thumb: **anything your skills write that represents the user's own data** should be
  under a protected prefix, and **every `seed` slug must be covered here**.

### 4.5 Namespacing (how packs stay out of each other's way)

Your pack may only touch GBrain pages under its **namespace prefix**: the `[pack] namespace`
if set, else `[pack] name`. Every `seed` slug and every `protected` slug/prefix must equal the
namespace or sit under `<namespace>/`. Out-of-namespace slugs are rejected. This keeps two
packs (and the user's own pages) from colliding.

### 4.6 Dashboard — `[components.dashboard]`

Ships a **prebuilt Canvas micro-app** as a persistent "My Canvas" artifact.

```toml
[components.dashboard]
artifact_id = "my-pack-dashboard"   # must start with the pack name (namespace rule for artifacts)
content_type = "project"
files_from = "components/canvas-app/dist"   # a directory of built files (index.html + assets)
entry = "index.html"                        # the entry file within files_from
```

- The directory's files are packed into the artifact's `files` map; the row is inserted with
  `is_persistent = 1`, so it survives session deletion and appears under **My Canvas**.
- Idempotent: re-install upserts the same `artifact_id` (no duplicate rows).
- `artifact_id` **must start with the pack name** so packs can't clobber each other's (or the
  user's) artifacts.

### 4.7 Knowledge import — `[components.knowledge]` — NOT YET SUPPORTED

A future component for shipping a whole prebuilt knowledge base as a removable, read-only
GBrain *source*. **It is not available in `pack-protocol/0.1`.** If your manifest contains a
`[components.knowledge]` table, **install is rejected** with the error code
`KNOWLEDGE_UNSUPPORTED`. Do not include it yet.

> Why deferred: the runtime knowledge-base source mapping (mounting/removing a named
> read-only source) is not implemented yet. Until it lands, ship user-editable starter content
> as `[[components.seed]]` pages instead (which are individually tracked and protected).

### 4.8 Config — forbidden

There is **no** `[components.config]`. Packs may not write to `config.toml` in
`pack-protocol/0.1`. A manifest containing `[components.config]` is rejected. (Express
behavior through skills/conventions instead.)

---

## 5. The capability sandbox (validation rules)

Before placing a single file, the platform validates your manifest against these invariants
and reports **all** violations at once. Run `nevoflux pack validate <manifest>` to see them
without installing. Your pack must satisfy:

1. **Whitelisted destinations only** — files land solely in `skills/`, `canvas-tools/`, or
   `packs/<name>/`. Nothing else is writable.
2. **No path traversal (cross-platform)** — every source path (`skills.dir`,
   `canvas_tools.files[]`, `seed.from`, `dashboard.files_from`) must be a *relative* path that
   stays inside the pack; absolute paths, `..` escapes, and backslash separators are rejected
   on every OS.
3. **No `[components.config]`** — config writes are forbidden.
4. **Namespace isolation** — `seed` slugs, `protected` slugs/prefixes (and a future
   knowledge `source_name`) must be inside the pack namespace.
5. **`seed` ⊆ `protected`** — every seed slug must be covered by a protected slug/prefix.
   *Hard reject otherwise* — this is the "never auto-delete user data" guarantee.
6. **Dashboard id namespacing** — `dashboard.artifact_id` must start with the pack name.

Parse-time field checks also apply: `name` matches `[a-z0-9-]+`; `version`/`min_nevoflux` are
valid semver; `protocol` is supported.

---

## 6. Lifecycle & guarantees

**Install** runs in phases, appending to a receipt and rolling back on any failure:
`resolve → compat (min_nevoflux ≤ daemon) → capability → idempotency → place files → seed
pages → dashboard artifact → activate (reload skills) → commit receipt`.

- **Idempotent**: installing the same version is a no-op (use `--force` to reinstall); seed is
  only-if-absent; the dashboard upserts by id.
- **Receipt**: written to `~/.config/nevoflux/packs/<name>/receipt.json` — records every placed
  file (absolute path + sha256), the dashboard artifact id, and the seeded page slugs.

**Uninstall** is driven entirely by the receipt (it never guesses):

- Deletes the files the pack placed — but **skips any file the user has since edited**
  (sha256 mismatch) unless you pass `--force`.
- Removes the dashboard artifact and prunes the pack's own dirs.
- **Keeps seeded/user pages by default.** `--purge-data` deletes the seeded pages (protected
  pages still refuse to be auto-removed).

**Update** refreshes the pack's own files/artifacts and adds any *new* seed pages
(only-if-absent), but never touches existing user data.

---

## 7. Installing & managing packs

**CLI** (requires a running daemon):

```bash
nevoflux pack validate   <path/to/pack.toml>          # dry capability check → { ok, violations[] }
nevoflux pack install    <path/to/pack.toml> [--force]
nevoflux pack uninstall  <name> [--purge-data] [--force]
nevoflux pack update     <path/to/pack.toml>
nevoflux pack list                                    # installed packs
nevoflux pack status     <name>                       # version, component counts, deps
```

**Settings UI**: **Settings → Packs** — lists installed packs, installs from a `pack.toml`
path (with an optional pre-flight validate), and offers per-row Update / Uninstall (with an
optional "also delete this pack's data" choice, defaulting to *off*). Progress and errors
(including `KNOWLEDGE_UNSUPPORTED`) are surfaced inline.

Where things land on Linux (macOS/Windows use the platform config dir):

| What | Path |
|---|---|
| Skills | `~/.config/nevoflux/skills/` |
| Canvas-tools | `~/.config/nevoflux/canvas-tools/` |
| Receipt | `~/.config/nevoflux/packs/<name>/receipt.json` |
| Seed pages | GBrain knowledge base (under your namespace) |
| Dashboard | "My Canvas" (persistent artifact) |

---

## 8. Versioning & compatibility

- **`protocol`** (`pack-protocol/MAJOR.MINOR`) — the protocol your manifest targets; the
  platform validates it's supported. Breaking changes bump MAJOR; additive fields bump MINOR.
- **`min_nevoflux`** — your pack's lower bound on the daemon version. Install fails early with
  a clear message if the daemon is older.
- Bump your pack's own `version` (semver) on every release; `update` uses it.

---

## 9. Testing your pack

1. **Validate** (no writes, no daemon mutations):
   ```bash
   nevoflux pack validate my-pack/pack.toml
   ```
   Expect `{ "ok": true, "violations": [] }`. Any violation strings (e.g.
   `SeedNotProtected`, `PathTraversal`, `SlugOutsideNamespace`,
   `ArtifactIdNotNamespaced`, `ConfigComponentForbidden`) tell you exactly what to fix.

2. **Round-trip in a sandbox** so you don't touch your real config. Point the daemon and CLI at
   throwaway dirs (seed an empty `config.toml` so the config dir resolves to the sandbox):
   ```bash
   export XDG_CONFIG_HOME=/tmp/pk-cfg NEVOFLUX_DATA_DIR=/tmp/pk-data
   mkdir -p /tmp/pk-cfg/nevoflux && : > /tmp/pk-cfg/nevoflux/config.toml
   nevoflux --daemon &                 # start a sandboxed daemon
   nevoflux pack install   my-pack/pack.toml
   nevoflux pack list
   ls /tmp/pk-cfg/nevoflux/skills/      # verify placement
   cat /tmp/pk-cfg/nevoflux/packs/my-pack/receipt.json
   nevoflux pack uninstall my-pack      # verify it leaves no trace
   nevoflux --stop
   ```

3. **Skill lint**: confirm every skill's `allowed-tools` entry matches a real tool name (a
   mismatch is silently dropped, so check the registry).

---

## 10. A complete example

```
career-pack/
├── pack.toml
└── components/
   ├── skills/
   │  ├── career/                      # host skill: conventions live here
   │  │  ├── SKILL.md
   │  │  └── conventions/{rules,scoring,writing}.md
   │  ├── career-evaluate/SKILL.md
   │  └── career-scan/SKILL.md
   ├── canvas-tools/pdf-render.toml
   ├── seed/{cv,profile}.template.md
   └── canvas-app/dist/index.html
```

```toml
[pack]
name = "career-pack"
version = "0.1.0"
protocol = "pack-protocol/0.1"
min_nevoflux = "0.3.0"
description = "A job-hunt command center."
license = "MIT"
namespace = "career"                 # pages live under career/…

[components.skills]
dir = "components/skills"

[components.canvas_tools]
files = ["components/canvas-tools/pdf-render.toml"]
external_binaries = ["weasyprint"]

[[components.seed]]
slug = "career/cv"
from = "components/seed/cv.template.md"
[[components.seed]]
slug = "career/profile"
from = "components/seed/profile.template.md"

[components.dashboard]
artifact_id = "career-pack-dashboard"
content_type = "project"
files_from = "components/canvas-app/dist"
entry = "index.html"

[components.protected]
slugs    = ["career/cv", "career/profile"]
prefixes = ["career/reports/", "career/companies/"]
```

`career/cv` and `career/profile` are seeded (only if absent) **and** protected; the skills'
conventions are read via `skill_read('career', 'conventions/…')`; the dashboard lands in My
Canvas; uninstall removes the skills + tool + dashboard but **keeps the user's `career/…`
pages** unless `--purge-data` is passed.

---

## 11. Gotchas / FAQ

- **"My skill installed but a tool doesn't work."** An `allowed-tools` entry doesn't match a
  real tool name — it's silently ignored. Check the exact tool name.
- **"Install was rejected: `SeedNotProtected`."** Add every `seed` slug to
  `[components.protected]` (a slug or a covering prefix). This is mandatory.
- **"`KNOWLEDGE_UNSUPPORTED`."** Remove `[components.knowledge]` — it's deferred (§4.7). Use
  `[[components.seed]]` for starter content.
- **"`PathTraversal` / `OutsideWhitelistDir`."** A source path is absolute, uses `..`, or uses
  backslashes. Keep all component paths relative and inside the pack.
- **"`ArtifactIdNotNamespaced`."** Prefix `dashboard.artifact_id` with your pack name.
- **CLI can't reach the daemon.** Start it (`nevoflux --daemon`) or ensure the browser/native
  host launched a daemon built with `pack.*` support.
- **Uninstall left my edited file.** By design — uninstall skips files whose sha256 no longer
  matches the receipt (you edited them). Use `--force` to remove anyway.
- **The dashboard didn't update after `update`.** It upserts by `artifact_id`; ensure the id is
  stable across versions.
