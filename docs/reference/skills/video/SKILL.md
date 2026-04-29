---
name: video
description: Author HyperFrames-compatible video compositions that render to MP4 via WebCodecs. Use for any user request involving video, animation, motion graphics, or "make a video".
version: "1.2.0"
author: "NevoFlux"
tags: [canvas, video, composition, animation, hyperframes]
enabled: true
triggers:
  - "/video"
  - "做个视频"
  - "做一个视频"
  - "make a video"
  - "video composition"
  - "render mp4"
  - "motion graphics"
  - "animated intro"
  - "TikTok"
  - "9:16"
  - "product intro video"
  - "release announcement video"
  - "视频介绍"
# NOTE: allowed_tools is deliberately left empty (same as the other shipped
# skills — app, skill-creator). The daemon's built-in tool check in
# server.rs::BUILTIN_TOOLS doesn't currently include canvas_video_* or
# the artifact tools, so a non-empty list here would cause the skill to
# fail load. Once BUILTIN_TOOLS is expanded (or the check rewired against
# the agent's own tool registry), we can restore the explicit list with
# canvas_create_composition / canvas_lint_composition / canvas_render_video
# plus (when they ship) extract_visual_identity and tts_synthesize_* in
# Week 8-11.
allowed_tools: []
snippets: []
---

# /video — HyperFrames-Compatible Video Composition

Generate MP4-renderable compositions. HTML must work in Canvas preview mode and under `canvas_render_video` deterministic rendering.

## When to Load / Mode Decision

**Load:** 视频/video/MP4/动画/motion graphics/intro/promo/announcement/aspect-ratio spec/"make a video from" CSV|URL|PDF.

**Skip:** static poster (SVG/HTML), interactive app (`/app`), slide deck (pptx).

**Mode selection:**
```
Uploaded video file?  Yes → Mode 2 (video overlay)
URL or @tab ref?      Yes → Mode 3 (website-to-video)
Otherwise             → Mode 1 (generate from scratch)
```

## High-Level Approach

**Required for new compositions (5 steps); skip for minor edits:**

1. **WHAT** — Nail the narrative arc and emotional beats before touching HTML.
2. **Structure** — Count scenes; assign tracks (video / audio / overlay / caption).
3. **Timing** — Which clip drives total duration? TikTok rhythm (~1.5s/beat) vs. documentary (~5s)?
4. **Layout** — Sketch each scene's hero frame (element positions at maximum saturation, pre-animation).
5. **Animate** — Only now add GSAP; `gsap.from()` into CSS-defined positions.

## Visual Identity Gate (VIG)

**Never write a composition without visual identity. Never default to `#3b82f6` or `Roboto`.**

Check in order:
1. **DESIGN.md in context?** → Use it, skip questions.
2. **Named style from user** (e.g. "Swiss Pulse", "Velvet Standard", "dark and techy") → match against `reference/visual-styles.md` (8 named styles grounded in real graphic design traditions: Swiss Pulse, Velvet Standard, Deconstructed, Maximalist Type, Data Drift, Soft Signal, Folk Frequency, Shadow Cut). Generate a minimal DESIGN.md from that style's palette + GSAP signature + suggested transition.
3. **Mood keyword from user** ("explosive", "warm", "cinematic", "luxury", "technical") → map via `reference/vocabulary.md` §1, then pick a palette from `reference/palettes/` (9 ready palettes: bold-energetic, warm-editorial, dark-premium, clean-corporate, nature-earth, neon-electric, pastel-soft, jewel-rich, monochrome). Confirm in 2 sentences.
4. **Neither** → Ask 3 questions: mood (explosive/cinematic/fluid/technical/warm) + canvas (light/dark) + brand reference. Echo understanding back; wait for confirmation before coding.

*Exception — Mode 2:* visual identity comes from source video; only ask overlay color + font style.

*Self-check:* "Blue or green?" = no VIG. About to write `color: #3b82f6` = no VIG.

## Three Creative Modes

**Mode 1 — From scratch.** Start from `templates/product-intro-*`, `tiktok-hook`, `logo-3d-reveal`, or `product-3d-spin`. VIG always required.

**Mode 2 — Video overlay.** User uploads `.mp4/.mov/.webm`; adds subtitles, annotations, watermarks. Start from `templates/video-overlay.html`. Hard rules:
- `composition.duration` ≤ source video duration (black frames otherwise)
- No simultaneous `<video>` + `<audio>` (source audio passes through)
- Layer via `data-track-index`: source video = 0, overlays = 1–10, watermark = 11+

**Mode 3 — Website-to-video.** Start from `templates/website-promo-16x9.html`. Two-step shortcut for the brand-identity phase:

1. `canvas_extract_visual_identity({target:{url}})` → returns a `VisualIdentity` JSON (name, tagline, colors with role hints, fonts by source, logo URL, key_assets).
2. `canvas_create_from_visual_identity({title, width, height, duration_sec, fps, template, visual_identity})` — pass the VI from step 1 verbatim. Daemon deterministically renders DESIGN.md from VI fields and creates the composition in one call. **Do NOT manually render DESIGN.md and call `canvas_create_composition`** — that path is error-prone (LLM hallucinates field names, drops weights, mis-formats YAML); the new tool is the right baseline.

After the composition exists you can iterate:
- (Optional) `browser_edit_artifact` on the composition's `DESIGN.md` to apply user-specific tweaks ("make background black"); then `canvas_apply_design_md` to refresh the brand layer.
- (Optional) Write `SCRIPT.md` / `STORYBOARD.md` into the composition for narration + per-beat planning.
- (Optional) `tts_synthesize_api({composition_id, text})` — writes `narration.mp3` directly into the composition's files map; the renderer picks it up and muxes into the MP4. Requires `[tts.elevenlabs] api_key` in `~/.config/nevoflux/config.toml`.
- (Future) `tts_synthesize_local` (Kokoro) + `tts_transcribe` (Whisper) — registered today but return ConfigMissing until the `nevoflux-tts` ONNX crate ships.
- `canvas_lint_composition` → zero ERROR → send preview link. (Daemon path runs in strict mode: narrowed warnings — `comp/overlapping-gsap-tweens`, `comp/unscoped-gsap-selector` — escalate to errors.)
- `canvas_render_video` → MP4 (with narration audio if present).

Key Mode 3 components: `components/screenshot-reveal.html`, `components/feature-list-checkmark.html`. The VI extraction skips the VIG questionnaire — just echo back `name` / `colors.primary` / `typography.hero.family` to the user for confirmation. Template: `reference/DESIGN-template.md`; extensions: `reference/design-md-video-extension.md`.

## Image Asset Gate (IAG) — MANDATORY when user provides any image

**If the user's request involves an image they provided** (uploaded file, URL, "this picture", clipboard paste, "do a video of <THIS>"), call `canvas_attach_asset` BEFORE writing any `<img src="assets/...">` HTML. The order is `canvas_create_composition` → `canvas_attach_asset` → `browser_edit_artifact` (the create gives you the `composition_id`).

### Vision input vs binary asset — they're different

When the user attaches a local file to chat, you'll see TWO things in your context:

1. **The image as a vision attachment** — you can literally "see" it. Use this to make design decisions: pick colors, write copy that fits, choose layout.
2. **The file path as a `local_files` entry** — usually something like `/tmp/Image_xyz.png`. Use this to call `canvas_attach_asset({ local_path: ... })`.

These are TWO separate jobs:
- vision attachment → for YOUR understanding
- `canvas_attach_asset` → for the RENDERER (puts bytes into `composition.files["assets/..."]` so `<img src="assets/...">` resolves at render time)

You need both. Seeing the image doesn't put it in the artifact; calling attach without seeing leaves you without design context.

### Choosing the source variant

`canvas_attach_asset` takes ONE of four sources:

| Source | When to use |
|---|---|
| **`local_path`** | **PREFERRED** when the path is in your `local_files` context (user uploaded a file). Daemon reads bytes server-side, no tool-arg size cap. |
| `url` | User gave you a public http(s) URL the daemon can fetch directly. |
| `data_b64` | You already have the bytes in tool-arg-friendly size (≤ 1 MB). For larger files use `local_path` or `url` — `data_b64` will hit tool-arg limits. |
| `from_tab` | Not yet wired — currently errors. Use `data_b64` with explicitly captured screenshot bytes. |

```
1. canvas_attach_asset({ composition_id, local_path: "/tmp/Image_xyz.png" })
   → returns { path: "assets/Image_xyz.png", mime_type, size_bytes }
2. Reference the returned `path` in HTML: <img src="assets/Image_xyz.png">
```

### Banned patterns (silently break the render or burn turns)

- ❌ `<img src="https://example.com/the-image.jpg">` — render tab can't fetch external URLs.
- ❌ `<img src="data:image/png;base64,iVBOR...">` — bloats artifact, breaks lint.
- ❌ Writing `<img src="assets/foo.png">` BEFORE calling `canvas_attach_asset` — file doesn't exist yet.
- ❌ Calling `canvas_attach_asset` AFTER `browser_edit_artifact` finishes editing index.html — past Canvas Editor saves clobbered assets; now defended at the daemon but order still cleaner.
- ❌ **`glob`-ing `/tmp/`, `read`-ing the image (UTF-8 fails), or asking the user "what's the path?" when the path is already in your `local_files` context.** This wastes turns. The path appears as a path-string near the user message; use it directly with `local_path`.
- ❌ Spinning up a local HTTP server to expose the file via URL — pointless detour. Use `local_path` instead.

### Why it's strict

The renderer loads composition HTML in a sandbox iframe with no origin. Only `assets/*` paths backed by entries in the composition's `files` map get inlined as `data:` URIs. Anything else 404s.

### Mode 3 exception

`canvas_create_from_visual_identity` already attaches the extracted hero screenshot internally — no separate `canvas_attach_asset` needed for that one image. Additional images still go through the IAG.

## Template-vs-custom decision (read before calling create_composition)

The 7 shipped templates are **opinionated** — pre-baked scenes for specific creative briefs (TikTok hook, product reel, website promo, 3D logo reveal). They have their own placeholder copy slots (`<<HOOK_WORD>>`, `<<FEATURE_N_TITLE>>`), scene timing, and ambient decoratives.

Use `template`:
- User says "TikTok hook" / "product intro" / "website promo" — match a template's brief.
- User asks for one of the named visual styles in `reference/visual-styles.md`.

Use `html` (custom):
- User says "make a video of this image" / "animate this picture" / "video about X" without naming a style.
- User's main content is a single image they provided.
- User wants minimal / clean / non-template-y output.

**Defaulting to a template when the user didn't ask for one is a known failure mode** — the output looks like the template (its scenes, its decoratives, its rhythm), not what the user described. Better to ship a clean custom HTML composition that's actually about their content.

## DESIGN.md Workflow

`DESIGN.md` lives in composition VirtualFS — persistent brand identity across sessions.

**Before every composition:** `readFile('DESIGN.md')` — found → use it; not found → create (Mode 3: auto via `canvas_create_from_visual_identity`; Mode 1/2: VIG answers → write to file).

Sections: `## Brand` / `## Colors` / `## Typography` / `## Motion` / `## Style Prompt` / `## What NOT to Do`. Full template: `reference/DESIGN-template.md`.

Usage: CSS variables (`color: var(--primary)`); eases from Motion section; "change colors" → edit DESIGN.md not composition; second video → reuse first DESIGN.md.

## TTS & Narration

See `snippets/tts-workflow.md` (full workflow + status) and `snippets/auto-captions.md` (caption emission rules).

**Today (P5b-1 + P5b-final shipped):**
- `tts_synthesize_api({text, composition_id, voice_id?, model_id?})` — ElevenLabs HTTP path. Daemon writes `narration.mp3` into the artifact's files map; renderer auto-muxes via ffmpeg. Requires `[tts.elevenlabs] api_key` in `~/.config/nevoflux/config.toml`. Text capped at 600 chars (~60s of speech).

**Registered but gated on the next `nevoflux-tts` ONNX crate milestone:**
- `tts_synthesize_local` — Kokoro-82M local TTS (`af/am/bf/bm/zf/zm` voices). Returns ConfigMissing today.
- `tts_transcribe` — Whisper ONNX → segment timestamps. Returns ConfigMissing today. Auto-captions depend on this.

**Auto-captions (P5c):** when Whisper is wired, `tts_transcribe({composition_id, file_path: "narration.mp3"})` returns `segments[]`; group by sentence boundary (7–15 chars per row) and emit `caption-subtitle` clips on `data-track-index ≥ 20`. Eight `comp/caption-*` lint rules enforce the DOM contract. Constraints: max 1 narration per composition (`nf/single-audio`); no narration when a `<video>` track has its own audio.

## Layout Before Animation

See `snippets/layout-before-animation.md` for code examples.

Every element must be CSS-positioned at its hero frame **before** any GSAP is added. `.scene-content` must use flex/grid with padding — not `position:absolute` with hardcoded `top`. Use `gsap.from()` to animate into CSS-defined position; CSS is ground truth.

## Scene Transitions

See `snippets/scene-transition-ironrules.md` for code examples.

4 hard rules (any violation = broken composition):
1. Insert a transition component between every pair of scenes (`flash-through-white`, `crossfade`, or `wipe-diagonal`)
2. Every visible element needs an entrance animation (`gsap.from()` or `gsap.fromTo()`)
3. **No exit animations before the transition** — clip adapter handles visibility via `data-duration`; only the final scene may fade out
4. Overlay elements (lower-thirds, captions, watermarks) may have their own exit animations

## Timeline, Clip & Determinism Rules

Required `#stage` attributes: `data-composition-id`, `data-width`, `data-height`, `data-duration`, `data-fps`, `data-bg`.

All timed elements: `class="clip"` + `data-start` + `data-duration` + `data-track-index`. GSAP: `{ paused: true }` → `window.__timelines`. Media: `data-start` + `data-duration` + `data-volume`. Async loads: `window.__readyPromises`.

CDN whitelist (esm.sh only): `gsap@3.13`, `three@0.160`, `lottie-web@5.12`. No jsdelivr/unpkg. Assets via relative VirtualFS paths — no external URLs. Fonts via `@font-face` + `assets/` or system stack; no Google/Adobe Fonts.

**Determinism quick-reference** (full spec: `snippets/determinism-rules.md` + `reference/canvas-render-determinism-spec.md`):

| Forbidden | Use instead |
|---|---|
| `repeat: -1` | `repeat: Math.ceil(duration/cycle) - 1` |
| `gsap.set()` on future-scene elements | `tl.set(selector, vars, timePosition)` |
| `fetch()` during render | Pre-load in `__readyPromises` |
| `setTimeout` / `setInterval` for animation | GSAP timeline |
| CSS `@keyframes` | GSAP (lint WARN) |
| `<br>` in wrapping text | `max-width` + natural wrap |
| `ScrollTrigger` / `OrbitControls` / `setAnimationLoop` | GSAP tween for camera/scroll |
| `window.screen.*` / `devicePixelRatio` branching | `stage.dataset.width/height` |

## Three.js Rules

See `snippets/three-js-rules.md` for code examples (lint enforces all 6).

1. `preserveDrawingBuffer: true` on `WebGLRenderer`
2. No interaction controls (`OrbitControls`, `DragControls`) — camera motion via GSAP tween
3. Register `{ renderer, scene, camera }` in `window.__threeRenderers`
4. Asset loads in `window.__readyPromises`
5. Skeletal animation: `mixer.setTime(t)` via `NevofluxSDK.timeline.onFrame` — never `mixer.update(delta)`
6. Renderer size: `stage.dataset.width/height` (preferred) or `window.innerWidth/innerHeight` (both valid)

## Output & Checklist

**After lint passes:** reply with spec (W×H, duration, fps, scene count, engine), lint result (0 errors), and preview link `nevoflux://canvas/{id}/composition`.

**Pre-submit self-check:**
- Approach 5 steps + VIG + static CSS done before GSAP?
- Every `.clip` has `data-start`+`data-duration`? GSAP `{paused:true}` in `__timelines`?
- Async loads in `__readyPromises`? Three.js: `preserveDrawingBuffer`+`__threeRenderers`?
- Assets: relative paths, esm.sh CDN only, no `repeat:-1`, no `<br>` in wrapping text?
- Every element has entrance anim; no exit anims except final scene/overlays; transitions between scenes?

**v1 limits:** duration ≤ 60s; resolution ≤ 1920×1920; fps ∈ {24,25,30}; max 1 `<audio>`; MP4 only. Exceed any → tell user.

## Rules Snapshot

Core disciplines (each one-line with linter rule_id where enforced):
- Layout before animation (see `snippets/layout-before-animation.md`; enforced by `comp/timed-element-missing-clip-class`)
- Deterministic time (see `snippets/determinism-rules.md`; enforced by `nf/forbidden-apis`)
- Scene container layout (see `snippets/scene-transition-ironrules.md`; enforced by `comp/overlapping-clips-same-track`)
- Three.js renderer setup (see `snippets/three-js-rules.md`; enforced by `nf/three-renderer`, `nf/three-register`, `nf/mixer-settime`)
- CDN whitelist (enforced by `nf/cdn-whitelist`)
- Single audio per composition (enforced by `nf/single-audio`)

## Snippets

| Path | Purpose |
|---|---|
| `snippets/layout-before-animation.md` | Discipline for layout stability before animations |
| `snippets/scene-transition-ironrules.md` | Scene container + transition track rules |
| `snippets/three-js-rules.md` | Required patterns for Three.js templates |
| `snippets/determinism-rules.md` | Top-level rules for reproducible renders |
| `snippets/tts-workflow.md` | When/how to use TTS narration (status + workflow) |
| `snippets/auto-captions.md` | Caption emission contract + lint guarantees |
| `snippets/ambient-decoratives.md` | **Background decorative layer rules — 2-5 ambient decoratives per scene; biggest delta between AI-generic and intentional design** |
| `scripts/animation-map.mjs` | **Static GSAP timeline analyzer.** Run `node scripts/animation-map.mjs <comp.html>` to see per-tween summary, stagger detection, dead-zone report (>1s gaps), pacing flags (fast/slow), and an ASCII Gantt. Catches choreography problems the linter misses. |
| `reference/visual-styles.md` | 8 named visual identities (Swiss Pulse, Velvet Standard, Deconstructed, Maximalist Type, Data Drift, Soft Signal, Folk Frequency, Shadow Cut) — palette + GSAP signature + transition pairing |
| `reference/palettes/` | 9 ready palettes (bold-energetic, warm-editorial, dark-premium, clean-corporate, nature-earth, neon-electric, pastel-soft, jewel-rich, monochrome) for direct DESIGN.md `## Colors` use |

## Templates & Components

Templates (full-composition scaffolds):

| Path | Purpose |
|---|---|
| `templates/website-promo-16x9.html` | Website-to-video promo (16:9, mode 3 golden path) |
| `templates/product-intro-16x9.html` | 16:9 product intro (3-scene, 25-35s) |
| `templates/product-intro-9x16.html` | 9:16 vertical adaptation |
| `templates/tiktok-hook.html` | TikTok/Reels hook (9:16, 6s, 3-scene) |
| `templates/video-overlay.html` | Mode-2 overlay on existing video |
| `templates/logo-3d-reveal.html` | Three.js minimal logo reveal |
| `templates/product-3d-spin.html` | Three.js IcosahedronGeometry spin |

Components (drop-in clips):

| Path | Purpose |
|---|---|
| `components/feature-list-checkmark.html` | 3-5 feature checklist, stagger bounce |
| `components/screenshot-reveal.html` | Image reveal with border animation |
| `components/wipe-diagonal.html` | Diagonal pixel-wipe scene transition (0.6s) |
| `components/flash-through-white.html` | White flash hard-cut transition (0.45s) |
| `components/crossfade.html` | Two-layer crossfade (0.8s) |
| `components/data-chart-bar-race.html` | Horizontal bar chart race |
| `components/data-chart-line.html` | Line chart with draw-on |
| `components/caption-subtitle.html` | Single-line subtitle (TTS-style) |
| `components/caption-bouncy.html` | Per-word bouncy entrance |
| `components/caption-typewriter.html` | Character-by-character reveal |
| `components/caption-animated-overlay.html` | Multi-line narrative overlay |
| `components/lower-third-corporate.html` | Corporate lower-third band |
| `components/lower-third-minimal.html` | Minimal editorial lower-third |
| `components/annotation-arrow.html` | SVG arrow annotation |
| `components/watermark-animated.html` | Corner-pinned brand watermark |
