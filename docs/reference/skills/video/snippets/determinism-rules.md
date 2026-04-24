---
name: determinism-rules
purpose: Top-level rules for keeping a composition's rendering bit-for-bit reproducible
tags: [determinism, rendering, capture, rules]
---

# 确定性规则 / Determinism Rules

## 核心哲学 / Core Philosophy

Compositions must render deterministically: same artifact + same options + same runtime version = byte-identical MP4 (or frame-wise pixel equivalence, tolerating font rendering variance). Critical for CI regression testing (SHA256 validation), user trust (preview must match render), and concurrent safety (two clicks = two identical renders).

Browser defaults violate determinism throughout (random, wall-clock, vsync, async assets, font fallback). This guide enumerates required patches and Canvas runtime's contract injected into the composition iframe during render.

## Timing 规则 / Timing Rules

**RULE: No `setInterval`, `setTimeout`, hand-rolled `requestAnimationFrame` loops, `ScrollTrigger`, or `OrbitControls`.** All animation must go through GSAP timelines driven by the external render clock.

### Anti-Pattern: Do NOT Do This

```javascript
// ❌ FORBIDDEN — breaks determinism
setInterval(() => {
  el.style.transform = `rotate(${angle}deg)`;
  angle += 5;
}, 50);

// ❌ FORBIDDEN — hand-rolled RAF loop
function animate() {
  el.style.opacity = Math.random();
  requestAnimationFrame(animate);
}
animate();

// ❌ FORBIDDEN — setTimeout-based animation
setTimeout(() => {
  // animate something
}, 1000);
```

### Correct Pattern: Use GSAP Timelines

```javascript
// ✅ CORRECT — timeline driven by render clock
const tl = gsap.timeline({ paused: true });
tl.to(el, { rotation: 360, duration: 2 });
window.__timelines = window.__timelines || [];
window.__timelines.push(tl);

// Auto-play in preview mode (no render driver present).
// In render mode, __nfRenderTime is defined and drives tl.seek() externally.
if (typeof window.__nfRenderTime === 'undefined') tl.play();
```

## Random & Clock

Canvas runtime patches globals for determinism:

- **`Math.random`** — seeded Mulberry32 RNG (deterministic)
- **`Date.now()`** — returns composition timeline time
- **`performance.now()`** — returns composition timeline time (ms)
- **`window.__nfRenderTime`** — authoritative composition time (s)

Call freely; values are deterministic and identical on replay.

## External Resources

CDN whitelist: `esm.sh/gsap`, `esm.sh/three`, `esm.sh/lottie-web` (enforced by `nf/cdn-whitelist`).

All other assets in `assets/` directory. URLs outside `assets/` and CDN whitelist trigger `nf/ready-promises`.

## Ready Promises

For external resources (images, fonts, models) loading before frame 0:

```javascript
window.__readyPromises = window.__readyPromises || [];
window.__readyPromises.push(imageLoadPromise);
```

Render loop awaits all promises before frame capture.

## GSAP Rules

- **MUST use `gsap.timeline()` and push to `window.__timelines`** — the capture loop discovers timelines here
- **MUST create timelines with `{ paused: true }`** — otherwise GSAP self-drives on RAF, breaking determinism
- **SHOULD guard preview-mode auto-play with `if (typeof window.__nfRenderTime === 'undefined') tl.play();`** — otherwise the composition page shows a black/empty initial frame in the editor because nothing ever seeks the paused timeline
- **MUST NOT use `ScrollTrigger`, `ScrollSmoother`, `Observer`, `Draggable`** — depends on user input (meaningless in render mode)
- **MUST NOT use `repeat: -1` (infinite repeat)**  — architecture-level hard rule; calculate finite repeat count instead:
  ```javascript
  // ❌ Forbidden
  gsap.to(el, { rotation: 360, duration: 2, repeat: -1 });
  
  // ✅ Correct
  const compDuration = 10;  // from stage metadata
  const cycleDuration = 2;
  const repeats = Math.ceil(compDuration / cycleDuration) - 1;
  gsap.to(el, { rotation: 360, duration: cycleDuration, repeat: repeats });
  ```

The reason: Canvas drives GSAP by advancing root time each frame (`gsap.updateRoot(t)`). If a tween declares `repeat: -1`, its duration becomes infinite, breaking timeline.duration() calculations and seek() behavior.

## Three.js / WebGL

Required initialization:

```javascript
const renderer = new THREE.WebGLRenderer({
  preserveDrawingBuffer: true,  // ← REQUIRED for frame capture
  antialias: true,
  alpha: false,  // recommended for composition
});

// Must register for re-render each frame
window.__threeRenderers = window.__threeRenderers || [];
window.__threeRenderers.push({ renderer, scene, camera });
```

Skeletal animation **must** use timeline time, not delta:

```javascript
// ✅ Correct
const mixer = new THREE.AnimationMixer(model);
const action = mixer.clipAction(animationClip);
action.play();

window.NevofluxSDK.timeline.onFrame((t) => {
  mixer.setTime(t);  // composition time, deterministic
});

// ❌ Wrong
mixer.update(deltaTime);  // wall-clock delta, non-deterministic
```

Forbidden Three.js APIs: `OrbitControls`, `DragControls`, `TransformControls`, `PointerLockControls`, `renderer.setAnimationLoop()`, `Stats.js`, `dat.gui`, WebXR.

## Audio & Scripts

**One audio source:** mutex among `<audio>` element, TTS narration, `<video>` audio (enforced by `nf/single-audio` ERROR).

**Scripts:** ES-module scripts exempt from syntax checking (CSP blocks `new Function()`); plain `<script>` is checked.

## Enforced By

**`nf/forbidden-apis`** (ERROR) — Rejects `setInterval`, `setTimeout`, `ScrollTrigger`, `OrbitControls`, hand-rolled RAF loops

**`nf/cdn-whitelist`** (WARNING) — Flags non-whitelisted CDN URLs

**`nf/ready-promises`** (INFO) — Detects `<img>` without `window.__readyPromises` registration

**`nf/three-renderer`** (ERROR) — Requires `preserveDrawingBuffer: true`

**`nf/three-register`** (WARNING) — Detects WebGLRenderer without `window.__threeRenderers` push

**`nf/mixer-settime`** (WARNING) — Flags `mixer.update(delta)` (use `setTime(t)`)

**`nf/single-audio`** (ERROR) — Rejects multiple audio sources
