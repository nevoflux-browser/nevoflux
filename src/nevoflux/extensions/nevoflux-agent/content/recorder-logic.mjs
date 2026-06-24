/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// DOM-free recorder core. No DOM, no chrome, no browser.* — unit-testable.

const SECRET_RE = /pass|token|secret|otp|cvv/i;

export function mapEventToAction(type) {
  switch (type) {
    case 'click': return 'click';
    case 'change': return 'fill';
    case 'input': return 'fill';
    case 'scroll': return 'scroll';
    default: return null;
  }
}

export function isSecretField({ tag, inputType, name, autocomplete } = {}) {
  if ((inputType || '').toLowerCase() === 'password') return true;
  if (SECRET_RE.test(name || '')) return true;
  return SECRET_RE.test(autocomplete || '');
}

function snake(s) {
  return String(s || 'value').trim().toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '') || 'value';
}

export function buildStep({ action, target, value, inputType, name, autocomplete, url, title, tsMs, waitAfter = null }) {
  const step = { type: 'step', action, target: { ...target }, url, title, ts_ms: tsMs, wait_after: waitAfter };
  const isFile = (inputType || '').toLowerCase() === 'file';
  if (isFile) {
    step.target.element_kind = 'file';
    step.value = '{{file}}';
    step.input_ref = '{{file}}';
    return step;
  }
  if (action === 'fill' && isSecretField({ tag: target?.tag, inputType, name, autocomplete })) {
    step.value = null;
    step.redacted = true;
    return step; // no input_ref for secrets
  }
  step.value = value ?? null;
  if (action === 'fill' && value != null) {
    step.input_ref = `{{${snake(name || target?.name)}}}`;
  }
  return step;
}

export function coalesceFill(buffer) {
  if (!Array.isArray(buffer) || buffer.length === 0) return '';
  return buffer[buffer.length - 1].value ?? '';
}

export function convergeScroll(events, windowMs) {
  if (!Array.isArray(events) || events.length === 0) return [];
  const out = [events[0]];
  let anchor = events[0].tsMs;
  for (const e of events.slice(1)) {
    if (e.tsMs - anchor > windowMs) { out.push(e); anchor = e.tsMs; }
  }
  return out;
}

export function makeHeader({ recordingId, createdAt, startUrl, goalHint }) {
  return { type: 'header', recording_id: recordingId, created_at: createdAt,
    start_url: startUrl, goal_hint: goalHint };
}
