// composition-linter/types.js
/**
 * @typedef {'error'|'warning'|'info'} LintSeverity
 *
 * @typedef {Object} LintIssue
 * @property {LintSeverity} severity
 * @property {string} rule_id       "comp/..." | "nf/..." | "linter/internal"
 * @property {string} message
 * @property {number} [line]
 * @property {number} [col]
 * @property {string} [fix_hint]
 *
 * @typedef {Object} LintReport
 * @property {LintIssue[]} errors
 * @property {LintIssue[]} warnings
 * @property {LintIssue[]} infos
 * @property {number} elapsed_ms
 *
 * @typedef {Object} LintContext
 * @property {Document} doc
 * @property {string} raw
 * @property {HTMLElement[]} scripts
 * @property {HTMLElement[]} styles
 * @property {HTMLElement[]} mediaElements
 * @property {string[]} classes
 * @property {string} [composition_id]
 */
export {};
