/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import {
  afterCandidateClick,
  mayUseCoordinateFallback,
  MUTATING_ACTIONS,
  mayResendAfterInjection,
} from '../../extensions/nevoflux-agent/background/click-policy.mjs';

describe('afterCandidateClick', () => {
  it('stops after any click that was sent, seen effect or not', () => {
    expect(afterCandidateClick({ success: true, effective: true, effect: 'observed' })).toBe(
      'done'
    );
    expect(afterCandidateClick({ success: true, effective: false, effect: 'none_observed' })).toBe(
      'done'
    );
  });

  it('stops on a covered target: every candidate sits under the same cover', () => {
    expect(afterCandidateClick({ success: false, error: { code: 1003, message: 'covered' } })).toBe(
      'done'
    );
  });

  it('tries the next candidate only when this one could not be clicked', () => {
    expect(afterCandidateClick({ success: false, error: { code: 1001 } })).toBe('next');
    expect(afterCandidateClick({ success: false, error: { code: 1002 } })).toBe('next');
    expect(afterCandidateClick(undefined)).toBe('next');
  });
});

describe('mayUseCoordinateFallback', () => {
  it('only when no click was sent and the target was not covered', () => {
    expect(mayUseCoordinateFallback({ anyClickSent: false, covered: false })).toBe(true);
    expect(mayUseCoordinateFallback({ anyClickSent: true, covered: false })).toBe(false);
    expect(mayUseCoordinateFallback({ anyClickSent: false, covered: true })).toBe(false);
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
