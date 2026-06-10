/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Pure (DOM-free) logic for the Packs settings section.
 *
 * This module is the *tested source of truth* for the Packs UI helpers.
 * It is imported directly by the node unit-test runner
 * (tests/unit/pack-ui-logic.test.mjs).
 *
 * NOTE: settings.js is loaded as a classic <script> in the chrome context
 * (see settings.html) and therefore cannot `import` an ES module at
 * runtime. The functions below are inline-duplicated inside settings.js
 * under a `_PackLogic` namespace; if you change the behaviour here, mirror
 * it there (and vice-versa). Keep this .mjs authoritative for tests.
 *
 * Daemon RPC contract (mirrored from the nevoflux-agent daemon):
 *   pack.list      -> { packs: [{ name, version, installed_at }] }
 *   pack.status    -> { ... }
 *   pack.validate  -> { ok: bool, violations: [string | {message}] }
 *   pack.install   -> wait:true  => { success, version, files }
 *                     wait:false => { started, op_id }
 *   pack.uninstall -> { success }
 *   pack.update    -> { success, version, files }
 *
 * Errors surface as { code, message } (e.g. KNOWLEDGE_UNSUPPORTED,
 * INSTALL_FAILED).
 */

'use strict';

/**
 * Normalise a `pack.list` report into an array of row descriptors.
 *
 * Accepts the daemon report shape `{ packs: [...] }`, a bare array, or a
 * nullish/garbage value (returns []). Each entry is normalised to
 * `{ name, version, installedAt }` with safe string defaults so the
 * renderer never has to null-check.
 *
 * @param {{packs?: Array}|Array|null|undefined} report
 * @returns {Array<{name: string, version: string, installedAt: string}>}
 */
export function packListToRows(report) {
  let packs;
  if (Array.isArray(report)) {
    packs = report;
  } else if (report && Array.isArray(report.packs)) {
    packs = report.packs;
  } else {
    packs = [];
  }

  return packs
    .filter((p) => p && typeof p === 'object')
    .map((p) => ({
      name: typeof p.name === 'string' ? p.name : '',
      version: p.version != null ? String(p.version) : '',
      // Daemon uses snake_case (installed_at); tolerate camelCase too.
      installedAt:
        p.installed_at != null
          ? String(p.installed_at)
          : p.installedAt != null
            ? String(p.installedAt)
            : '',
    }));
}

/**
 * Turn a `pack.validate` result into a human-readable status string.
 *
 * `{ ok: true }` -> a friendly "passed" line. Otherwise enumerates the
 * `violations[]` (each may be a bare string or an object with a `message`
 * / `rule` field). A falsy result is treated as a failed validation with
 * no detail.
 *
 * @param {{ok?: boolean, violations?: Array}|null|undefined} result
 * @returns {string}
 */
export function validateResultMessage(result) {
  if (result && result.ok) {
    return 'Validation passed — no policy violations.';
  }

  const raw = (result && result.violations) || [];
  const violations = Array.isArray(raw) ? raw : [];
  const messages = violations
    .map((v) => {
      if (v == null) return '';
      if (typeof v === 'string') return v;
      if (typeof v === 'object') {
        return String(v.message || v.rule || v.detail || JSON.stringify(v));
      }
      return String(v);
    })
    .filter((m) => m.length > 0);

  if (messages.length === 0) {
    return 'Validation failed.';
  }
  const count = messages.length;
  const noun = count === 1 ? 'violation' : 'violations';
  return `Validation failed (${count} ${noun}):\n` +
    messages.map((m) => `• ${m}`).join('\n');
}

/**
 * Build the params object for `pack.install`.
 *
 * `wait:true` keeps the first cut synchronous (the daemon returns
 * `{ success, version, files }`). Pass `wait:false` to opt into the
 * streaming/`op_id` flow.
 *
 * @param {string} manifestPath  path the daemon reads the manifest from
 * @param {{force?: boolean, wait?: boolean}} [opts]
 * @returns {{manifest_path: string, wait: boolean, force?: boolean}}
 */
export function installParams(manifestPath, opts = {}) {
  const params = {
    manifest_path: typeof manifestPath === 'string' ? manifestPath.trim() : '',
    // Default to synchronous install for the first-cut UX.
    wait: opts.wait === false ? false : true,
  };
  if (opts.force) {
    params.force = true;
  }
  return params;
}

/**
 * Build the params object for `pack.uninstall`.
 *
 * `purgeData` (default OFF) maps to the daemon's `purge_data` flag which
 * also deletes the pack's on-disk data.
 *
 * @param {string} name
 * @param {{purgeData?: boolean, force?: boolean}} [opts]
 * @returns {{name: string, purge_data: boolean, force?: boolean}}
 */
export function uninstallParams(name, opts = {}) {
  const params = {
    name: typeof name === 'string' ? name : '',
    purge_data: opts.purgeData === true,
  };
  if (opts.force) {
    params.force = true;
  }
  return params;
}

/**
 * Build the params object for `pack.update`.
 *
 * @param {string} manifestPath
 * @returns {{manifest_path: string}}
 */
export function updateParams(manifestPath) {
  return {
    manifest_path: typeof manifestPath === 'string' ? manifestPath.trim() : '',
  };
}

/**
 * Map a daemon error into a friendly user-facing message.
 *
 * Accepts an `Error`, a `{ code, message }` object, or a bare string.
 * Special-cases known daemon error codes:
 *   - KNOWLEDGE_UNSUPPORTED: packs aren't available in this build/config.
 *   - INSTALL_FAILED / UPDATE_FAILED / UNINSTALL_FAILED / VALIDATION_FAILED:
 *     keep the daemon-supplied detail but prefix a friendly label.
 *
 * @param {Error|{code?: string, message?: string}|string|null|undefined} error
 * @returns {string}
 */
export function packErrorMessage(error) {
  if (error == null) {
    return 'Unknown error.';
  }
  if (typeof error === 'string') {
    return error;
  }

  const code = error.code || (error.error && error.error.code);
  const message =
    error.message || (error.error && error.error.message) || '';

  switch (code) {
    case 'KNOWLEDGE_UNSUPPORTED':
      return (
        'Packs are not supported in this build or configuration. ' +
        (message ? `(${message})` : 'Knowledge-base support is unavailable.')
      );
    case 'INSTALL_FAILED':
      return `Install failed${message ? `: ${message}` : '.'}`;
    case 'UPDATE_FAILED':
      return `Update failed${message ? `: ${message}` : '.'}`;
    case 'UNINSTALL_FAILED':
      return `Uninstall failed${message ? `: ${message}` : '.'}`;
    case 'VALIDATION_FAILED':
      return `Validation failed${message ? `: ${message}` : '.'}`;
    default:
      break;
  }

  if (message) return message;
  if (code) return String(code);
  return 'Unknown error.';
}
