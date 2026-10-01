/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Element ids travel through the daemon's selector-based tools (browser_input,
 * browser_probe, browser_upload_file, browser_wait_for) as `ref:eN`.
 * Handlers send those to browser.nevoflux.actOnRef instead of a CSS lookup.
 */
export function refRoute(selector) {
  if (typeof selector !== 'string') {
    return null;
  }
  const m = selector.match(/^ref:\s*e?(\d+)\s*$/i);
  return m ? `e${m[1]}` : null;
}

/** The value a fill request carries: `value`, or `text` (browser_input's NativeFill). */
export function fillValue(params) {
  return params?.value ?? params?.text;
}
