/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import {
  afterCandidateClick,
  mayUseCoordinateFallback,
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
