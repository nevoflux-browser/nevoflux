/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for pendant-logic.mjs — the Pendant section of nevoflux://settings.
 * Inputs are the daemon's `pendant.ports` / `pendant.wifi` / `pendant.list`
 * answers and its `system:pendant:job` events.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  isTerminal,
  progress,
  phaseText,
  linkLabel,
  portOption,
  defaultPort,
  replaceNotice,
  wifiStart,
  isOurs,
  pendantRow,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/pendant-logic.mjs';

describe('pendant-logic: a job as it goes', () => {
  it('knows when a job is over', () => {
    expect(isTerminal('done')).toBe(true);
    expect(isTerminal('failed')).toBe(true);
    expect(isTerminal('cancelled')).toBe(true);
    expect(isTerminal('confirm')).toBe(false);
  });

  it('counts steps for a pairing and a Wi-Fi change', () => {
    expect(progress('pair', 'probing')).toEqual({ steps: 6, done: 0 });
    expect(progress('pair', 'confirm')).toEqual({ steps: 6, done: 3 });
    expect(progress('pair', 'replacing')).toEqual({ steps: 6, done: 4 });
    expect(progress('pair', 'done')).toEqual({ steps: 6, done: 6 });
    expect(progress('wifi', 'writing')).toEqual({ steps: 3, done: 2 });
  });

  it('says what is happening and how it ended', () => {
    expect(phaseText({ phase: 'confirm' })).toBe('Press BOOT on the pendant');
    expect(phaseText({ phase: 'done', kind: 'pair', name: 'NevoFlux-A1B2', link: 'lan' })).toBe(
      'NevoFlux-A1B2 is paired and connected (LAN).'
    );
    expect(phaseText({ phase: 'done', kind: 'pair', name: 'NevoFlux-A1B2', link: null })).toBe(
      'NevoFlux-A1B2 is paired. It connects once it is on Wi-Fi.'
    );
    expect(phaseText({ phase: 'done', kind: 'wifi', name: 'NevoFlux-A1B2' })).toMatch(/Wi-Fi updated/);
    expect(phaseText({ phase: 'failed', reason: 'BOOT was not pressed' })).toBe('Could not finish: BOOT was not pressed');
    expect(phaseText(null)).toBe('');
  });

  it('only follows the job this page started', () => {
    expect(isOurs({ job: 'a' }, 'a')).toBe(true);
    expect(isOurs({ job: 'b' }, 'a')).toBe(false);
    expect(isOurs({ job: 'b' }, null)).toBe(true);
  });
});

describe('pendant-logic: ports and the paired list', () => {
  const rows = [
    { port: 'COM3', error: 'the pendant did not answer' },
    { port: 'COM5', name: 'NevoFlux-A1B2', paired: true, paired_here: true },
    { port: 'COM7', busy: true },
  ];

  it('labels each port and picks the usable one', () => {
    expect(portOption(rows[0])).toEqual({ value: 'COM3', label: 'COM3 — not answering', usable: false });
    expect(portOption(rows[1]).label).toBe('NevoFlux-A1B2 (COM5) · paired here');
    expect(portOption(rows[2]).usable).toBe(false);
    expect(defaultPort(rows)).toBe('COM5');
    expect(defaultPort([])).toBe('');
  });

  it('warns what pairing again does', () => {
    expect(replaceNotice(rows[1])).toMatch(/replaces that pairing/);
    expect(replaceNotice({ paired: true })).toMatch(/another computer/);
    expect(replaceNotice({ paired: false })).toBe('');
  });

  it('shows how a pendant is connected', () => {
    expect(linkLabel('ble')).toBe('Bluetooth');
    expect(linkLabel(null)).toBe('Offline');
    const r = pendantRow({ control_channel_id: 'c', name: 'NevoFlux-A1B2', online: true, link: 'relay', paired_at: 1762400000 });
    expect(r.status).toBe('Connected · Relay');
    expect(r.since.length > 0).toBe(true);
    expect(pendantRow({ online: false }).status).toBe('Offline');
  });
});

describe('pendant-logic: the Wi-Fi form', () => {
  it('starts from the computer network when it is 2.4 GHz', () => {
    const s = wifiStart({ current: { ssid: 'HomeNet', band: '2.4' }, password: 'pw', nearby: [{ ssid: 'HomeNet' }] });
    expect(s).toEqual({ ssid: 'HomeNet', password: 'pw', warning: '', choices: ['HomeNet'] });
  });

  it('catches 5 GHz and offers the same router 2.4 GHz name', () => {
    const s = wifiStart({
      current: { ssid: 'HomeNet-5G', band: '5' },
      password: 'pw',
      nearby: [{ ssid: 'Cafe' }, { ssid: 'HomeNet' }],
    });
    expect(s.ssid).toBe('HomeNet');
    expect(s.password).toBe('pw');
    expect(s.warning).toMatch(/only joins 2.4 GHz/);
    expect(s.warning).toMatch(/HomeNet looks like/);
  });

  it('asks for a choice when no sibling is in range', () => {
    const s = wifiStart({ current: { ssid: 'Office', band: '5' }, password: 'pw', nearby: [{ ssid: 'Cafe' }] });
    expect(s.ssid).toBe('');
    expect(s.password).toBe('');
    expect(s.warning).toMatch(/pick a 2.4 GHz network/);
  });

  it('starts empty off Windows', () => {
    expect(wifiStart({ current: null, nearby: [] })).toEqual({ ssid: '', password: '', warning: '', choices: [] });
  });
});
