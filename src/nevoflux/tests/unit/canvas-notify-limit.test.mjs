/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { allowCanvasNotify } from '../../extensions/nevoflux-agent/background/canvas-notify-limit.mjs';

describe('canvas-notify-limit', () => {
  it('allows 3 per 30s per artifact then refuses', () => {
    const h = new Map();
    expect(allowCanvasNotify(h, 'a', 0)).toBe(true);
    expect(allowCanvasNotify(h, 'a', 1000)).toBe(true);
    expect(allowCanvasNotify(h, 'a', 2000)).toBe(true);
    expect(allowCanvasNotify(h, 'a', 3000)).toBe(false);
  });
  it('buckets are per artifact', () => {
    const h = new Map();
    for (let i = 0; i < 3; i++) allowCanvasNotify(h, 'a', i);
    expect(allowCanvasNotify(h, 'b', 5)).toBe(true);
  });
  it('the window rolls', () => {
    const h = new Map();
    for (let i = 0; i < 3; i++) allowCanvasNotify(h, 'a', i * 1000);
    expect(allowCanvasNotify(h, 'a', 29999)).toBe(false);
    expect(allowCanvasNotify(h, 'a', 30000)).toBe(true);
  });
  it('a missing artifact id shares one bucket', () => {
    const h = new Map();
    for (let i = 0; i < 3; i++) allowCanvasNotify(h, undefined, i);
    expect(allowCanvasNotify(h, '', 10)).toBe(false);
    expect(allowCanvasNotify(h, null, 11)).toBe(false);
  });
  it('refused attempts are not recorded', () => {
    const h = new Map();
    for (let i = 0; i < 3; i++) allowCanvasNotify(h, 'a', i);
    allowCanvasNotify(h, 'a', 20000);
    expect(allowCanvasNotify(h, 'a', 30002)).toBe(true);
  });
});
