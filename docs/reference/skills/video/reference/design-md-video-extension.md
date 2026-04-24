# Video Extension for Google design.md — Formal Specification

**Version:** 1.0.0 | **Status:** Stable
**Extends:** [google-labs-code/design.md](https://github.com/google-labs-code/design.md)
**Maintained by:** NevoFlux `/video` skill

This specification defines additive YAML keys (`motion`, `voice`, `aspect`) and markdown sections
that extend a Google `design.md` with video-specific tokens: GSAP easing curves, TTS voice
parameters, and canvas aspect/safe-zone configuration. Files conforming to this spec are
simultaneously valid Google `design.md` — non-video consumers see a correct base document.

---

## Status & Version

**Extension name:** Video Extension v1 for Google design.md (semver **1.0.0**).
Extends [google-labs-code/design.md](https://github.com/google-labs-code/design.md) — no pinned
upstream version; compatible with any release. Stability: **stable** — breaking changes require a
major version bump and a migration note in this file.

**Purpose.** Google's `design.md` captures brand tokens (colors, typography, spacing, components)
in YAML frontmatter + markdown. It does not address video concerns. This extension adds three
top-level YAML keys and three appended markdown sections without modifying the base schema.
Every existing Google design.md parser continues to work unchanged.

---

## Compatibility Rules

**Unknown-key policy.** Google's linter treats unrecognized top-level YAML keys as **warnings**,
not errors. `motion`, `voice`, and `aspect` generate at most lint warnings for non-video consumers.
Consumers MUST NOT fail hard on their presence.

**Section-ordering policy.** Google's eight canonical sections (Overview, Colors, Typography,
Layout, Elevation & Depth, Shapes, Components, Do's and Don'ts) MUST retain their order. This
extension appends three sections **after** "Do's and Don'ts": **Motion**, **Voice**, **Aspect &
Safe Zones**. Extension sections MUST NOT be interleaved among Google's eight.

**Subset and degradation behavior.** A consumer without video extension support receives a fully
valid Google design.md subset. An extension-aware consumer that encounters a file without video
keys MUST apply the defaults from the Fallback Table. Partial presence is supported: a file may
declare `motion` only; defaults apply for absent `voice` and `aspect`.

---

## Token Schema

Each field documents: `type` / `default` / `allowed` / `example` / `note`.

### motion

| Field | Type | Default | Allowed | Example | Note |
|---|---|---|---|---|---|
| `ease_default` | string | `"power2.out"` | any valid GSAP easing | `"power2.out"` | Default `ease:` for all GSAP tweens unless per-tween override present |
| `ease_entrance` | string | `"back.out(1.7)"` | same; factor 1.2–2.5 | `"back.out(1.7)"` | Applied to scene entrances and logo reveals; overrides `ease_default` |
| `ease_exit` | string | `"power2.in"` | same; power3.in snappier | `"power2.in"` | Applied to element and scene exits; overrides `ease_default` |
| `scene_duration_default` | string (CSS) | `"5s"` | 2s–10s recommended | `"5s"` | Default per-scene duration when composition plan omits explicit length |
| `stagger_default` | string (CSS) | `"0.3s"` | 0.05s–1.0s | `"0.3s"` | Default `stagger:` for multi-element GSAP tweens |
| `beat_interval` | string (CSS) | `"1.5s"` | 0.8s–5s | `"1.5s"` | Target visual-beat cadence; one major visual change per interval |
| `layout_before_animation` | boolean | `true` | true \| false | `true` | Structural intent flag; no runtime effect |
### voice
| Field | Type | Default | Allowed | Example | Note |
|---|---|---|---|---|---|
| `provider` | string (enum) | `"kokoro"` | `kokoro \| elevenlabs` | `"kokoro"` | TTS engine; maps to `tts_synthesize_local` or `tts_synthesize_api` |
| `voice_id` | string | `"af"` | kokoro: af/am/bf/bm/zf/zm; elevenlabs: UUID | `"af"` | Voice identifier; af = kokoro English female (warm) |
| `speed` | float | `1.0` | 0.5–2.0 | `1.0` | Speed multiplier passed directly to TTS provider |
| `tone` | string (enum) | `"neutral"` | `neutral \| energetic \| warm \| clinical` | `"neutral"` | Narration register hint; not passed to TTS provider |
| `pronunciation` | object | `{}` | string→string map | `{"API":"A P I"}` | Literal substitution on transcripts before TTS synthesis |
### aspect
| Field | Type | Default | Allowed | Example | Note |
|---|---|---|---|---|---|
| `default` | string (enum) | `"16:9"` | `16:9 \| 9:16 \| 1:1` | `"16:9"` | Selects canvas dimensions for `canvas_create_composition` |
| `width` | integer (px) | `1920` | 1920 (16:9) / 1080 (9:16 or 1:1) | `1920` | Canvas pixel width; must match `aspect.default` |
| `height` | integer (px) | `1080` | 1080 (16:9 or 1:1) / 1920 (9:16) | `1080` | Canvas pixel height; must match `aspect.default` |
| `safe_zones.top` | string (CSS px) | `"120px"` | any px value | `"120px"` | Clear margin from top; covers platform chrome (progress bars, live badges) |
| `safe_zones.bottom` | string (CSS px) | `"160px"` | 160px (16:9); 220px (9:16) | `"160px"` | Clear margin from bottom; reserves caption + platform overlay space |
| `safe_zones.sides` | string (CSS px) | `"80px"` | 80px (16:9); 120px (1:1) | `"80px"` | Clear margin per side; covers 5% edge-crop safety on YouTube/LinkedIn |

---

## Default Fallback Table

Authoritative reference for defaults when a `DESIGN.md` omits a video extension token.
| Token | Default | Source / rationale |
|---|---|---|
| `motion.ease_default` | `"power2.out"` | vocabulary.md mood "minimal/corporate"; safe workhorse for 90% of tweens |
| `motion.ease_entrance` | `"back.out(1.7)"` | vocabulary.md mood "energetic" entrance pattern; light bounce |
| `motion.ease_exit` | `"power2.in"` | Mirror of ease_default; vocabulary.md exit recipe |
| `motion.scene_duration_default` | `"5s"` | video-skill-draft.md §模式 1 average scene length |
| `motion.stagger_default` | `"0.3s"` | DESIGN-template.md mid-range; balances tightness and readability |
| `motion.beat_interval` | `"1.5s"` | Social-media pacing default; vocabulary.md "energetic" cadence |
| `motion.layout_before_animation` | `true` | Always-on structural discipline flag; no runtime effect |
| `voice.provider` | `"kokoro"` | Local TTS: no API key, sub-50 ms latency, offline-capable |
| `voice.voice_id` | `"af"` | Kokoro English female — clear, warm, intelligible at 1.0× |
| `voice.speed` | `1.0` | Neutral rate calibrated as natural speech for kokoro af |
| `voice.tone` | `"neutral"` | No bias; conversational register for most content |
| `voice.pronunciation` | `{}` | No substitutions; raw transcript passed to TTS unchanged |
| `aspect.default` | `"16:9"` | Dominant target: YouTube, LinkedIn, X |
| `aspect.width` | `1920` | Standard 1080p landscape |
| `aspect.height` | `1080` | Standard 1080p landscape |
| `aspect.safe_zones.top` | `"120px"` | Conservative: covers YouTube bar + TikTok live badge |
| `aspect.safe_zones.bottom` | `"160px"` | Caption reserve for 16:9; increase to 220px for 9:16 |
| `aspect.safe_zones.sides` | `"80px"` | ~4% of 1920px; YouTube and LinkedIn edge-crop safety |

---

## Consumption Contract

How the `/video` skill agent maps `DESIGN.md` tokens to composition behavior. Section 4 defaults
apply for any absent token before these mappings execute.

**Colors / Typography / Spacing / Rounded**: Injected into `:root` as CSS custom properties
before HTML loads. Pattern: `colors.<name>` → `--color-<name>`,
`typography.<role>.family` → `--typography-<role>-family`, `spacing.<name>` → `--spacing-<name>`,
`rounded.<name>` → `--rounded-<name>`. Components resolve `{token}` references and inject as CSS
class properties (single-level resolution).

**Motion tokens**: `ease_default` / `ease_entrance` / `ease_exit` → default `ease:` in GSAP
tweens (entrance/exit variants override); exposed as `--motion-ease_*` CSS variables.
`scene_duration_default` → default `data-duration` for clips. `stagger_default` → default
`stagger:`; exposed as `--motion-stagger_default`. `beat_interval` → planning target only; not
a hard clock. `layout_before_animation` → HTML comment only; no programmatic effect.

**Voice tokens**: `provider` / `voice_id` / `speed` → passed to `tts_synthesize_local` (kokoro)
or `tts_synthesize_api` (elevenlabs). `tone` → narration script register only (not sent to TTS):
`"energetic"` → short active sentences; `"warm"` → conversational; `"clinical"` → precise.
`pronunciation` → literal key-value substitution on transcripts before TTS synthesis.

**Aspect tokens**: `aspect.default` → canvas dimensions for `canvas_create_composition`:
`"16:9"` → 1920×1080, `"9:16"` → 1080×1920, `"1:1"` → 1080×1080. Explicit `width`/`height`
override enum defaults. `safe_zones.*` → `#stage` padding via `--aspect-safe_top`,
`--aspect-safe_bottom`, `--aspect-safe_sides`. Templates MUST use these variables; hardcoded
margin values are non-conformant.

---

## Examples

Three complete `DESIGN.md` files, each valid per the Google base spec and this extension.

### Example A: Minimal — Google base only; /video skill uses all defaults

No video extension keys. Parses as a valid Google `design.md`; `/video` applies all defaults
from the Fallback Table (power2.out easing, kokoro af voice, 16:9 at 1920×1080).

```yaml
---
name: "acme-corp"
version: "1.0.0"
description: "Clean corporate identity. No video extensions — all defaults apply."
colors:
  primary: "#0070f3"
  secondary: "#005bcc"
  accent: "#ff6b00"
  background: "#ffffff"
  foreground: "#111111"
typography:
  hero: { family: "Inter, sans-serif", weight: 700 }
  body: { family: "Inter, sans-serif", weight: 400 }
spacing: { xs: "4px", sm: "8px", md: "16px", lg: "24px", xl: "48px" }
rounded: { sm: "4px", md: "8px", lg: "16px" }
components:
  button: { bg: "{colors.primary}", fg: "{colors.foreground}", radius: "{rounded.md}" }
  card: { bg: "{colors.background}", border: "{colors.secondary}", radius: "{rounded.lg}" }
  caption_box: { bg: "{colors.primary}", fg: "#ffffff", radius: "{rounded.sm}", padding: "{spacing.sm} {spacing.md}" }
  lower_third: { accent_bar: "{colors.accent}", bg: "rgba(0,0,0,0.80)", radius: "{rounded.md}" }
---

### Overview
Blue/white corporate. All video tokens take Section 4 defaults.

### Do's and Don'ts
DO use `{colors.accent}` for exactly one focal point per scene.
DON'T use more than two typeface families.
```

### Example B: Brand-Styled — Full extension; warm 16:9, deliberate pacing

Complete video extension. Cinematic warm brand, kokoro bf (British female), 3s beat.

```yaml
---
name: "solaris-brand"
version: "1.0.0"
description: "Warm cinematic brand for Solaris energy company."
colors:
  primary: "#e8a020"
  secondary: "#b87818"
  accent: "#ff5c38"
  background: "#0d0a06"
  foreground: "#f5f0e8"
typography:
  hero: { family: "Playfair Display, Georgia, serif", weight: 700 }
  body: { family: "Inter, sans-serif", weight: 400 }
spacing: { xs: "4px", sm: "8px", md: "16px", lg: "24px", xl: "48px" }
rounded: { sm: "4px", md: "8px", lg: "16px" }
components:
  button: { bg: "{colors.primary}", fg: "{colors.background}", radius: "{rounded.md}" }
  card: { bg: "{colors.background}", border: "{colors.secondary}", radius: "{rounded.lg}" }
  caption_box: { bg: "{colors.primary}", fg: "{colors.background}", radius: "{rounded.sm}", padding: "{spacing.sm} {spacing.md}" }
  lower_third: { accent_bar: "{colors.accent}", bg: "rgba(13,10,6,0.88)", radius: "{rounded.md}" }
motion:
  ease_default: "power2.inOut"
  ease_entrance: "power3.out"
  ease_exit: "power3.in"
  scene_duration_default: "6s"
  stagger_default: "0.4s"
  beat_interval: "3s"
  layout_before_animation: true
voice:
  provider: "kokoro"
  voice_id: "bf"
  speed: 0.9
  tone: "warm"
  pronunciation: { "Solaris": "SOH-lah-ris", "kWh": "kilowatt hour" }
aspect:
  default: "16:9"
  width: 1920
  height: 1080
  safe_zones: { top: "120px", bottom: "160px", sides: "80px" }
---

### Overview
Dark warm cinematic. Gold/amber on deep black. 3s beat, deliberate pacing for investor audiences.

### Do's and Don'ts
DO allow 3s per beat. DON'T use glassmorphism — H.264 amplifies blur noise on dark backgrounds.

### Motion
power2.inOut default; power3.out entrances; power3.in exits. Beat 3s. Stagger 0.4s.

### Voice
Kokoro bf, 0.9× speed, warm tone. Pronunciation overrides for "Solaris" and "kWh".

### Aspect & Safe Zones
16:9 at 1920×1080. Safe: top 120px, bottom 160px, sides 80px.
```

### Example C: Vertical / TikTok — 9:16, fast motion, energetic voice

TikTok/Reels/Shorts. 9:16 aspect, bottom safe zone 220px, back.out(2.5) bounce, 1.5s beat,
1.2× speed. Font sizes MUST be scaled ×1.3 vs the 16:9 baseline.

```yaml
---
name: "neon-drop-tiktok"
version: "1.0.0"
description: "High-energy TikTok / Reels identity for NeonDrop streetwear brand."
colors:
  primary: "#00ff88"
  secondary: "#00cc6a"
  accent: "#ff00aa"
  background: "#000000"
  foreground: "#ffffff"
typography:
  hero: { family: "'Arial Black', 'Helvetica Neue', sans-serif", weight: 900 }
  body: { family: "Inter, sans-serif", weight: 700 }
spacing: { xs: "4px", sm: "8px", md: "16px", lg: "24px", xl: "48px" }
rounded: { sm: "0px", md: "4px", lg: "8px" }
components:
  button: { bg: "{colors.primary}", fg: "{colors.background}", radius: "{rounded.sm}" }
  card: { bg: "{colors.background}", border: "{colors.primary}", radius: "{rounded.md}" }
  caption_box: { bg: "{colors.primary}", fg: "{colors.background}", radius: "{rounded.sm}", padding: "{spacing.sm} {spacing.md}" }
  lower_third: { accent_bar: "{colors.accent}", bg: "rgba(0,0,0,0.90)", radius: "{rounded.sm}" }
motion:
  ease_default: "back.out(2)"
  ease_entrance: "back.out(2.5)"
  ease_exit: "power3.in"
  scene_duration_default: "3s"
  stagger_default: "0.1s"
  beat_interval: "1.5s"
  layout_before_animation: true
voice:
  provider: "kokoro"
  voice_id: "am"
  speed: 1.2
  tone: "energetic"
  pronunciation: { "NeonDrop": "Nee-on Drop", "collab": "cuh-LAB" }
aspect:
  default: "9:16"
  width: 1080
  height: 1920
  safe_zones: { top: "120px", bottom: "220px", sides: "60px" }
---

### Overview
Black/neon-green streetwear. 9:16 only. Beat every 1.5s. No filler.

### Do's and Don'ts
DO keep scenes under 3s. DO scale font sizes ×1.3. DON'T use serif or slow sine easings.

### Motion
back.out(2.5) entrances; power3.in exits. Stagger 0.1s for character bursts.

### Voice
Kokoro am, 1.2× speed, energetic tone. Pronunciation overrides for brand terms.

### Aspect & Safe Zones
9:16 at 1080×1920. Bottom 220px (TikTok caption reserve). Sides 60px.
```
