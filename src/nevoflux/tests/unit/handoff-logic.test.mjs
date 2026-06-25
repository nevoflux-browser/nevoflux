/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { makeRecordingId, recordingTracePath, buildSkillCreatorOpeningPrompt }
  from '../../extensions/nevoflux-agent/content/handoff-logic.mjs';

describe('handoff-logic', () => {
  it('makeRecordingId is rec_-prefixed and deterministic', () => {
    const a = makeRecordingId(['tab7', '12345']);
    const b = makeRecordingId(['tab7', '12345']);
    expect(a.startsWith('rec_')).toBe(true);
    expect(a).toBe(b);
  });
  it('recordingTracePath joins with forward slash', () => {
    expect(recordingTracePath('/data/recordings', 'rec_x')).toBe('/data/recordings/rec_x.jsonl');
  });
  it('opening prompt embeds path + goal + R&R pointer', () => {
    const p = buildSkillCreatorOpeningPrompt({ tracePath: '/d/rec_x.jsonl', goalHint: 'book a flight' });
    expect(p.includes('/d/rec_x.jsonl')).toBe(true);
    expect(p.includes('book a flight')).toBe(true);
    expect(p.toLowerCase().includes('record')).toBe(true);
  });
});
