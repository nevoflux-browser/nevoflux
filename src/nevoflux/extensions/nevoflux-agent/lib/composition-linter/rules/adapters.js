// composition-linter/rules/adapters.js — ported from the upstream adapters rule set.
// Rule IDs use the `comp/*` prefix.
// See ../LICENSE-NOTICE.md for attribution.
//
// Adaptation notes:
//
//   Upstream operated on a custom streaming-parser object model: scripts had
//   `.attrs` (raw attribute string) and `.content` (inner text), and tags were
//   plain objects with `.raw`.  We work with DOM HTMLScriptElement nodes from
//   DOMParser (available via `ctx.scripts`).
//
//   Inline script text  → el.textContent  (no src attribute present)
//   External script src → el.getAttribute('src')  (may be relative or absolute)
//   Element attributes  → standard DOM getAttribute
//
//   `ctx.doc.querySelectorAll(...)` covers the full document for attribute
//   searches (e.g. data-lottie-src).
//
//   Rule IDs: upstream snake_case codes are mapped to kebab-case `comp/*` IDs
//   following the pattern established in the other ported rule files.

import { push } from '../utils.js';

// ── Helpers ────────────────────────────────────────────────────────────────

/** Collect inline text from all <script> elements that have no src. */
function inlineScriptTexts(scripts) {
  return scripts
    .filter(el => !el.getAttribute('src'))
    .map(el => el.textContent || '');
}

/** Collect the src attribute value from all external <script> elements. */
function externalScriptSrcs(scripts) {
  return scripts
    .map(el => el.getAttribute('src') || '')
    .filter(Boolean);
}

// ── Rule: missing_lottie_script ────────────────────────────────────────────
//
// Fires when the composition references Lottie (via data-lottie-src attribute
// or direct lottie API calls in inline scripts) but no external Lottie script
// is loaded via a <script src="...lottie..."> tag.
//
// Upstream: checked `tags` for data-lottie-src and `scripts` inline content.
// Adaptation: uses ctx.doc.querySelectorAll for attribute search and
//   ctx.scripts for script inspection.

function ruleMissingLottieScript(ctx, report) {
  const inlineTexts = inlineScriptTexts(ctx.scripts);
  const srcList     = externalScriptSrcs(ctx.scripts);

  const hasLottieAttr = ctx.doc.querySelectorAll('[data-lottie-src]').length > 0;
  const usesLottieApi = inlineTexts.some(t =>
    /lottie\.(loadAnimation|setSpeed|play|stop|destroy)\b/.test(t),
  );
  const hasLottieScript = srcList.some(src => /lottie/i.test(src));

  if (!(hasLottieAttr || usesLottieApi) || hasLottieScript) return;

  push(report, {
    severity: 'error',
    rule_id: 'comp/missing-lottie-script',
    message:
      'Composition uses Lottie but no Lottie script is loaded. The animation will not render.',
    fix_hint:
      'Add <script src="https://cdn.jsdelivr.net/npm/lottie-web@5/build/player/lottie.min.js"></script> before your Lottie code.',
  });
}

// ── Rule: missing_three_script ─────────────────────────────────────────────
//
// Fires when an inline script references the THREE global namespace but no
// external Three.js script is loaded via a <script src="...three..."> tag.
//
// Upstream: checked inline script content for /\bTHREE\./.
// Adaptation: same regex applied to el.textContent for each inline script.

function ruleMissingThreeScript(ctx, report) {
  const inlineTexts = inlineScriptTexts(ctx.scripts);
  const srcList     = externalScriptSrcs(ctx.scripts);

  const usesThree     = inlineTexts.some(t => /\bTHREE\./.test(t));
  const hasThreeScript = srcList.some(src => /three/i.test(src));

  if (!usesThree || hasThreeScript) return;

  push(report, {
    severity: 'error',
    rule_id: 'comp/missing-three-script',
    message:
      'Composition uses Three.js but no Three.js script is loaded. The 3D scene will not render.',
    fix_hint:
      'Add <script src="https://cdn.jsdelivr.net/npm/three@0.160/build/three.min.js"></script> before your Three.js code.',
  });
}

// ── Export ─────────────────────────────────────────────────────────────────

export default [
  ruleMissingLottieScript,
  ruleMissingThreeScript,
];
