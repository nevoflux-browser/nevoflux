/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { refRoute, fillValue } from '../../extensions/nevoflux-agent/background/ref-routing.mjs';

describe('refRoute', () => {
  it('routes ref: selectors to actOnRef and leaves CSS alone', () => {
    expect(refRoute('ref:e7')).toBe('e7');
    expect(refRoute('ref:7')).toBe('e7');
    expect(refRoute('#e7')).toBeNull();
    expect(refRoute(undefined)).toBeNull();
  });
});

describe('fillValue', () => {
  // The daemon's browser_input strategy sends Fill {selector, text}; the
  // background fill handler required `value` and failed every plain input.
  it('accepts text as well as value', () => {
    expect(fillValue({ value: 'a' })).toBe('a');
    expect(fillValue({ text: 'b' })).toBe('b');
    expect(fillValue({})).toBeUndefined();
  });
});
