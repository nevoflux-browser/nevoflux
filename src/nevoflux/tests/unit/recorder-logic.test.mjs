/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for recorder-logic.mjs — DOM-free recorder core.
 * Pure functions: no DOM, no chrome, no browser.* dependencies.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  mapEventToAction, isSecretField, buildStep, coalesceFill, convergeScroll, makeHeader,
} from '../../extensions/nevoflux-agent/content/recorder-logic.mjs';

describe('recorder-logic: mapEventToAction', () => {
  it('maps known events, ignores noise', () => {
    expect(mapEventToAction('click')).toBe('click');
    expect(mapEventToAction('change')).toBe('fill');
    expect(mapEventToAction('scroll')).toBe('scroll');
    expect(mapEventToAction('mousemove')).toBe(null);
  });
});

describe('recorder-logic: isSecretField', () => {
  it('flags password + token-shaped names', () => {
    expect(isSecretField({ tag: 'input', inputType: 'password', name: 'pw' })).toBe(true);
    expect(isSecretField({ tag: 'input', inputType: 'text', name: 'api_token' })).toBe(true);
    expect(isSecretField({ tag: 'input', inputType: 'text', name: 'email' })).toBe(false);
  });
  it('flags secret autocomplete values', () => {
    expect(isSecretField({ inputType: 'text', name: 'x', autocomplete: 'current-password' })).toBe(true);
    expect(isSecretField({ inputType: 'text', name: 'x', autocomplete: 'new-password' })).toBe(true);
    expect(isSecretField({ inputType: 'text', name: 'x', autocomplete: 'email' })).toBe(false);
    expect(isSecretField({ inputType: 'text', name: 'x', autocomplete: 'username' })).toBe(false);
  });
});

describe('recorder-logic: buildStep', () => {
  it('redacts secrets at source (value null, no input_ref)', () => {
    const s = buildStep({ action: 'fill', target: { role: 'textbox', name: 'Password' },
      value: 'hunter2', inputType: 'password', name: 'pw', url: 'u', tsMs: 5 });
    expect(s.value).toBe(null);
    expect(s.redacted).toBe(true);
    expect(s.input_ref).toBe(undefined);
    expect(s.i).toBe(undefined); // daemon assigns i
  });
  it('adds a candidate input_ref for normal fills', () => {
    const s = buildStep({ action: 'fill', target: { role: 'textbox', name: 'Email' },
      value: 'a@b.c', inputType: 'email', name: 'email', url: 'u', tsMs: 5 });
    expect(s.input_ref).toBe('{{email}}');
    expect(s.value).toBe('a@b.c');
    expect(s.type).toBe('step');
  });
  it('forces {{file}} for file inputs', () => {
    const s = buildStep({ action: 'fill', target: { role: 'button', name: 'Upload' },
      value: 'C:/x.pdf', inputType: 'file', name: 'doc', url: 'u', tsMs: 5 });
    expect(s.value).toBe('{{file}}');
    expect(s.target.element_kind).toBe('file');
  });
  it('redacts fills when autocomplete signals a secret', () => {
    const s = buildStep({ action: 'fill', target: { role: 'textbox', name: 'New password' },
      value: 's3cr3t', inputType: 'text', name: 'pwd_field', autocomplete: 'new-password',
      url: 'u', tsMs: 5 });
    expect(s.value).toBe(null);
    expect(s.redacted).toBe(true);
    expect(s.input_ref).toBe(undefined);
  });
  it('does not redact when autocomplete is non-secret', () => {
    const s = buildStep({ action: 'fill', target: { role: 'textbox', name: 'Email' },
      value: 'a@b.c', inputType: 'text', name: 'email', autocomplete: 'email',
      url: 'u', tsMs: 5 });
    expect(s.value).toBe('a@b.c');
    expect(s.redacted).toBe(undefined);
    expect(s.input_ref).toBe('{{email}}');
  });
});

describe('recorder-logic: coalesceFill / convergeScroll / makeHeader', () => {
  it('coalesce keeps the last value', () => {
    expect(coalesceFill([{ value: 'a' }, { value: 'ab' }, { value: 'abc' }])).toBe('abc');
  });
  it('converge reduces a scroll burst to one', () => {
    const out = convergeScroll([{ tsMs: 0 }, { tsMs: 50 }, { tsMs: 90 }], 100);
    expect(out.length).toBe(1);
  });
  it('makeHeader shapes the header line', () => {
    const h = makeHeader({ recordingId: 'rec_x', createdAt: 1, startUrl: 'u', goalHint: 'g' });
    expect(h.type).toBe('header');
    expect(h.recording_id).toBe('rec_x');
  });
});
