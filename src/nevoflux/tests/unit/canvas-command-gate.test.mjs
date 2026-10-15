/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { commandVerdict } from '../../extensions/nevoflux-agent/background/canvas-command-gate.mjs';

describe('commandVerdict', () => {
  it('passes the daemon verdict through when it is a command verdict', () => {
    expect(commandVerdict({ success: true, data: { kind: 'command', allow: true } })).toEqual({ kind: 'command', allow: true });
    const no = { kind: 'command', allow: false, code: 'COMMAND_NOT_DECLARED', message: 'm' };
    expect(commandVerdict({ success: true, data: no })).toEqual(no);
  });
  it('an answer without the command kind is a refusal', () => {
    // An older daemon reads a command check as a browser-action check and may allow it.
    for (const res of [
      { success: true, data: { allow: true } },
      { success: true, data: { kind: 'action', allow: true } },
      { success: false, error: 'boom' },
      null,
      undefined,
      { success: true, data: { kind: 'command', allow: 'yes' } },
    ]) {
      const v = commandVerdict(res);
      expect(v.allow).toBe(false);
      expect(v.code).toBe('POLICY_UNAVAILABLE');
    }
  });
});
