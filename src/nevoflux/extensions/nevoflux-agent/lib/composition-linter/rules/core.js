// composition-linter/rules/core.js — ported from the upstream core rule set.
// Rule IDs use the `comp/*` prefix.
// See ../LICENSE-NOTICE.md for attribution.

import { push } from '../utils.js';

// ── helpers ────────────────────────────────────────────────────────────────

/** Truncate a string for use as a snippet in issue messages. */
function truncateSnippet(str, max = 120) {
  if (!str) return undefined;
  const s = str.replace(/\s+/g, ' ').trim();
  if (!s) return undefined;
  return s.length > max ? s.slice(0, max) + '…' : s;
}

/** Read an attribute value from a raw HTML tag string; returns null if absent. */
function readAttrRaw(tagSource, attr) {
  if (!tagSource) return null;
  const escaped = attr.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = tagSource.match(new RegExp(`\\b${escaped}\\s*=\\s*["']([^"']+)["']`, 'i'));
  return match?.[1] || null;
}

/** Extract composition IDs referenced in CSS attribute selectors. */
function extractCompositionIdsFromCss(css) {
  const ids = new Set();
  const pattern = /\[data-composition-id=["']([^"']+)["']\]/g;
  let m;
  while ((m = pattern.exec(css)) !== null) {
    if (m[1]) ids.add(m[1]);
  }
  return [...ids];
}

/** Try to parse a JS string; return syntax error message or null. */
function getInlineScriptSyntaxError(source) {
  if (!source || !source.trim()) return null;
  try {
    // eslint-disable-next-line no-new-func
    new Function(source);
    return null;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    // chrome:// contexts block Function() / eval via CSP; the rejection
    // is not a syntax error in the script being linted. Treat as "cannot
    // determine" and return null so the rule does not false-positive.
    if (/\beval\b|\bFunction\b|not allowed in.*Parent Process|System Context/i.test(msg)) {
      return null;
    }
    return msg;
  }
}

// ── Regex constants (ported verbatim from upstream utils.ts) ───────────────

const TIMELINE_REGISTRY_INIT_PATTERN =
  /window\.__timelines\s*=\s*window\.__timelines\s*\|\|\s*(?:\{\}|\[\])|window\.__timelines\s*=\s*(?:\{\}|\[\])|window\.__timelines\s*\?\?=\s*(?:\{\}|\[\])/i;
const TIMELINE_REGISTRY_ASSIGN_PATTERN =
  /window\.__timelines\[[^\]]+\]\s*=|window\.__timelines\.push\s*\(/i;
const INVALID_SCRIPT_CLOSE_PATTERN = /<script[^>]*>[\s\S]*?<\s*\/\s*script(?!>)/i;

// ── core rule: root_missing_composition_id + root_missing_dimensions ────────
//
// Narrowed vs upstream: only fires when the document contains at least one
// data-composition-id attribute, indicating it is an authored composition.
// Plain scaffold HTML (e.g. the baseline) does not declare data-composition-id
// and so is intentionally exempt — it would be noise on unrelated pages.

function ruleRootMissingCompositionAttributes(ctx, report) {
  // Only apply to documents that contain at least one data-composition-id,
  // so we don't flag plain HTML pages that are not compositions at all.
  const hasAnyCompId = ctx.raw.includes('data-composition-id');
  if (!hasAnyCompId) return;

  // Find the root composition element: first element in <body> that carries
  // data-composition-id (or the first non-trivial body child if none has it).
  const rootEl = ctx.doc.body
    ? ctx.doc.body.firstElementChild
    : ctx.doc.querySelector('[data-composition-id]');

  if (!rootEl || !rootEl.getAttribute('data-composition-id')) {
    push(report, {
      severity: 'error',
      rule_id: 'comp/root-missing-composition-id',
      message: 'Root composition is missing `data-composition-id`.',
      fix_hint: 'Add a stable `data-composition-id` to the entry composition wrapper.',
      snippet: rootEl ? truncateSnippet(rootEl.outerHTML) : undefined,
    });
  }

  if (
    !rootEl ||
    !rootEl.getAttribute('data-width') ||
    !rootEl.getAttribute('data-height')
  ) {
    push(report, {
      severity: 'error',
      rule_id: 'comp/root-missing-dimensions',
      message: 'Root composition is missing `data-width` or `data-height`.',
      fix_hint: 'Set numeric `data-width` and `data-height` on the entry composition root.',
      snippet: rootEl ? truncateSnippet(rootEl.outerHTML) : undefined,
    });
  }
}

// ── core rule: missing_timeline_registry + timeline_registry_missing_init ───
//
// Narrowed vs upstream: only fires when the document contains at least one
// data-composition-id, indicating it is an authored composition that needs a
// timeline registry. The baseline scaffold HTML has no data-composition-id
// and no timelines, so these would be false positives there.

function ruleTimelineRegistry(ctx, report) {
  // Guard: only flag compositions that declare at least one composition ID.
  // Plain HTML without any data-composition-id does not need window.__timelines.
  const hasAnyCompId = ctx.raw.includes('data-composition-id');
  if (!hasAnyCompId) return;

  if (
    !TIMELINE_REGISTRY_INIT_PATTERN.test(ctx.raw) &&
    !TIMELINE_REGISTRY_ASSIGN_PATTERN.test(ctx.raw)
  ) {
    push(report, {
      severity: 'error',
      rule_id: 'comp/missing-timeline-registry',
      message: 'Missing `window.__timelines` registration.',
      fix_hint: 'Register each composition timeline on `window.__timelines[compositionId]`.',
    });
  }
  if (
    TIMELINE_REGISTRY_ASSIGN_PATTERN.test(ctx.raw) &&
    !TIMELINE_REGISTRY_INIT_PATTERN.test(ctx.raw)
  ) {
    push(report, {
      severity: 'error',
      rule_id: 'comp/timeline-registry-missing-init',
      message:
        '`window.__timelines[…] = …` is used without initializing `window.__timelines` first.',
      fix_hint:
        'Add `window.__timelines = window.__timelines || {};` before any timeline assignment.',
    });
  }
}

// ── core rule: timeline_id_mismatch ────────────────────────────────────────

function ruleTimelineIdMismatch(ctx, report) {
  const htmlCompIds = new Set();
  const timelineRegKeys = new Set();
  const compIdRe = /data-composition-id\s*=\s*["']([^"']+)["']/gi;
  const tlKeyRe = /window\.__timelines\[\s*["']([^"']+)["']\s*\]/g;
  let m;
  while ((m = compIdRe.exec(ctx.raw)) !== null) {
    if (m[1]) htmlCompIds.add(m[1]);
  }
  while ((m = tlKeyRe.exec(ctx.raw)) !== null) {
    if (m[1]) timelineRegKeys.add(m[1]);
  }
  for (const key of timelineRegKeys) {
    if (!htmlCompIds.has(key)) {
      push(report, {
        severity: 'error',
        rule_id: 'comp/timeline-id-mismatch',
        message: `Timeline registered as "${key}" but no element has data-composition-id="${key}". The runtime cannot auto-nest this timeline.`,
        fix_hint: `Change window.__timelines["${key}"] to match the data-composition-id attribute, or vice versa.`,
      });
    }
  }
}

// ── core rule: invalid_inline_script_syntax (malformed close tag) ──────────

function ruleInvalidScriptCloseSyntax(ctx, report) {
  if (!INVALID_SCRIPT_CLOSE_PATTERN.test(ctx.raw)) return;
  push(report, {
    severity: 'error',
    rule_id: 'comp/invalid-inline-script-syntax',
    message: 'Detected malformed inline `<script>` closing syntax.',
    fix_hint: 'Close inline scripts with a valid `</script>` tag.',
  });
}

// ── core rule: invalid_inline_script_syntax (JS parse error) ───────────────
//
// Our context provides DOM script elements; we use .textContent for content
// and check for src / type="application/json" via getAttribute().

function ruleInvalidInlineScriptSyntax(ctx, report) {
  for (const script of ctx.scripts) {
    // Skip external scripts and JSON data blocks
    if (script.getAttribute('src')) continue;
    const type = script.getAttribute('type') || '';
    if (/application\/json/i.test(type)) continue;
    // Skip ES modules: `import` declarations are module-level syntax and are
    // not valid inside new Function(), so parsing them would always throw a
    // SyntaxError even for perfectly valid module scripts.
    if (/module/i.test(type)) continue;

    const content = script.textContent || '';
    const syntaxError = getInlineScriptSyntaxError(content);
    if (!syntaxError) continue;
    push(report, {
      severity: 'error',
      rule_id: 'comp/invalid-inline-script-syntax',
      message: `Inline script has invalid syntax: ${syntaxError}`,
      fix_hint: 'Fix the inline script syntax before render verification.',
      snippet: truncateSnippet(content),
    });
  }
}

// ── core rule: host_missing_composition_id ─────────────────────────────────
//
// Upstream iterates raw OpenTag objects; we use DOM querySelectorAll instead.

function ruleHostMissingCompositionId(ctx, report) {
  for (const el of ctx.doc.querySelectorAll('[data-composition-src]')) {
    const src = el.getAttribute('data-composition-src') || '';
    if (el.getAttribute('data-composition-id')) continue;
    push(report, {
      severity: 'error',
      rule_id: 'comp/host-missing-composition-id',
      message: `Composition host for "${src}" is missing \`data-composition-id\`.`,
      fix_hint:
        'Set `data-composition-id` on every `data-composition-src` host element.',
      snippet: truncateSnippet(el.outerHTML),
    });
  }
}

// ── core rule: scoped_css_missing_wrapper ──────────────────────────────────
//
// Upstream uses ExtractedBlock[]  (regex-based). We use DOM style elements.
// compositionIds is built from the DOM doc inline here.

function ruleScopedCssMissingWrapper(ctx, report) {
  // Build the set of composition IDs that exist in this document.
  const compositionIds = new Set();
  for (const el of ctx.doc.querySelectorAll('[data-composition-id]')) {
    const id = el.getAttribute('data-composition-id');
    if (id) compositionIds.add(id);
  }

  const scopedCssCompositionIds = new Set();
  for (const styleEl of ctx.styles) {
    const css = styleEl.textContent || '';
    for (const compId of extractCompositionIdsFromCss(css)) {
      scopedCssCompositionIds.add(compId);
    }
  }

  for (const compId of scopedCssCompositionIds) {
    if (compositionIds.has(compId)) continue;
    push(report, {
      severity: 'warning',
      rule_id: 'comp/scoped-css-missing-wrapper',
      message: `Scoped CSS targets composition "${compId}" but no matching wrapper exists in this HTML.`,
      fix_hint:
        'Preserve the matching composition wrapper or align the CSS scope to an existing wrapper.',
    });
  }
}

// ── core rule: non_deterministic_code ──────────────────────────────────────

function ruleNonDeterministicCode(ctx, report) {
  const patterns = [
    {
      pattern: /Math\.random\s*\(/,
      label: 'Math.random()',
      hint: 'Use a seeded PRNG (e.g. a simple mulberry32) so renders are deterministic across frames.',
    },
    {
      pattern: /Date\.now\s*\(/,
      label: 'Date.now()',
      hint: 'Remove time-dependent code. Use GSAP timeline position instead of wall-clock time.',
    },
    {
      pattern: /new\s+Date\s*\(/,
      label: 'new Date()',
      hint: 'Remove time-dependent code. Use GSAP timeline position instead of wall-clock time.',
    },
    {
      pattern: /performance\.now\s*\(/,
      label: 'performance.now()',
      hint: 'Remove time-dependent code. Use GSAP timeline position instead of wall-clock time.',
    },
    {
      pattern: /crypto\.getRandomValues\s*\(/,
      label: 'crypto.getRandomValues()',
      hint: 'Remove time-dependent code. Use a seeded PRNG for deterministic renders.',
    },
  ];

  for (const script of ctx.scripts) {
    const content = script.textContent || '';
    // Strip comments to avoid false positives
    const stripped = content.replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
    for (const { pattern, label, hint } of patterns) {
      if (pattern.test(stripped)) {
        push(report, {
          severity: 'error',
          rule_id: 'comp/non-deterministic-code',
          message: `Script contains \`${label}\` which produces non-deterministic output. Renders may differ between frames or runs.`,
          fix_hint: hint,
          snippet: truncateSnippet(content),
        });
      }
    }
  }
}

// ── export ─────────────────────────────────────────────────────────────────

export default [
  ruleRootMissingCompositionAttributes,
  ruleTimelineRegistry,
  ruleTimelineIdMismatch,
  ruleInvalidScriptCloseSyntax,
  ruleInvalidInlineScriptSyntax,
  ruleHostMissingCompositionId,
  ruleScopedCssMissingWrapper,
  ruleNonDeterministicCode,
];
