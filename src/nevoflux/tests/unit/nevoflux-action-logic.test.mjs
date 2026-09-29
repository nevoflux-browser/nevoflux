/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Imports the real module the actor imports — not a copy.
import { describe, it, expect } from './test-runner.mjs';
import { snapshotValue } from '../../engine-overlays/browser/actors/NevofluxActionLogic.sys.mjs';

describe('snapshotValue', () => {
  it('never shows a password, file or hidden input value', () => {
    for (const type of ['password', 'PASSWORD', 'file', 'hidden']) {
      expect(snapshotValue({ tagName: 'INPUT', type, value: 'hunter2' })).toBeNull();
    }
  });

  it('shows other values, cut to 30 characters', () => {
    expect(snapshotValue({ tagName: 'INPUT', type: 'text', value: 'Alice' })).toBe('Alice');
    expect(snapshotValue({ tagName: 'SELECT', type: 'select-one', value: 'XL' })).toBe('XL');
    const long = 'x'.repeat(40);
    expect(snapshotValue({ tagName: 'TEXTAREA', type: 'textarea', value: long })).toBe(
      'x'.repeat(27) + '...'
    );
  });

  it('shows nothing for an empty or missing value', () => {
    expect(snapshotValue({ tagName: 'INPUT', type: 'text', value: '' })).toBeNull();
    expect(snapshotValue({ tagName: 'DIV', type: undefined, value: undefined })).toBeNull();
  });
});
