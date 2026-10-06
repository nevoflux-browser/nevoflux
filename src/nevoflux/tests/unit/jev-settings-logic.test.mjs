/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for jev-settings-logic.mjs — the DOM-free logic backing the Jev
 * section on nevoflux://settings. Inputs are the daemon's `jev.get` /
 * `jev.test` data shapes (crates/daemon/src/jev/rpc.rs).
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  NOTICE_TEXT,
  POINTS,
  needsNotice,
  parseDomains,
  domainsText,
  scopeHint,
  keyPlaceholder,
  buildSetParams,
  formatTest,
  validTimeout,
  applyToggle,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/jev-settings-logic.mjs';

describe('jev settings logic', () => {
  it('enabling_without_ack_needs_the_notice', () => {
    expect(needsNotice({ enabling: true, acked: false })).toBe(true);
    expect(needsNotice({ enabling: true, acked: true })).toBe(false);
    expect(needsNotice({ enabling: false, acked: false })).toBe(false);
    expect(NOTICE_TEXT).toContain('sensitive-site list');
  });

  it('the notice is asked once', () => {
    expect(needsNotice({ enabling: true, acked: undefined })).toBe(true);
  });

  it('lists the five points with permissions off by default', () => {
    expect(POINTS.map((p) => p.key)).toEqual(['tools', 'skills', 'visibility', 'rebuild', 'permissions']);
    expect(POINTS[4].hint).toContain('Off by default');
  });

  it('parses domains like the daemon', () => {
    expect(parseDomains(' *.Bank.Example/login \n\n.bank.example\nhttps://mail.example.org/x ')).toEqual([
      'bank.example',
      'mail.example.org',
    ]);
    expect(domainsText(['a.example', 'b.example'])).toBe('a.example\nb.example');
  });

  it('gives the scope hints of spec 5.9 item 6', () => {
    expect(scopeHint({ scope: 'applies', permissions: false })).toBe(null);
    expect(scopeHint({ scope: 'local', permissions: false })).toContain('does not apply');
    expect(scopeHint({ scope: 'acp', permissions: false })).toContain('does not apply');
    expect(scopeHint({ scope: 'acp', permissions: true })).toContain('Only the permission check applies');
  });

  it('describes the key without revealing it', () => {
    expect(keyPlaceholder({ has_api_key: true, api_key: 'sk-...abcd', key_from_env: false })).toContain('sk-...abcd');
    expect(keyPlaceholder({ has_api_key: false, api_key: '', key_from_env: true })).toContain('NEVOFLUX_API_KEY_TYPESAFE');
    expect(keyPlaceholder({ has_api_key: false, api_key: '', key_from_env: false })).toContain('No key');
  });

  it('buildSetParams_omits_an_untouched_key', () => {
    expect(buildSetParams({ timeout_ms: 900 })).toEqual({ timeout_ms: 900 });
    expect(buildSetParams({ api_key: '' })).toEqual({});
    expect(buildSetParams({ api_key: 'sk-new' })).toEqual({ api_key: 'sk-new' });
    expect(buildSetParams({ clear_key: true })).toEqual({ api_key: null });
    expect(buildSetParams({ points: { tools: false } })).toEqual({ points: { tools: false } });
    expect(buildSetParams({ domains_text: 'a.example\n' })).toEqual({ sensitive_domains: ['a.example'] });
  });

  it('formats a connection test', () => {
    expect(formatTest({ latency_ms: [280, 312, 340], p50_ms: 312, suggested_timeout_ms: 800 })).toBe(
      'Median 312 ms (280 / 312 / 340). Suggested timeout: 800 ms.'
    );
  });

  it('bounds the timeout', () => {
    expect(validTimeout(800)).toBe(true);
    expect(validTimeout(99)).toBe(false);
    expect(validTimeout(10001)).toBe(false);
    expect(validTimeout(800.5)).toBe(false);
  });

  it('a failed save shows what the daemon really has', async () => {
    // Turning Jev off fails (daemon restarting): the toggle must not claim
    // it is off while page content keeps flowing.
    const r = await applyToggle({
      want: false,
      needsConfirm: false,
      confirm: async () => true,
      save: async () => false,
      reload: async () => ({ enabled: true }),
      read: (d) => d.enabled,
    });
    expect(r.checked).toBe(true);
  });

  it('turn on saves on, even if the page changed under the notice', async () => {
    const sent = [];
    let checkbox = true;
    const r = await applyToggle({
      want: checkbox,
      needsConfirm: true,
      confirm: async () => {
        checkbox = false; // another save's response repainted the toggle meanwhile
        return true;
      },
      save: async (v) => {
        sent.push(v);
        return true;
      },
      reload: async () => ({ enabled: false }),
      read: (d) => d.enabled,
    });
    expect(sent).toEqual([true]);
    expect(r.checked).toBe(true);
    expect(r.acked).toBe(true);
  });

  it('cancelling the notice saves nothing and stays off', async () => {
    const sent = [];
    const r = await applyToggle({
      want: true,
      needsConfirm: true,
      confirm: async () => false,
      save: async (v) => {
        sent.push(v);
        return true;
      },
      reload: async () => ({ enabled: false }),
      read: (d) => d.enabled,
    });
    expect(sent).toEqual([]);
    expect(r.checked).toBe(false);
    expect(r.acked).toBe(false);
  });

  it('when even the reload fails, the toggle shows the old state', async () => {
    const r = await applyToggle({
      want: true,
      needsConfirm: false,
      confirm: async () => true,
      save: async () => false,
      reload: async () => {
        throw new Error('daemon down');
      },
      read: (d) => d.enabled,
    });
    expect(r.checked).toBe(false);
  });
});
