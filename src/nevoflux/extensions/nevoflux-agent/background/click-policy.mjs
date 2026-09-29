/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * How click_by_id walks its candidate selectors (the element, then its
 * children). A click the actor sent is final: trying the next candidate
 * clicks a second element. Mutations are never retried (design §4.5).
 */

const COVERED = 1003;

/** After one candidate: 'done' to stop, 'next' to try the next one. */
export function afterCandidateClick(result) {
  if (result && result.success !== false) {
    return 'done';
  }
  if (result?.error?.code === COVERED) {
    return 'done';
  }
  return 'next';
}

/**
 * The stored-rect coordinate click is for elements no selector reaches
 * (cross-origin iframes). Never after a sent click — that is a second click —
 * and never onto a cover.
 */
export function mayUseCoordinateFallback({ anyClickSent, covered }) {
  return !anyClickSent && !covered;
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
