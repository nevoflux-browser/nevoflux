// composition-linter/rules/nevoflux.js — NevoFlux-specific determinism rules.
// See ../LICENSE-NOTICE.md (these rules are NOT ported — they are original).

import { push, findNodeLineCol, isCdnWhitelisted } from '../utils.js';

// ── nf/ready-promises ─────────────────────────────────────────────────────────
//
// Non-whitelisted <img> / external resources must register their load-promise
// in window.__readyPromises so the render loop waits.

function ruleReadyPromises(ctx, report) {
  const registers = ctx.scripts.some(s =>
    (s.textContent || '').includes('window.__readyPromises'));
  for (const img of ctx.doc.querySelectorAll('img')) {
    const src = img.getAttribute('src') || '';
    if (!src.startsWith('assets/') && !registers) {
      const { line, col } = findNodeLineCol(ctx.raw, img);
      push(report, {
        severity: 'info',
        rule_id: 'nf/ready-promises',
        message: `<img src="${src}"> outside assets/ without a window.__readyPromises registration.`,
        line, col,
        fix_hint: 'Push a loaded-promise into window.__readyPromises so the render loop waits.',
      });
    }
  }
}

// ── nf/three-renderer ─────────────────────────────────────────────────────────
//
// new THREE.WebGLRenderer({...}) must include preserveDrawingBuffer: true.

function ruleThreeRenderer(ctx, report) {
  for (const s of ctx.scripts) {
    const src = s.textContent || '';
    const m = /new\s+THREE\.WebGLRenderer\s*\(\s*(\{[\s\S]*?\})/.exec(src);
    if (!m) continue;
    if (!/preserveDrawingBuffer\s*:\s*true/.test(m[1])) {
      push(report, {
        severity: 'error',
        rule_id: 'nf/three-renderer',
        message: 'THREE.WebGLRenderer must be constructed with { preserveDrawingBuffer: true } for deterministic capture.',
        fix_hint: 'Add preserveDrawingBuffer: true to the WebGLRenderer options.',
      });
    }
  }
}

// ── nf/three-register ─────────────────────────────────────────────────────────
//
// If a THREE.WebGLRenderer is constructed, it must be pushed onto
// window.__threeRenderers so the capture loop can re-render it each frame.

function ruleThreeRegister(ctx, report) {
  const hasRendererCtor = ctx.scripts.some(s =>
    /new\s+THREE\.WebGLRenderer/.test(s.textContent || ''));
  if (!hasRendererCtor) return;
  const registers = ctx.scripts.some(s =>
    /window\.__threeRenderers/.test(s.textContent || ''));
  if (!registers) {
    push(report, {
      severity: 'warning',
      rule_id: 'nf/three-register',
      message: 'Three.js renderer must be pushed onto window.__threeRenderers so the capture loop can re-render it each frame.',
      fix_hint: 'window.__threeRenderers = window.__threeRenderers || []; window.__threeRenderers.push({ renderer, scene, camera });',
    });
  }
}

// ── nf/mixer-settime ──────────────────────────────────────────────────────────
//
// THREE.AnimationMixer must be driven with .setTime(t), NOT .update(delta).

function ruleMixerSetTime(ctx, report) {
  for (const s of ctx.scripts) {
    const src = s.textContent || '';
    if (!/AnimationMixer/.test(src)) continue;
    if (/\.update\s*\(/.test(src)) {
      push(report, {
        severity: 'warning',
        rule_id: 'nf/mixer-settime',
        message: 'THREE.AnimationMixer must be driven with .setTime(t), not .update(delta), for deterministic capture.',
        fix_hint: 'Replace mixer.update(delta) with mixer.setTime(window.__hfRenderTime).',
      });
    }
  }
}

// ── nf/forbidden-apis ─────────────────────────────────────────────────────────
//
// Error on: ScrollTrigger, OrbitControls, setInterval(, hand-rolled rAF loops.

function ruleForbiddenApis(ctx, report) {
  const forbidden = [
    { pat: /\bScrollTrigger\b/,  msg: 'ScrollTrigger is forbidden in compositions.' },
    { pat: /\bOrbitControls\b/,  msg: 'OrbitControls is forbidden (non-deterministic user input).' },
    { pat: /\bsetInterval\s*\(/, msg: 'setInterval is forbidden (not driven by the render clock).' },
    { pat: /requestAnimationFrame\s*\([^)]+\)\s*;[\s\S]{0,400}?requestAnimationFrame\s*\(/,
      msg: 'Hand-rolled requestAnimationFrame loops are forbidden; use GSAP timelines instead.' },
  ];
  for (const s of ctx.scripts) {
    const src = s.textContent || '';
    for (const { pat, msg } of forbidden) {
      if (pat.test(src)) {
        push(report, { severity: 'error', rule_id: 'nf/forbidden-apis', message: msg });
      }
    }
  }
}

// ── nf/cdn-whitelist ──────────────────────────────────────────────────────────
//
// Only esm.sh/{gsap,three,lottie-web} allowed as CDN sources.

function ruleCdnWhitelist(ctx, report) {
  for (const s of ctx.doc.querySelectorAll('script[src]')) {
    const url = s.getAttribute('src') || '';
    if (/^https?:\/\//.test(url) && !isCdnWhitelisted(url)) {
      const { line, col } = findNodeLineCol(ctx.raw, s);
      push(report, {
        severity: 'warning',
        rule_id: 'nf/cdn-whitelist',
        message: `<script src="${url}"> is outside the allowed CDN whitelist (esm.sh/gsap, esm.sh/three, esm.sh/lottie-web).`,
        line, col,
        fix_hint: 'Switch to an esm.sh URL pinned to an allowed library version.',
      });
    }
  }
}

// ── nf/single-audio ───────────────────────────────────────────────────────────
//
// At most one of: <audio>, TTS narration, <video> audio track.

function ruleSingleAudio(ctx, report) {
  const audios = ctx.doc.querySelectorAll('audio');
  const videos = ctx.doc.querySelectorAll('video');
  const ttsRefs = ctx.scripts.filter(s =>
    /narration\.wav|tts_synthesize/.test(s.textContent || ''));
  let sources = 0;
  if (audios.length > 0) sources++;
  if (videos.length > 0) sources++;
  if (ttsRefs.length > 0) sources++;
  if (sources > 1) {
    push(report, {
      severity: 'error',
      rule_id: 'nf/single-audio',
      message: `Composition has ${sources} audio sources; v1 allows only one (<audio>, TTS narration, or <video> audio — mutex).`,
      fix_hint: 'Remove all but one audio source.',
    });
  }
}

export default [
  ruleReadyPromises,
  ruleThreeRenderer,
  ruleThreeRegister,
  ruleMixerSetTime,
  ruleForbiddenApis,
  ruleCdnWhitelist,
  ruleSingleAudio,
];
