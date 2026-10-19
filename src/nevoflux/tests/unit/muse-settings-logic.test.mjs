/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for muse-settings-logic.mjs — the DOM-free logic backing the
 * Muse section on nevoflux://settings. Inputs are the daemon's
 * `remote.pairings` rows and the sticky `system:agent:status` snapshot.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  AGENT_STATUS_TOPIC,
  TIERS,
  TIER_LABELS,
  GUARDED_LABEL,
  GUARDED_HELP,
  EMPTY_TEXT,
  isTier,
  displayName,
  stateLabel,
  nextSnapshot,
  channelSetChanged,
  museRows,
  confirmsGuarded,
  unconfirmedReason,
  tierError,
  settingError,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/muse-settings-logic.mjs';

const agent = (id, extra = {}) => ({
  control_channel_id: id,
  kind: 'agent',
  label: null,
  created_at: 1792000000,
  can_be_woken: false,
  tier: 'read-only',
  ...extra,
});

const device = (id) => ({
  control_channel_id: id,
  kind: 'device',
  label: 'Phone',
  created_at: 1792000000,
  can_be_woken: true,
  tier: null,
});

const snap = (agents, ts = 1000) => nextSnapshot(null, { agents }, ts);

describe('muse settings logic', () => {
  it('uses the exact contract strings', () => {
    expect(AGENT_STATUS_TOPIC).toBe('system:agent:status');
    expect(GUARDED_LABEL).toBe('Let Muse approve sensitive actions');
    expect(GUARDED_HELP).toBe(
      'Muse can approve scheduling, packs, model switching, screen sharing and prompt changes for its tasks. Off: only you can, here.'
    );
    expect(EMPTY_TEXT).toBe('No AI agent is paired. Type /pair-agent in the sidebar to pair one.');
    expect(tierError('NOT_FOUND')).toBe('Could not change the tier: NOT_FOUND');
    expect(settingError('store failed')).toBe('Could not change the setting: store failed');
  });

  it('lists the four tiers least to most, each with a label', () => {
    expect(TIERS).toEqual(['read-only', 'browser-auto', 'browser-auto-local-read', 'full-auto']);
    for (const t of TIERS) {
      expect(typeof TIER_LABELS[t]).toBe('string');
    }
  });

  it('validates tiers by exact spelling', () => {
    expect(isTier('full-auto')).toBe(true);
    expect(isTier('Full-Auto')).toBe(false);
    expect(isTier('full_auto')).toBe(false);
    expect(isTier('')).toBe(false);
    expect(isTier(null)).toBe(false);
    expect(isTier(undefined)).toBe(false);
  });

  it('excludes devices', () => {
    const rows = museRows([device('d1'), agent('a1'), device('d2')], snap([]));
    expect(rows.map((r) => r.channelId)).toEqual(['a1']);
  });

  it('merges the pairing tier with the snapshot state and flag', () => {
    const rows = museRows(
      [agent('a1', { label: 'Work Muse', tier: 'browser-auto' }), agent('a2', { tier: 'full-auto' })],
      snap([
        { channel_id: 'a2', label: null, state: 'paused', muse_may_approve_guarded: false },
        { channel_id: 'a1', label: 'Work Muse', state: 'connected', muse_may_approve_guarded: true },
      ])
    );
    expect(rows.length).toBe(2);
    expect(rows[0]).toEqual({
      channelId: 'a1',
      name: 'Work Muse',
      state: 'connected',
      stateLabel: 'Connected',
      heading: 'Work Muse · Connected',
      tier: 'browser-auto',
      known: true,
      guarded: true,
    });
    expect(rows[1].heading).toBe('Muse · Paused');
    expect(rows[1].tier).toBe('full-auto');
    expect(rows[1].guarded).toBe(false);
  });

  it('shows a pairing the snapshot does not name as offline with an unknown switch', () => {
    const rows = museRows([agent('a1'), agent('a2')], snap([{ channel_id: 'a2', state: 'connected' }]));
    expect(rows[0].state).toBe('offline');
    expect(rows[0].heading).toBe('Muse · Offline');
    expect(rows[0].known).toBe(false);
    expect(rows[0].guarded).toBeNull();
    // Named but without the key (an older daemon): off, and known.
    expect(rows[1].known).toBe(true);
    expect(rows[1].guarded).toBe(false);
  });

  it('shows every pairing as unknown before the first snapshot', () => {
    const rows = museRows([agent('a1')], null);
    expect(rows[0].guarded).toBeNull();
    expect(rows[0].stateLabel).toBe('Offline');
  });

  it('reads an unknown or offline state as offline, never connected', () => {
    expect(stateLabel('connected')).toBe('Connected');
    expect(stateLabel('paused')).toBe('Paused');
    expect(stateLabel('offline')).toBe('Offline');
    expect(stateLabel('reconnecting')).toBe('Offline');
    expect(stateLabel(undefined)).toBe('Offline');
    const rows = museRows([agent('a1')], snap([{ channel_id: 'a1', state: 'reconnecting' }]));
    expect(rows[0].heading).toBe('Muse · Offline');
  });

  it('falls back to Muse for a missing or blank label, as the sidebar row does', () => {
    expect(displayName(null)).toBe('Muse');
    expect(displayName('   ')).toBe('Muse');
    expect(displayName('  Home  ')).toBe('Home');
    const rows = museRows(
      [agent('a1', { label: '  ' }), agent('a2', { label: 'Pairing name' }), agent('a3')],
      snap([
        { channel_id: 'a1', label: null, state: 'connected' },
        { channel_id: 'a2', label: '', state: 'connected' },
        { channel_id: 'a3', label: 'Live name', state: 'connected' },
      ])
    );
    expect(rows.map((r) => r.name)).toEqual(['Muse', 'Pairing name', 'Live name']);
  });

  it('drops a pairing tier the page does not know', () => {
    const rows = museRows([agent('a1', { tier: 'god-mode' }), agent('a2', { tier: null })], null);
    expect(rows[0].tier).toBeNull();
    expect(rows[1].tier).toBeNull();
  });

  it('copes with junk input', () => {
    expect(museRows(null, null)).toEqual([]);
    expect(museRows([null, { kind: 'agent' }, agent('a1')], { agents: [null] }).length).toBe(1);
  });

  it('replaces the snapshot entirely and ignores older or malformed deliveries', () => {
    let held = nextSnapshot(null, { agents: [{ channel_id: 'c1' }, { channel_id: 'c2' }] }, 1000);
    held = nextSnapshot(held, { agents: [{ channel_id: 'c1', state: 'paused' }] }, 2000) || held;
    expect(held.agents.length).toBe(1);
    expect(held.publishedMs).toBe(2000);
    expect(nextSnapshot(held, { agents: [] }, 1500)).toBeNull();
    expect(nextSnapshot(held, { oops: true }, 3000)).toBeNull();
    expect(nextSnapshot(held, { agents: [] }, 2000)).toBeTruthy();
    // An entry without a channel id is dropped, not the whole snapshot.
    expect(nextSnapshot(null, { agents: [{ label: 'x' }, { channel_id: 'c' }] }, 1).agents.length).toBe(1);
  });

  it('detects a change in the set of channel ids, not in their order or state', () => {
    const a = snap([{ channel_id: 'c1', state: 'connected' }, { channel_id: 'c2' }]);
    const reordered = snap([{ channel_id: 'c2' }, { channel_id: 'c1', state: 'paused' }]);
    const added = snap([{ channel_id: 'c1' }, { channel_id: 'c2' }, { channel_id: 'c3' }]);
    const swapped = snap([{ channel_id: 'c1' }, { channel_id: 'c4' }]);
    expect(channelSetChanged(null, a)).toBe(true);
    expect(channelSetChanged(a, reordered)).toBe(false);
    expect(channelSetChanged(a, added)).toBe(true);
    expect(channelSetChanged(a, swapped)).toBe(true);
    expect(channelSetChanged(a, snap([]))).toBe(true);
  });

  it('needs the daemon to confirm the state asked for', () => {
    expect(confirmsGuarded({ enabled: true }, true)).toBe(true);
    expect(confirmsGuarded({ enabled: false }, false)).toBe(true);
    expect(confirmsGuarded({ enabled: false }, true)).toBe(false);
    expect(confirmsGuarded({}, true)).toBe(false);
    expect(confirmsGuarded(null, false)).toBe(false);
    expect(unconfirmedReason(true)).toBe('the daemon did not confirm the setting is on');
  });
});
