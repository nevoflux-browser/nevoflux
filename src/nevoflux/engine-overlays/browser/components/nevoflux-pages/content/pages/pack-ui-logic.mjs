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
 * Classify a pack source string as remote (GitHub) vs. a local path.
 *
 * Mirrors the daemon/CLI classification: a source is "remote" when the
 * trimmed string starts with `github:` (shorthand,
 * `github:user/repo[/sub][@ref]`) or `https://github.com/` (full URL).
 * Everything else (including empty / nullish / a filesystem path) is a
 * local manifest path.
 *
 * @param {string|null|undefined} source
 * @returns {boolean}
 */
export function isRemoteSource(source) {
  if (typeof source !== 'string') return false;
  const s = source.trim();
  return s.startsWith('github:') || s.startsWith('https://github.com/');
}

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
 * A REMOTE source (`github:…` / `https://github.com/…`) is sent as
 * `{ source }`; a LOCAL path is sent as `{ manifest_path }` so local
 * installs keep working against any daemon. `wait:true` keeps the first
 * cut synchronous (the daemon returns `{ success, version, files }`).
 * Pass `wait:false` to opt into the streaming/`op_id` flow.
 *
 * @param {string} source  remote `github:`/URL source or a local manifest path
 * @param {{force?: boolean, wait?: boolean}} [opts]
 * @returns {{source?: string, manifest_path?: string, wait: boolean, force?: boolean}}
 */
export function installParams(source, opts = {}) {
  const src = typeof source === 'string' ? source.trim() : '';
  const params = {
    ...(isRemoteSource(src) ? { source: src } : { manifest_path: src }),
    // Default to synchronous install for the first-cut UX.
    wait: opts.wait === false ? false : true,
  };
  if (opts.force) {
    params.force = true;
  }
  return params;
}

/**
 * Build the params object for `pack.inspect`.
 *
 * Inspect only ever runs against a remote source (the preview step before
 * a remote install), but the param shape is `{ source }` regardless.
 *
 * @param {string} source
 * @returns {{source: string}}
 */
export function inspectParams(source) {
  return { source: typeof source === 'string' ? source.trim() : '' };
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
 * Like `installParams`, a REMOTE source is sent as `{ source }` and a
 * LOCAL path as `{ manifest_path }`.
 *
 * @param {string} source  remote `github:`/URL source or a local manifest path
 * @returns {{source: string}|{manifest_path: string}}
 */
export function updateParams(source) {
  const src = typeof source === 'string' ? source.trim() : '';
  return isRemoteSource(src) ? { source: src } : { manifest_path: src };
}

/**
 * Turn a `pack.inspect` response into a plain-text preview string.
 *
 * Pure / DOM-free so it is unit-testable. Surfaces the pack identity, the
 * bundled components (skills, canvas tools, seed pages, dashboard,
 * knowledge), and any policy violations. Canvas tools are flagged with the
 * binary they run because installing the pack trusts that binary to run on
 * the user's machine. When `violations` is non-empty the preview makes the
 * count prominent so the caller can block the install.
 *
 * @param {object|null|undefined} data  the `pack.inspect` response
 * @returns {{text: string, violations: string[], hasViolations: boolean}}
 */
export function summarizeInspect(data) {
  const d = data && typeof data === 'object' ? data : {};
  const pack = d.pack && typeof d.pack === 'object' ? d.pack : {};
  const comps =
    d.components && typeof d.components === 'object' ? d.components : {};

  const lines = [];

  // Identity line: "name version — description".
  const name = typeof pack.name === 'string' && pack.name ? pack.name : '(unnamed pack)';
  const version =
    pack.version != null && String(pack.version) ? String(pack.version) : '';
  const description =
    typeof pack.description === 'string' && pack.description
      ? pack.description
      : '';
  let header = name;
  if (version) header += ` ${version}`;
  if (description) header += ` — ${description}`;
  lines.push(header);

  const asArray = (v) => (Array.isArray(v) ? v : []);

  const skills = asArray(comps.skills).filter(
    (s) => typeof s === 'string' && s
  );
  if (skills.length) {
    lines.push(`Skills: ${skills.join(', ')}`);
  }

  const canvasTools = asArray(comps.canvas_tools);
  if (canvasTools.length) {
    const rendered = canvasTools
      .map((t) => {
        if (t && typeof t === 'object') {
          const tn = typeof t.name === 'string' ? t.name : '';
          const bin = typeof t.binary === 'string' ? t.binary : '';
          if (tn && bin) return `${tn} (runs: ${bin})`;
          if (tn) return tn;
          if (bin) return `(runs: ${bin})`;
          return '';
        }
        return typeof t === 'string' ? t : '';
      })
      .filter((s) => s.length > 0);
    if (rendered.length) {
      lines.push(`Canvas tools: ${rendered.join(', ')}`);
      // Make the binary-execution trust boundary explicit.
      lines.push('  ⚠ Canvas tools run the binaries listed above on your machine.');
    }
  }

  const seed = asArray(comps.seed).filter((s) => typeof s === 'string' && s);
  if (seed.length) {
    lines.push(`Seed pages: ${seed.join(', ')}`);
  }

  if (typeof comps.dashboard === 'string' && comps.dashboard) {
    lines.push(`Dashboard: ${comps.dashboard}`);
  }

  if (comps.knowledge) {
    lines.push('Knowledge: yes');
  }

  // Normalise violations to plain strings (bare string or { message }).
  const rawViolations = Array.isArray(d.violations) ? d.violations : [];
  const violations = rawViolations
    .map((v) => {
      if (v == null) return '';
      if (typeof v === 'string') return v;
      if (typeof v === 'object') {
        return String(v.message || v.rule || v.detail || JSON.stringify(v));
      }
      return String(v);
    })
    .filter((m) => m.length > 0);

  if (violations.length === 0) {
    lines.push('Violations: <none>');
  } else {
    const count = violations.length;
    const noun = count === 1 ? 'violation' : 'violations';
    lines.push(`⚠ Violations (${count} ${noun}):`);
    for (const v of violations) {
      lines.push(`  • ${v}`);
    }
  }

  return {
    text: lines.join('\n'),
    violations,
    hasViolations: violations.length > 0,
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

/**
 * Normalize a `system:pack:progress` frame for the install UI.
 *
 * The daemon streams `{ op_id, phase, status, progress_pct, log }` frames on
 * the `system:pack:progress` EventBus topic during a `wait:false` install.
 * `phase` is one of Resolve/Compat/Capability/Idempotency/Place/Seed/Knowledge/
 * Artifact/Activate/Commit/Report; `status` is Running/Ok/Failed/RolledBack/
 * Cancelled. A frame is terminal when status !== 'Running' (success === 'Ok').
 *
 * @param {{op_id?: string, phase?: string, status?: string, progress_pct?: number, log?: string}} frame
 * @param {string} opId  the op_id returned by `pack.install` (wait:false)
 * @returns {{matched: boolean, pct: number, phase: string, status: string, terminal: boolean}}
 */
export function summarizePackProgress(frame, opId) {
  const f = frame || {};
  const status = typeof f.status === 'string' ? f.status : '';
  const pct = typeof f.progress_pct === 'number' ? f.progress_pct : 0;
  const phase = typeof f.phase === 'string' ? f.phase : '';
  const log = typeof f.log === 'string' ? f.log : '';
  const ok = status === 'Ok';
  const failed = status === 'Failed' || status === 'RolledBack' || status === 'Cancelled';
  return {
    matched: f.op_id === opId,
    pct,
    phase,
    status,
    line: `[${phase} ${pct}%]${log ? ` ${log}` : ''}`,
    // Terminal only on a recognized terminal status — a missing/unknown status
    // keeps the UI waiting rather than falsely "completing" the install.
    terminal: ok || failed,
    ok,
    failed,
  };
}
