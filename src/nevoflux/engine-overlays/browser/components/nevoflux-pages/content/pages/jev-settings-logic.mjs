/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free logic backing the Jev section on nevoflux://settings (design
 * v1.6 §5.9). Kept separate from settings.js so it can be unit-tested
 * without a browser; settings.js imports it dynamically from
 * chrome://nevoflux/content/pages/.
 *
 * Inputs are the daemon's `jev.get` / `jev.test` data shapes
 * (crates/daemon/src/jev/rpc.rs).
 */

/** Shown before Jev is turned on the first time (spec §5.8). */
export const NOTICE_TEXT =
  'Except for sites on the sensitive-site list, page content and tool results are sent to the Jev endpoint you configure.';

/** The decision points the user can switch, in the spec's order (§5.9 item 4). */
export const POINTS = [
  { key: 'tools', label: 'Choose the tools for each task' },
  { key: 'skills', label: 'Load a skill when the request clearly needs one' },
  { key: 'visibility', label: 'Grade large tool results' },
  { key: 'rebuild', label: 'Rebuild earlier turns when it pays off' },
  { key: 'permissions', label: 'Ask before risky actions', hint: 'Off by default' },
];

/** Turning Jev on needs the notice confirmed once. */
export function needsNotice({ enabling, acked }) {
  return Boolean(enabling) && acked !== true;
}

/**
 * One domain per line, normalised the way the daemon does
 * (privacy::normalise_domain): scheme, path, a leading "*." or "." and a
 * trailing "." removed, lowercased; blanks and repeats dropped.
 */
export function parseDomains(text) {
  const out = [];
  for (const line of String(text || '').split(/\r?\n/)) {
    let d = line.trim().toLowerCase();
    d = d.replace(/^[a-z][a-z0-9+.-]*:\/\//, '');
    d = d.split(/[/?#]/)[0];
    d = d.replace(/^\*\./, '').replace(/^\.+/, '').replace(/\.+$/, '');
    if (d && !out.includes(d)) {
      out.push(d);
    }
  }
  return out;
}

/** The list as the textarea shows it. */
export function domainsText(list) {
  return (list || []).join('\n');
}

/**
 * The provider note of spec §5.9 item 6, or null when Jev applies.
 * `scope` is `applies`, `acp` or `local`.
 */
export function scopeHint({ scope, permissions }) {
  if (scope === 'acp' && permissions) {
    return 'Only the permission check applies with this provider.';
  }
  if (scope === 'acp' || scope === 'local') {
    return "Jev's context optimisation does not apply with this provider.";
  }
  return null;
}

/** What the key field says when it is empty; never the key itself. */
export function keyPlaceholder({ has_api_key, api_key, key_from_env }) {
  if (has_api_key) {
    return `Current: ${api_key}. Leave blank to keep it.`;
  }
  if (key_from_env) {
    return 'Using the key from NEVOFLUX_API_KEY_TYPESAFE.';
  }
  return 'No key set.';
}

/**
 * The `jev.set` params for a change. An empty key field means "keep the
 * stored key", so it is left out; `clear_key` removes the key.
 */
export function buildSetParams(changes) {
  const params = {};
  for (const [k, v] of Object.entries(changes || {})) {
    if (k === 'api_key') {
      if (typeof v === 'string' && v.trim() !== '') {
        params.api_key = v.trim();
      }
    } else if (k === 'clear_key') {
      if (v) {
        params.api_key = null;
      }
    } else if (k === 'domains_text') {
      params.sensitive_domains = parseDomains(v);
    } else {
      params[k] = v;
    }
  }
  return params;
}

/** A connection test's result in one line. */
export function formatTest(result) {
  const all = (result.latency_ms || []).join(' / ');
  return `Median ${result.p50_ms} ms (${all}). Suggested timeout: ${result.suggested_timeout_ms} ms.`;
}

/** The daemon accepts whole milliseconds from 100 to 10000. */
export function validTimeout(n) {
  return Number.isInteger(n) && n >= 100 && n <= 10000;
}
