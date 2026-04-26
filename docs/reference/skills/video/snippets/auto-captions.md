---
name: auto-captions
purpose: Generate caption tracks from a composition's narration audio using tts_transcribe + caption-subtitle DOM patterns
tags: [captions, tts, transcribe, narration, accessibility]
---

# Auto-captions workflow

Auto-captions turn a composition's narration audio into timed caption clips
without the user writing them by hand. Workflow lives end-to-end inside the
agent: transcribe → group → emit DOM.

## When to use

- Composition has `narration.mp3` (from `tts_synthesize_api`) or
  `narration.wav` (Kokoro local) in its files map.
- User asks for "subtitles" / "captions" / "字幕" / accessibility, OR
- Composition is for a platform where captions are a default expectation
  (TikTok, YouTube Shorts).

Skip when:
- The video has its own embedded audio with burned-in captions.
- The narration is already shorter than ~3 s (captions add no value).

## Status

`tts_transcribe` is registered but its Whisper ONNX backend is gated on the
`nevoflux-tts` crate milestone — calling it today returns `ConfigMissing`
with setup hints. Until then, the agent can:

1. Use `tts_synthesize_api` with `composition_id` to get audio + an
   estimated `duration_sec`.
2. Skip auto-captions; ask the user to provide a script or accept silent
   playback.
3. Once Whisper lands, `tts_transcribe({composition_id, file_path: "narration.mp3"})`
   returns segment timestamps that drop straight into the DOM pattern below.

## Pipeline (when Whisper is wired)

```jsonc
{ "tool": "tts_transcribe",
  "args": { "composition_id": "comp-...", "file_path": "narration.mp3" },
  "returns": { "text": "...", "segments": [
    {"start_ms": 0,    "end_ms": 1240, "text": "Welcome to NevoFlux"},
    {"start_ms": 1240, "end_ms": 2980, "text": "your AI-native browser"}
  ] } }
```

Group segments into caption rows of 7–15 chars (per row) preferring
sentence breaks over length caps. For each row emit:

```html
<div class="clip caption-subtitle"
     data-start="<sec>"
     data-duration="<sec>"
     data-track-index="20"
     data-component="caption-subtitle">
  <span>Welcome to NevoFlux</span>
</div>
```

## DOM contract (enforced by linter)

Lint rules that gate this pattern (see `lib/composition-linter/rules/captions.js`):

| rule_id | enforces |
|---|---|
| `comp/caption-exit-missing-hard-kill` | every caption clip needs an exit tween that snaps `opacity:0` at `data-duration` |
| `comp/caption-text-overflow-risk` | caption text within 7-15 char row budget |
| `comp/caption-transcript-not-inline` | transcript JSON is read from artifact files, not inlined into HTML |
| `comp/caption-transcript-parse-error` | transcript JSON parses cleanly |
| `comp/caption-container-relative-position` | parent `.caption-track` is `position: relative` |
| `comp/caption-overflow-clips-scaled-words` | scaled words don't bleed past stage clip |
| `comp/caption-textshadow-on-group-container` | text-shadow on the group container, not per-word |
| `comp/caption-fittext-scale-mismatch` | scaled font size matches container scale |

The linter runs in strict mode on the daemon dispatch path
(`canvas_lint_composition` always passes `strict: true`), so violations
are blocking errors — design the caption emission to satisfy all eight
upfront.

## Constraints

- **One narration per composition** (enforced by `nf/single-audio`).
- **Narration ≤ composition duration**: clip captions to
  `min(segment.end_ms, composition.duration*1000)`.
- **Track index ≥ 20** for caption rows so they paint above scenes (per
  scene-transition-ironrules).
- **Caption clip = ONE caption row**, not a whole sentence — easier to
  re-time later.

## Failure mode reference

- `tts_transcribe` returns `ConfigMissing` → tell the user "auto-captions
  require the Whisper local model, which ships in the next nevoflux-tts
  milestone — for now I can render silent or with manual captions". DO NOT
  retry the call hoping for a different result.
- Transcript has fewer segments than expected (e.g. one giant segment for
  a 30 s narration) → Whisper's beam-search merged adjacent phrases;
  re-split on punctuation in the agent before emitting clips.
