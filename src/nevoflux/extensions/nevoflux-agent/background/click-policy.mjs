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
