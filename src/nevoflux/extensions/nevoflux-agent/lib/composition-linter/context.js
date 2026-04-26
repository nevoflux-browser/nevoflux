// composition-linter/context.js

/**
 * Parse raw HTML into a LintContext. Uses DOMParser (text/html mode, which
 * does NOT execute scripts). Safe to call on untrusted composition HTML.
 */
export function buildContext(html, opts = {}) {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  const scripts = Array.from(doc.querySelectorAll('script'));
  const styles  = Array.from(doc.querySelectorAll('style'));
  const mediaElements = Array.from(doc.querySelectorAll('audio, video'));

  const classSet = new Set();
  for (const el of doc.querySelectorAll('[class]')) {
    for (const c of (el.getAttribute('class') || '').split(/\s+/)) {
      if (c) classSet.add(c);
    }
  }

  return {
    doc,
    raw: html,
    scripts,
    styles,
    mediaElements,
    classes: [...classSet],
    composition_id: opts.composition_id,
    strict: !!opts.strict,
  };
}
