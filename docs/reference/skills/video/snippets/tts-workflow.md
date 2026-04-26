---
name: tts-workflow
purpose: When and how to use TTS narration in compositions (provider choice, workflow, constraints)
tags: [tts, narration, kokoro, elevenlabs, workflow]
---

# TTS 工作流 / TTS Workflow

> **Status (2026-04-26):**
> - `tts_synthesize_api` (ElevenLabs HTTP) — **shipped** (P5b-1). Set `[tts.elevenlabs] api_key` in `~/.config/nevoflux/config.toml`.
> - `tts_synthesize_local` (Kokoro ONNX) — **registered, returns ConfigMissing**. Inference ships with the `nevoflux-tts` crate milestone.
> - `tts_transcribe` (Whisper ONNX) — **registered, returns ConfigMissing**. Ships alongside Kokoro.
> - Audio mux (ffmpeg `-c:a aac`) — **shipped** (P5b-final). When a composition contains `narration.mp3` or `narration.wav` in its files map, the renderer mounts a tempfile and muxes it as the second input.
>
> Today's narrated-video flow: use `tts_synthesize_api` with `composition_id` set, then `canvas_render_video` — the audio lands in the output MP4. Once Kokoro/Whisper land, the local path becomes the default and `tts_transcribe` unlocks auto-captions.

## When to Use TTS

- Mode 1 (zero-to-video) with a user-provided script
- Mode 3 (website-to-video) for narrator voice over extracted content  
- **DO NOT** use TTS for Mode 2 (video overlay) if the base `<video>` has its own audio (triggers `nf/single-audio`)

## Providers

| Provider | When | Constraints |
|---|---|---|
| **Kokoro (local)** | Default — 6 voices (af/am/bf/bm/zf/zm), fast, offline | Best for English + Chinese; moderate expressiveness |
| **ElevenLabs (API)** | User wants specific voice id / high expressiveness | Requires API key in config.toml; paid |
| **User-uploaded** | User provides narration.wav directly | Skip TTS entirely; call `tts_transcribe()` for word timings |

## Workflow (5 Steps)

1. Agent writes `SCRIPT.md` (narration text) into the composition's files map via `editArtifact`.
2. Agent calls `tts_synthesize_local({text, voice_id, speed})` OR `tts_synthesize_api({text, provider, voice_id})` → returns `assets/narration.wav` + `assets/transcript.json` (word-level timings).
3. Agent writes `STORYBOARD.md` aligning transcript word timings to scenes.
4. Agent emits `caption-subtitle` clips using transcript word groupings:
   - Group words by sentence boundary (min 7 / max 15 chars per caption row)
   - Prefer period breaks over length caps
5. Render — the composition's `<audio src="assets/narration.wav">` plays alongside video, with captions synced.

## Voice Selection

- `DESIGN.md voice.voice_id` takes precedence. Otherwise:
- Default: `af` (neutral, female, English-default)
- **Chinese content:** prefer `zf` (female) or `zm` (male)
- `DESIGN.md voice.tone` guides speed:
  - `neutral` → speed 1.0
  - `energetic` → speed 1.15
  - `warm` → speed 0.95
  - `clinical` → speed 1.05

## Pronunciation Overrides

User/`DESIGN.md` may specify `voice.pronunciation` map (e.g., `"API": "A P I"`). Agent pre-processes script text, substituting keys with phonetic values before TTS.

## Auto-Captions

Agent generates captions from `transcript.json`:

```js
const captions = groupWordsBySentence(transcript, { minChars: 7, maxChars: 15 });
for (const cap of captions) {
  emit(`<div class="clip caption-subtitle"
            data-start="${cap.startMs/1000}"
            data-duration="${(cap.endMs-cap.startMs)/1000}"
            data-track-index="20">${cap.text}</div>`);
}
```

No separate tool — agent does this inline using `components/caption-subtitle.html` pattern.

## Constraints

- One narration per composition (enforced by `nf/single-audio`)
- Narration duration ≤ composition duration
- Composition with `<video src>` audio → **no TTS narration allowed** (audio mutex)
- Kokoro Chinese quality is "usable, not stunning" — heavy production suggests ElevenLabs or user-recorded voice

## Enforced By

- `nf/single-audio` — only one audio source per composition
- Composition duration = max(narration.duration, longest scene + transitions)
