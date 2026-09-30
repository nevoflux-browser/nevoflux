/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { readFileSync } from 'node:fs';
import {
  MUTATING_ACTIONS,
  mayResendAfterInjection,
} from '../../extensions/nevoflux-agent/background/click-policy.mjs';

describe('background by-id actions', () => {
  const bg = readFileSync(
    new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
    'utf8'
  );
  it('forward the id to the actor instead of resolving a selector', () => {
    expect(bg.includes('getElementSelector(')).toBe(false);
    expect(bg.includes('tryCoordinateClickFallback(')).toBe(false);
    expect((bg.match(/browser\.nevoflux\.actOnRef\(/g) || []).length).toBeGreaterThanOrEqual(3);
  });
});

describe('mayResendAfterInjection', () => {
  it('re-sends a mutation only when nothing received the first message', () => {
    const noReceiver = new Error('Could not establish connection. Receiving end does not exist.');
    expect(mayResendAfterInjection('click', noReceiver)).toBe(true);
    expect(mayResendAfterInjection('fill_by_id', noReceiver)).toBe(true);
  });

  it('never re-sends a mutation after a timeout: it may have run', () => {
    const timeout = new Error('Action timed out after 30000ms');
    for (const action of ['click', 'click_by_id', 'type', 'type_by_id', 'fill', 'fill_by_id']) {
      expect(MUTATING_ACTIONS.has(action)).toBe(true);
      expect(mayResendAfterInjection(action, timeout)).toBe(false);
    }
  });

  it('re-sends reads either way', () => {
    expect(mayResendAfterInjection('get_text', new Error('Action timed out after 30000ms'))).toBe(
      true
    );
  });
});
