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
        fix_hint: 'Replace mixer.update(delta) with mixer.setTime(window.__nfRenderTime).',
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
    /narration\.(wav|mp3)|tts_synthesize/.test(s.textContent || ''));
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

// ── nf/harsh-flash-transition ──────────────────────────────────────────────
//
// A full-screen overlay (position:absolute/fixed covering the stage via inset:0
// or all four offsets) with a FLAT, bright, near-opaque background and NO
// additive blend mode, whose opacity is GSAP-animated to a high value, renders
// as a harsh white-out "flash" at scene transitions. Real light-leaks use a
// soft gradient + `mix-blend-mode: screen` at low opacity and are exempt
// (gradient backgrounds and screen/lighten blends are both excluded below).

const ADDITIVE_BLEND_RE =
  /mix-blend-mode\s*:\s*(screen|lighten|color-dodge|plus-lighter|hard-light)/i;
const FLASH_OPACITY_THRESHOLD = 0.5;

/** Parse a flat CSS color → { alpha, light } or null if not a flat fill (e.g. a gradient). */
function parseFlatBackground(value) {
  if (!value) return null;
  const v = value.trim();
  if (/gradient\s*\(/i.test(v)) return null; // gradients are soft, not a flat flash
  let m = v.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+)\s*)?\)/i);
  if (m) {
    const a = m[4] !== undefined ? parseFloat(m[4]) : 1;
    return { alpha: a, light: (+m[1] + +m[2] + +m[3]) / 3 >= 150 };
  }
  m = v.match(/#([0-9a-fA-F]{6})\b/);
  if (m) {
    const r = parseInt(m[1].slice(0, 2), 16), g = parseInt(m[1].slice(2, 4), 16), b = parseInt(m[1].slice(4, 6), 16);
    return { alpha: 1, light: (r + g + b) / 3 >= 150 };
  }
  if (/\b(white|ivory|snow|whitesmoke|floralwhite|cornsilk)\b/i.test(v)) return { alpha: 1, light: true };
  return null;
}

/** Merge CSS declarations that apply to `el` (matching id/class rules + inline style). */
function collectAppliedDeclarations(el, styles) {
  const selectors = new Set();
  const id = el.getAttribute('id');
  if (id) selectors.add(`#${id}`);
  for (const c of (el.getAttribute('class') || '').split(/\s+/).filter(Boolean)) selectors.add(`.${c}`);
  let body = '';
  for (const styleEl of styles) {
    const css = styleEl.textContent || '';
    for (const [, sel, decl] of css.matchAll(/([#.][A-Za-z0-9_-]+)\s*\{([^}]+)\}/g)) {
      if (selectors.has((sel || '').trim())) body += ';' + (decl || '');
    }
  }
  return body + ';' + (el.getAttribute('style') || '');
}

/** True if the merged declarations make the element cover the whole stage. */
function isFullBleed(decl) {
  if (!/position\s*:\s*(absolute|fixed)/i.test(decl)) return false;
  if (/inset\s*:\s*0(\s+0)*\s*(;|$)/i.test(decl)) return true;
  const zero = (p) => new RegExp(`\\b${p}\\s*:\\s*0\\b`, 'i').test(decl);
  if (zero('top') && zero('right') && zero('bottom') && zero('left')) return true;
  return /width\s*:\s*100%/.test(decl) && /height\s*:\s*100%/.test(decl);
}

/** Extract { selector, peak } for to/fromTo tweens that animate opacity (peak = the "to" value). */
function extractOpacityPeaks(scriptText) {
  const out = [];
  const re = /\.(to|fromTo)\s*\(\s*["']([^"']+)["']\s*,\s*(\{[^}]*\})\s*(?:,\s*(\{[^}]*\}))?/g;
  let m;
  while ((m = re.exec(scriptText)) !== null) {
    const toVars = m[1] === 'fromTo' ? (m[4] || '') : (m[3] || '');
    const om = /opacity\s*:\s*([\d.]+)/.exec(toVars);
    if (om) out.push({ selector: m[2], peak: parseFloat(om[1]) });
  }
  return out;
}

function ruleHarshFlashTransition(ctx, report) {
  const peaks = [];
  for (const s of ctx.scripts) {
    if (s.getAttribute('src')) continue;
    for (const p of extractOpacityPeaks(s.textContent || '')) peaks.push(p);
  }
  if (peaks.length === 0) return;

  const flagged = new Set();
  for (const { selector, peak } of peaks) {
    if (peak < FLASH_OPACITY_THRESHOLD) continue;
    let els;
    try { els = ctx.doc.querySelectorAll(selector); } catch { continue; }
    for (const el of els) {
      const decl = collectAppliedDeclarations(el, ctx.styles);
      if (!isFullBleed(decl)) continue;
      if (ADDITIVE_BLEND_RE.test(decl)) continue; // soft additive overlay — fine
      const bgMatch = decl.match(/background(?:-color)?\s*:\s*([^;]+)/i);
      const bg = bgMatch ? parseFlatBackground(bgMatch[1]) : null;
      if (!bg || !bg.light || bg.alpha < 0.5) continue; // not a flat bright opaque fill
      const key = el.getAttribute('id') || el.getAttribute('class') || selector;
      if (flagged.has(key)) continue;
      flagged.add(key);
      const { line, col } = findNodeLineCol(ctx.raw, el);
      push(report, {
        severity: 'warning',
        rule_id: 'nf/harsh-flash-transition',
        message:
          `Full-screen overlay "${selector}" is animated to opacity ${peak} over a flat, ` +
          `near-opaque bright background with no additive blend mode — this renders as a harsh ` +
          `white-out flash at the scene transition.`,
        line, col,
        fix_hint:
          'Cap the peak opacity at ≤ 0.35 and use a soft radial gradient with ' +
          '`mix-blend-mode: screen`, or cross-fade the scenes (overlap fade-out/fade-in) ' +
          'instead of flashing through a full-screen overlay.',
      });
    }
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
  ruleHarshFlashTransition,
];
