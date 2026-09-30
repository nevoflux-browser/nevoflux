/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * How click_by_id walks its candidate selectors (the element, then its
 * children). A click the actor sent is final: trying the next candidate
 * clicks a second element. Mutations are never retried (design §4.5).
 */

const NOT_FOUND = 1001;
const COVERED = 1003;
// Refusals the actor makes before any press: nothing reached the page.
const REFUSED_BEFORE_PRESS = new Set([NOT_FOUND, 1002]);

/**
 * What one candidate's result says about the page: 'sent', 'covered',
 * 'not_sent' (refused before any press), or 'maybe_sent' — anything else,
 * e.g. 5001 when the actor threw after the press or a navigation tore it
 * down mid-query. A maybe-sent click must be treated as sent.
 */
export function candidateOutcome(result) {
  if (result && result.success !== false) {
    return 'sent';
  }
  const code = result?.error?.code;
  if (code === COVERED) {
    return 'covered';
  }
  if (REFUSED_BEFORE_PRESS.has(code)) {
    return 'not_sent';
  }
  return 'maybe_sent';
}

/** After one candidate: 'done' to stop, 'next' to try the next one. */
export function afterCandidateClick(result) {
  return candidateOutcome(result) === 'not_sent' ? 'next' : 'done';
}

/**
 * The stored-rect coordinate click is for elements no selector reaches
 * (cross-origin iframes): only when every candidate was not found. Never
 * after a hidden/offscreen refusal (the rect now shows something else), a
 * cover, or a click that may have been sent.
 */
export function mayUseCoordinateFallback(results) {
  return results.length > 0 && results.every((r) => r?.error?.code === NOT_FOUND);
}

/** Actions that change the page; sending one twice does it twice. */
export const MUTATING_ACTIONS = new Set([
  'click',
  'click_by_id',
  'type',
  'type_by_id',
  'fill',
  'fill_by_id',
]);

/**
 * executeInContentScript injects content.js and sends again when the first
 * send failed. For a mutation that is safe only if nobody received the first
 * message; after a timeout the action may have run.
 */
export function mayResendAfterInjection(action, error) {
  if (!MUTATING_ACTIONS.has(action)) {
    return true;
  }
  const message = String(error?.message ?? error ?? '');
  return /Receiving end does not exist|Could not establish connection/i.test(message);
}
