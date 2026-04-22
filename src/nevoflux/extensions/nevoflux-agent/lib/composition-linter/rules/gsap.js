// composition-linter/rules/gsap.js — ported from the upstream GSAP rule set.
// Rule IDs use the `comp/*` prefix.
// See ../LICENSE-NOTICE.md for attribution.
//
// Adaptation notes:
//
//   Upstream uses a dedicated `parseGsapScript()` parser that returns typed
//   GsapWindow objects with resolved targetSelector, position, end, properties,
//   and overwriteAuto fields.  We do NOT have that parser, so the three rules
//   that depended on it (`overlapping_gsap_tweens`, `gsap_animates_clip_element`,
//   `unscoped_gsap_selector`) are ported as regex-on-script-text analyses and
//   are documented as NARROWED below.  The simpler rules (`missing_gsap_script`,
//   `gsap_infinite_repeat`, `gsap_repeat_ceil_overshoot`, etc.) are ported
//   verbatim-equivalent.
//
//   Upstream also used `OpenTag` objects (raw tag strings from a streaming
//   parser) accessed via `readAttr(tag.raw, attr)`.  We use DOM element objects
//   accessible via `ctx.doc.querySelectorAll(...)`.

import { push } from '../utils.js';

// ── Helpers ────────────────────────────────────────────────────────────────

/** Truncate a code snippet to `max` chars for issue messages. */
function truncateSnippet(str, max = 120) {
  if (!str) return undefined;
  const s = str.replace(/\s+/g, ' ').trim();
  if (!s) return undefined;
  return s.length > max ? s.slice(0, max) + '…' : s;
}

/**
 * WINDOW_TIMELINE_ASSIGN_PATTERN — matches `window.__timelines["id"] = tl`
 * and extracts the composition ID from the bracket key.
 * Equivalent to the upstream `WINDOW_TIMELINE_ASSIGN_PATTERN` utility.
 */
const WINDOW_TIMELINE_ASSIGN_RE = /window\.__timelines\[\s*["']([^"']+)["']\s*\]\s*=/;

/** Return the composition ID registered by this script, or null. */
function readRegisteredTimelineCompId(scriptText) {
  const m = scriptText.match(WINDOW_TIMELINE_ASSIGN_RE);
  return m?.[1] || null;
}

/**
 * Return true if `selector` looks like an unscoped class/tag selector that
 * could match elements outside the owning composition.
 */
function isSuspiciousGlobalSelector(selector) {
  if (!selector) return false;
  if (selector.includes('[data-composition-id=')) return false;
  if (selector.startsWith('#')) return false;
  return selector.startsWith('.') || /^[a-z]/i.test(selector);
}

/**
 * If `selector` is a single-class selector (`.foo`), return `"foo"`.
 * Otherwise return null.
 */
function getSingleClassSelector(selector) {
  const m = selector.trim().match(/^\.([A-Za-z0-9_-]+)$/);
  return m ? m[1] : null;
}

/**
 * Parse all GSAP tween calls in a script block using regex.
 *
 * Returns an array of lightweight tween descriptors:
 *   { selector, method, duration, repeat, properties, overwriteAuto, raw }
 *
 * NARROWED vs upstream: upstream runs a full recursive parser (parseGsapScript)
 * that resolves exact timeline positions (start, end) and supports chained
 * calls.  This version uses regex-based extraction without position tracking,
 * so `overlapping_gsap_tweens` cannot compute overlap times.  The overlap
 * check is therefore replaced with a simpler "same selector, same property,
 * non-overwrite" heuristic (documented in the rule below).
 */
function extractGsapTweens(scriptText) {
  if (!/gsap\.timeline/.test(scriptText)) return [];

  // Detect the variable name that holds the timeline.
  // Typical patterns: `const tl = gsap.timeline(...)` or `var timeline = gsap.timeline(...)`
  const tlVarMatch = scriptText.match(/(?:const|let|var)\s+(\w+)\s*=\s*gsap\.timeline\s*\(/);
  const tlVar = tlVarMatch?.[1] ?? 'tl';

  const tweens = [];
  // Match tl.to/from/fromTo/set("selector", { ... }) — simplified, no nested parens.
  // We match up to the first ')' that closes the outer call; this misses complex
  // nested structures but captures the vast majority of real composition tweens.
  const callPattern = new RegExp(
    `${tlVar}\\.(set|to|from|fromTo)\\s*\\(\\s*["']([^"']+)["']\\s*,\\s*(\\{[^}]*\\})`,
    'g',
  );
  let m;
  while ((m = callPattern.exec(scriptText)) !== null) {
    const method = m[1] ?? 'to';
    const selector = m[2] ?? '';
    const propsRaw = m[3] ?? '{}';
    const raw = m[0];

    // Parse property keys and some numeric values from the props object literal.
    const propKeys = [];
    let duration = 0;
    let repeat = 0;
    let overwriteAuto = false;
    const META_KEYS = new Set(['duration', 'ease', 'repeat', 'yoyo', 'overwrite', 'delay']);
    const propPattern = /(\w+)\s*:\s*("[^"]*"|'[^']*'|-?[\d.]+|true|false|"auto"|'auto'|[\w.]+)/g;
    let pm;
    while ((pm = propPattern.exec(propsRaw)) !== null) {
      const key = pm[1] ?? '';
      const val = pm[2] ?? '';
      if (key === 'duration') duration = parseFloat(val) || 0;
      if (key === 'repeat') repeat = parseInt(val, 10) || 0;
      if (key === 'overwrite') overwriteAuto = val.replace(/["']/g, '') === 'auto';
      if (!META_KEYS.has(key)) propKeys.push(key);
    }

    tweens.push({ selector, method, duration, repeat, overwriteAuto, properties: propKeys, raw });
  }
  return tweens;
}

// ── META: class-usage counter (port of upstream countClassUsage) ───────────

/** Count how many DOM elements carry each CSS class name. */
function countClassUsage(doc) {
  const counts = new Map();
  for (const el of doc.querySelectorAll('[class]')) {
    for (const cls of (el.getAttribute('class') || '').split(/\s+/).filter(Boolean)) {
      counts.set(cls, (counts.get(cls) || 0) + 1);
    }
  }
  return counts;
}

// ── CSS-transform helpers (port of upstream cssTransformToGsapProps) ────────

function cssTransformToGsapProps(cssTransform) {
  const parts = [];
  const translateMatch = cssTransform.match(
    /translate\(\s*(-?[\d.]+)(%|px)?\s*,\s*(-?[\d.]+)(%|px)?\s*\)/,
  );
  if (translateMatch) {
    const [, xVal, xUnit, yVal, yUnit] = translateMatch;
    parts.push(xUnit === '%' ? `xPercent: ${xVal}` : `x: ${xVal}`);
    parts.push(yUnit === '%' ? `yPercent: ${yVal}` : `y: ${yVal}`);
  }
  const txMatch = cssTransform.match(/translateX\(\s*(-?[\d.]+)(%|px)?\s*\)/);
  if (txMatch) {
    const [, val, unit] = txMatch;
    parts.push(unit === '%' ? `xPercent: ${val}` : `x: ${val}`);
  }
  const tyMatch = cssTransform.match(/translateY\(\s*(-?[\d.]+)(%|px)?\s*\)/);
  if (tyMatch) {
    const [, val, unit] = tyMatch;
    parts.push(unit === '%' ? `yPercent: ${val}` : `y: ${val}`);
  }
  const scaleMatch = cssTransform.match(/scale\(\s*([\d.]+)\s*\)/);
  if (scaleMatch) parts.push(`scale: ${scaleMatch[1]}`);
  return parts.length > 0 ? parts.join(', ') : null;
}

// ── Rule: overlapping_gsap_tweens + gsap_animates_clip_element +
//          unscoped_gsap_selector ────────────────────────────────────────────
//
// These three upstream rules were co-located because they all iterate over
// GSAP tween windows extracted by the full parser.  We consolidate them here.
//
// NARROWED (overlapping_gsap_tweens):
//   Upstream used exact timeline positions (start/end times) computed by
//   parseGsapScript.  Without that parser we detect "same selector + same
//   property + no overwrite" pairs across all tweens in a script, regardless
//   of actual timeline position.  This produces fewer false positives (same
//   selector different properties are fine) but cannot compute the precise
//   overlap interval.  The message reflects this.
//
// NARROWED (gsap_animates_clip_element):
//   Upstream resolved elements via the tag list from a streaming parser.
//   We use DOM querySelectorAll to find elements with class="clip".

function ruleGsapTweenChecks(ctx, report) {
  // Build clip-element look-up maps from the DOM.
  // Keyed by CSS selector: #id or .classname
  const clipById = new Map();   // "#id" → { tag, id, classes }
  const clipByClass = new Map(); // ".cls" → { tag, id, classes }

  for (const el of ctx.doc.querySelectorAll('.clip')) {
    const classes = (el.getAttribute('class') || '').split(/\s+/).filter(Boolean);
    if (!classes.includes('clip')) continue;
    const id = el.getAttribute('id') || '';
    const info = { tag: el.tagName.toLowerCase(), id, classes: el.getAttribute('class') || '' };
    if (id) clipById.set(`#${id}`, info);
    for (const cls of classes) {
      if (cls !== 'clip') clipByClass.set(`.${cls}`, info);
    }
  }

  const classUsage = countClassUsage(ctx.doc);
  const rootCompId = ctx.composition_id || null;

  for (const script of ctx.scripts) {
    const content = script.textContent || '';
    // Skip external scripts
    if (script.getAttribute('src')) continue;
    if (!/gsap\.timeline/.test(content)) continue;

    const localTimelineCompId = readRegisteredTimelineCompId(content);
    const tweens = extractGsapTweens(content);

    // overlapping_gsap_tweens — heuristic: flag same-selector same-property pairs
    // without overwrite:"auto" on either tween.
    // NARROWED: no timeline position data, so we cannot compute the overlap
    // interval; we report the pairing as a potential conflict.
    const selectorPropMap = new Map(); // selector → Map<prop, tween>
    for (const tween of tweens) {
      if (tween.overwriteAuto) continue;
      const sel = tween.selector;
      const existing = selectorPropMap.get(sel);
      if (!existing) {
        const propMap = new Map();
        for (const p of tween.properties) propMap.set(p, tween);
        selectorPropMap.set(sel, propMap);
        continue;
      }
      const shared = tween.properties.filter(p => existing.has(p));
      if (shared.length === 0) {
        for (const p of tween.properties) existing.set(p, tween);
        continue;
      }
      const firstTween = existing.get(shared[0]);
      push(report, {
        severity: 'warning',
        rule_id: 'comp/overlapping-gsap-tweens',
        message:
          `GSAP tweens both target "${sel}" and animate ${shared.join(', ')} without \`overwrite: "auto"\`. ` +
          `If they overlap on the timeline they will conflict.`,
        selector: sel,
        fix_hint: `Shorten the earlier tween, stagger the later tween, or add \`overwrite: "auto"\` to one of them.`,
        snippet: truncateSnippet(firstTween ? `${firstTween.raw}\n${tween.raw}` : tween.raw),
      });
      // Record this tween's properties to detect further conflicts.
      for (const p of tween.properties) existing.set(p, tween);
    }

    // gsap_animates_clip_element — only flag visibility/display properties.
    for (const tween of tweens) {
      const sel = tween.selector;
      const clipInfo = clipById.get(sel) || clipByClass.get(sel);
      if (!clipInfo) continue;
      const conflicting = tween.properties.filter(p => p === 'visibility' || p === 'display');
      if (conflicting.length === 0) continue;
      const elDesc = `<${clipInfo.tag}${clipInfo.id ? ` id="${clipInfo.id}"` : ''} class="${clipInfo.classes}">`;
      push(report, {
        severity: 'error',
        rule_id: 'comp/gsap-animates-clip-element',
        message:
          `GSAP animation sets ${conflicting.join(', ')} on a clip element. ` +
          `Selector "${sel}" resolves to ${elDesc}. ` +
          `The framework manages clip visibility via ${conflicting.join('/')} — do not animate these properties on clip elements.`,
        selector: sel,
        fix_hint:
          'Remove the visibility/display tween, or move the content into a child <div> and target that instead.',
        snippet: truncateSnippet(tween.raw),
      });
    }

    // unscoped_gsap_selector
    // Only fires when the timeline is NOT the root composition's own timeline
    // (i.e. it belongs to a sub-composition that registers a different ID).
    if (!localTimelineCompId || localTimelineCompId === rootCompId) continue;
    for (const tween of tweens) {
      if (!isSuspiciousGlobalSelector(tween.selector)) continue;
      const className = getSingleClassSelector(tween.selector);
      if (className && (classUsage.get(className) || 0) < 2) continue;
      push(report, {
        severity: 'warning',
        rule_id: 'comp/unscoped-gsap-selector',
        message:
          `Timeline "${localTimelineCompId}" uses unscoped selector "${tween.selector}" that will target elements in ALL compositions when bundled, causing data loss (opacity, transforms, etc.).`,
        selector: tween.selector,
        fix_hint:
          `Scope the selector: \`[data-composition-id="${localTimelineCompId}"] ${tween.selector}\` or use a unique id.`,
        snippet: truncateSnippet(tween.raw),
      });
    }
  }
}

// ── Rule: gsap_css_transform_conflict ─────────────────────────────────────
//
// NARROWED vs upstream: upstream read inline `style` attributes via
// `readAttr(tag.raw, "style")` from the streaming-parser tag list.
// We use DOM querySelectorAll + getAttribute for the same effect.

function ruleGsapCssTransformConflict(ctx, report) {
  const cssTranslateSelectors = new Map(); // css-selector → transform value
  const cssScaleSelectors = new Map();

  // Scan <style> blocks
  for (const styleEl of ctx.styles) {
    const css = styleEl.textContent || '';
    for (const [, selector, body] of css.matchAll(/([#.][a-zA-Z0-9_-]+)\s*\{([^}]+)\}/g)) {
      const tMatch = (body ?? '').match(/transform\s*:\s*([^;]+)/);
      if (!tMatch || !tMatch[1]) continue;
      const tv = tMatch[1].trim();
      const sel = (selector ?? '').trim();
      if (/translate/i.test(tv)) cssTranslateSelectors.set(sel, tv);
      if (/scale/i.test(tv)) cssScaleSelectors.set(sel, tv);
    }
  }

  // Scan inline style attributes on DOM elements
  for (const el of ctx.doc.querySelectorAll('[style]')) {
    const inlineStyle = el.getAttribute('style') || '';
    const tMatch = inlineStyle.match(/transform\s*:\s*([^;]+)/);
    if (!tMatch || !tMatch[1]) continue;
    const tv = tMatch[1].trim();
    const id = el.getAttribute('id');
    const selectors = [];
    if (id) selectors.push(`#${id}`);
    for (const cls of (el.getAttribute('class') || '').split(/\s+/).filter(Boolean)) {
      selectors.push(`.${cls}`);
    }
    for (const sel of selectors) {
      if (/translate/i.test(tv) && !cssTranslateSelectors.has(sel))
        cssTranslateSelectors.set(sel, tv);
      if (/scale/i.test(tv) && !cssScaleSelectors.has(sel))
        cssScaleSelectors.set(sel, tv);
    }
  }

  if (cssTranslateSelectors.size === 0 && cssScaleSelectors.size === 0) return;

  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';
    if (!/gsap\.timeline/.test(content)) continue;

    const tweens = extractGsapTweens(content);

    // sel → { cssTransform, props: Set<string>, raw }
    const conflicts = new Map();

    for (const tween of tweens) {
      if (tween.method === 'fromTo') continue; // fromTo is exempt per upstream
      const sel = tween.selector;
      const cssKey = sel.startsWith('#') || sel.startsWith('.') ? sel : `#${sel}`;

      const translateProps = tween.properties.filter(p =>
        ['x', 'y', 'xPercent', 'yPercent'].includes(p),
      );
      const scaleProps = tween.properties.filter(p => p === 'scale');
      const cssFromTranslate =
        translateProps.length > 0 ? cssTranslateSelectors.get(cssKey) : undefined;
      const cssFromScale = scaleProps.length > 0 ? cssScaleSelectors.get(cssKey) : undefined;
      if (!cssFromTranslate && !cssFromScale) continue;

      const existing = conflicts.get(sel) ?? {
        cssTransform: [cssFromTranslate, cssFromScale].filter(Boolean).join(' '),
        props: new Set(),
        raw: tween.raw,
      };
      for (const p of [...translateProps, ...scaleProps]) existing.props.add(p);
      conflicts.set(sel, existing);
    }

    for (const [sel, { cssTransform, props, raw }] of conflicts) {
      const propList = [...props].join('/');
      const gsapEquivalent = cssTransformToGsapProps(cssTransform);
      const fix_hint = gsapEquivalent
        ? `Remove \`transform: ${cssTransform}\` from CSS and replace with GSAP properties: ${gsapEquivalent}. ` +
          `Example: tl.fromTo('${sel}', { ${gsapEquivalent} }, { ${gsapEquivalent}, ...yourAnimation }). ` +
          `tl.fromTo is exempt from this rule.`
        : `Remove the transform from CSS and use tl.fromTo('${sel}', ` +
          `{ xPercent: -50, x: -1000 }, { xPercent: -50, x: 0 }) so GSAP owns ` +
          `the full transform state. tl.fromTo is exempt from this rule.`;
      push(report, {
        severity: 'warning',
        rule_id: 'comp/gsap-css-transform-conflict',
        message:
          `"${sel}" has CSS \`transform: ${cssTransform}\` and a GSAP tween animates ` +
          `${propList}. GSAP will overwrite the full CSS transform, discarding any ` +
          `translateX(-50%) centering or CSS scale value.`,
        selector: sel,
        fix_hint,
        snippet: truncateSnippet(raw),
      });
    }
  }
}

// ── Rule: missing_gsap_script ──────────────────────────────────────────────
//
// Ported essentially verbatim from upstream; adapted to use DOM script elements.

function ruleMissingGsapScript(ctx, report) {
  const usesGsap = ctx.scripts.some(s => {
    if (s.getAttribute('src')) return false; // external; content is empty in DOM
    return /gsap\.(to|from|fromTo|timeline|set|registerPlugin)\b/.test(s.textContent || '');
  });
  if (!usesGsap) return;

  const hasGsapSrc = ctx.scripts.some(s => {
    const src = s.getAttribute('src') || '';
    if (/gsap/i.test(src)) return true;
    // ES module: `import ... from 'https://esm.sh/gsap...'` or similar
    const content = s.textContent || '';
    if (/import\s+[^;]*\bfrom\s+['"][^'"]*gsap[^'"]*['"]/i.test(content)) return true;
    return false;
  });

  // Detect GSAP bundled inline (no src).
  const hasInlineGsap = ctx.scripts.some(s => {
    if (s.getAttribute('src')) return false;
    const t = s.textContent || '';
    return (
      /\/\*\s*inlined:.*gsap/i.test(t) ||
      /\b_gsScope\b/.test(t) ||
      /\bGreenSock\b/.test(t) ||
      /\bgsap\.(config|defaults|version)\b/.test(t) ||
      (t.length > 5000 && /\bgsap\b/i.test(t))
    );
  });

  if (hasGsapSrc || hasInlineGsap) return;
  push(report, {
    severity: 'error',
    rule_id: 'comp/missing-gsap-script',
    message: 'Composition uses GSAP but no GSAP script is loaded. The animation will not run.',
    fix_hint:
      'Add <script src="https://cdn.jsdelivr.net/npm/gsap@3/dist/gsap.min.js"></script> before your animation script.',
  });
}

// ── Rule: audio_reactive_single_tween_per_group ────────────────────────────
//
// Ported verbatim from upstream; adapted to use DOM script/style elements.

function ruleAudioReactiveSingleTweenPerGroup(ctx, report) {
  // Only applies to caption-style compositions.
  const isCaptionFile = ctx.styles.some(s =>
    /\.caption[-_]?(?:group|word)/i.test(s.textContent || ''),
  );
  if (!isCaptionFile) return;

  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';
    if (!/AUDIO|audio[-_]?data|bands\[/.test(content)) continue;
    if (!(/forEach/.test(content) && /caption|group|cg-/.test(content))) continue;

    const hasInnerSamplingLoop =
      /for\s*\(\s*var\s+\w+\s*=\s*group\.start/.test(content) ||
      /for\s*\(\s*var\s+at\s*=/.test(content) ||
      /while\s*\(\s*\w+\s*<\s*group\.end/.test(content);

    if (!hasInnerSamplingLoop) {
      const hasPeakTween =
        /peak(?:Bass|Treble|Energy)/.test(content) && /group\.start/.test(content);
      if (hasPeakTween) {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/audio-reactive-single-tween-per-group',
          message:
            'Audio-reactive captions use a single tween per group based on peak values. ' +
            'This sets one static value at group.start — not perceptible as audio reactivity.',
          fix_hint:
            'Sample audio data at 100-200ms intervals throughout each group\'s lifetime ' +
            '(for loop from group.start to group.end) and create a tween at each sample ' +
            'point for visible pulsing.',
        });
      }
    }
  }
}

// ── Rule: gsap_infinite_repeat ─────────────────────────────────────────────
//
// Ported verbatim from upstream.

function ruleGsapInfiniteRepeat(ctx, report) {
  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';
    const pattern = /repeat\s*:\s*-1(?!\d)/g;
    let m;
    while ((m = pattern.exec(content)) !== null) {
      const start = Math.max(0, m.index - 60);
      const end = Math.min(content.length, m.index + m[0].length + 60);
      const snippet = content.slice(start, end).trim();
      push(report, {
        severity: 'error',
        rule_id: 'comp/gsap-infinite-repeat',
        message:
          'GSAP tween uses `repeat: -1` (infinite). Infinite repeats break the deterministic ' +
          'capture engine which seeks to exact frame times. Use a finite repeat count calculated ' +
          'from the composition duration: `repeat: Math.floor(duration / cycleDuration) - 1`.',
        fix_hint:
          'Replace `repeat: -1` with a finite count, e.g. `repeat: Math.floor(totalDuration / singleCycleDuration) - 1`. ' +
          'Use Math.floor (not Math.ceil) to ensure the animation fits within the total duration.',
        snippet: truncateSnippet(snippet),
      });
    }
  }
}

// ── Rule: gsap_repeat_ceil_overshoot ──────────────────────────────────────
//
// Ported verbatim from upstream.

function ruleGsapRepeatCeilOvershoot(ctx, report) {
  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';
    const pattern = /repeat\s*:\s*Math\.ceil\s*\([^)]+\)\s*-\s*1/g;
    let m;
    while ((m = pattern.exec(content)) !== null) {
      const start = Math.max(0, m.index - 40);
      const end = Math.min(content.length, m.index + m[0].length + 40);
      const snippet = content.slice(start, end).trim();
      push(report, {
        severity: 'warning',
        rule_id: 'comp/gsap-repeat-ceil-overshoot',
        message:
          'GSAP repeat calculation uses `Math.ceil` which can overshoot the composition duration. ' +
          'For example, Math.ceil(10.5 / 2) - 1 = 5 repeats → 6 cycles × 2s = 12s, exceeding 10.5s.',
        fix_hint:
          'Use `Math.floor` instead of `Math.ceil` to ensure the animation fits within the duration: ' +
          '`repeat: Math.floor(totalDuration / cycleDuration) - 1`. ' +
          'Math.floor(10.5 / 2) - 1 = 4 repeats → 5 cycles × 2s = 10s ✓',
        snippet: truncateSnippet(snippet),
      });
    }
  }
}

// ── Rule: scene_layer_missing_visibility_kill ──────────────────────────────
//
// NARROWED vs upstream: upstream iterated OpenTag objects with `readAttr`.
// We use DOM querySelectorAll and getAttribute instead.

function ruleSceneLayerMissingVisibilityKill(ctx, report) {
  // Detect multi-scene compositions: multiple elements whose id matches /^scene\d+$/i
  const sceneElements = Array.from(ctx.doc.querySelectorAll('[id]')).filter(el =>
    /^scene\d+$/i.test(el.getAttribute('id') || ''),
  );
  if (sceneElements.length < 2) return;

  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';

    for (const el of sceneElements) {
      const id = el.getAttribute('id') || '';
      // Check if this scene has exit tweens (opacity: 0 on #id)
      const exitPattern = new RegExp(`["']#${id}["'][^)]*opacity\\s*:\\s*0`);
      if (!exitPattern.test(content)) continue;

      // Check if there is a hard visibility kill on this scene
      const killPattern = new RegExp(`["']#${id}["'][^)]*visibility\\s*:\\s*["']hidden["']`);
      if (!killPattern.test(content)) {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/scene-layer-missing-visibility-kill',
          element_id: id,
          message:
            `Scene layer "#${id}" exits via opacity tween but has no visibility: hidden hard kill. ` +
            'When scrubbing or when tweens conflict, the scene may remain partially visible and overlap the next scene.',
          fix_hint:
            `Add \`tl.set("#${id}", { visibility: "hidden" }, <exit-end-time>)\` after the scene's exit tweens.`,
        });
      }
    }
  }
}

// ── Export ─────────────────────────────────────────────────────────────────

export default [
  ruleGsapTweenChecks,
  ruleGsapCssTransformConflict,
  ruleMissingGsapScript,
  ruleAudioReactiveSingleTweenPerGroup,
  ruleGsapInfiniteRepeat,
  ruleGsapRepeatCeilOvershoot,
  ruleSceneLayerMissingVisibilityKill,
];
