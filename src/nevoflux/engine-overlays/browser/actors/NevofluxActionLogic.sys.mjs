/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Decisions behind NevofluxChild's page actions, kept free of DOM and
 * ChromeUtils so node unit tests import this exact file.
 */

const SECRET_INPUT_TYPES = new Set(['password', 'file', 'hidden']);

/**
 * The value a snapshot line may show for a node, or null for none. A
 * snapshot goes to the model provider, so password, file and hidden inputs
 * never show theirs.
 */
export function snapshotValue({ tagName, type, value }) {
  if (typeof value !== 'string' || value === '') {
    return null;
  }
  if (
    String(tagName).toUpperCase() === 'INPUT' &&
    SECRET_INPUT_TYPES.has(String(type || '').toLowerCase())
  ) {
    return null;
  }
  return value.length > 30 ? value.slice(0, 27) + '...' : value;
}
