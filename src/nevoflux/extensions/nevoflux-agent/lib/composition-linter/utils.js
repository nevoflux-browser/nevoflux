// composition-linter/utils.js

/** Convert a byte offset in `raw` to a {line, col} (1-based) pair. */
export function offsetToLineCol(raw, offset) {
  if (offset == null || offset < 0 || offset > raw.length) return { line: null, col: null };
  let line = 1, col = 1;
  for (let i = 0; i < offset; i++) {
    if (raw.charCodeAt(i) === 10 /* \n */) { line++; col = 1; }
    else { col++; }
  }
  return { line, col };
}

/** Find the line/col of a node by scanning `raw` for its outerHTML.
 *  Best-effort — duplicate identical nodes land on the first match. */
export function findNodeLineCol(raw, node) {
  try {
    const needle = node.outerHTML;
    if (!needle) return { line: null, col: null };
    const idx = raw.indexOf(needle);
    if (idx === -1) return { line: null, col: null };
    return offsetToLineCol(raw, idx);
  } catch { return { line: null, col: null }; }
}

/** Whitelist test for CDN URLs per spec §5.3 nf/cdn-whitelist. */
const CDN_WHITELIST = [
  /^https:\/\/esm\.sh\/gsap(@\d|\/|$)/,
  /^https:\/\/esm\.sh\/three(@\d|\/|$)/,
  /^https:\/\/esm\.sh\/lottie-web(@\d|\/|$)/,
];
export function isCdnWhitelisted(url) {
  if (typeof url !== 'string') return false;
  return CDN_WHITELIST.some(rx => rx.test(url));
}

/** Push an issue into the right bucket of a LintReport. */
export function push(report, issue) {
  if (issue.severity === 'error') report.errors.push(issue);
  else if (issue.severity === 'warning') report.warnings.push(issue);
  else report.infos.push(issue);
}

/** Safe-ish text match — returns true if `text` contains `needle` outside comments. */
export function textContains(text, needle) {
  const stripped = text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1');
  return stripped.includes(needle);
}
