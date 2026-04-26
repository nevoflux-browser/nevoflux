// composition-linter/index.js
import { buildContext } from './context.js';
import { push } from './utils.js';

export const LINTER_VERSION = '0.2.0';

import compositionRules from './rules/composition.js';
import coreRules        from './rules/core.js';
import gsapRules        from './rules/gsap.js';
import mediaRules       from './rules/media.js';
import captionsRules    from './rules/captions.js';
import adaptersRules    from './rules/adapters.js';
import nevofluxRules    from './rules/nevoflux.js';

const ALL_RULES = [
  ...compositionRules,
  ...coreRules,
  ...gsapRules,
  ...mediaRules,
  ...captionsRules,
  ...adaptersRules,
  ...nevofluxRules,
];

/**
 * Lint a composition HTML string.
 * @param {string} html
 * @param {{ composition_id?: string, strict?: boolean }} [opts]
 * @returns {import('./types.js').LintReport}
 */
export function lint(html, opts = {}) {
  const t0 = (typeof performance !== 'undefined' && performance.now)
    ? performance.now() : Date.now();
  const report = { errors: [], warnings: [], infos: [], elapsed_ms: 0 };
  let ctx;
  try {
    ctx = buildContext(html, opts);
  } catch (err) {
    push(report, {
      severity: 'error', rule_id: 'linter/internal',
      message: `HTML parse failed: ${err && err.message ? err.message : err}`,
    });
    report.elapsed_ms = Math.round(((typeof performance !== 'undefined' && performance.now)
      ? performance.now() : Date.now()) - t0);
    return report;
  }
  for (const rule of ALL_RULES) {
    try {
      rule(ctx, report);
    } catch (err) {
      push(report, {
        severity: 'error', rule_id: 'linter/internal',
        message: `rule threw: ${err && err.message ? err.message : err}`,
      });
    }
  }
  report.elapsed_ms = Math.round(((typeof performance !== 'undefined' && performance.now)
    ? performance.now() : Date.now()) - t0);
  return report;
}
