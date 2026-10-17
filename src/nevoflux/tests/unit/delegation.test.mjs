/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import {
  DELEGATED_SESSION_CAP,
  DELEGATED_ASK_HOLD_MS,
  recordSessionOrigin,
  originOf,
  isDelegated,
  applyStatusSnapshot,
  hasLiveTask,
  askHoldMs,
  CanvasTabs,
  canvasOpenPlan,
  canvasChatSessionFor,
} from '../../extensions/nevoflux-agent/background/delegation.mjs';

const origin = (session_id, pairing = 'chan-1') => ({ session_id, origin: { kind: 'muse', pairing } });

describe('recordSessionOrigin', () => {
  it('remembers a delegated session from the contract frame', () => {
    const sessions = new Map();
    expect(recordSessionOrigin(sessions, origin('s1'))).toBe(true);
    expect(originOf(sessions, 's1')).toEqual({ pairing: 'chan-1' });
    expect(originOf(sessions, 's2')).toBeNull();
  });

  it('ignores anything that is not a muse origin with a session', () => {
    const sessions = new Map();
    for (const bad of [
      undefined,
      {},
      { session_id: '', origin: { kind: 'muse', pairing: 'c' } },
      { session_id: 's', origin: { kind: 'user', pairing: 'c' } },
      { session_id: 's', origin: { kind: 'muse' } },
      { session_id: 7, origin: { kind: 'muse', pairing: 'c' } },
    ]) {
      expect(recordSessionOrigin(sessions, bad)).toBe(false);
    }
    expect(sessions.size).toBe(0);
    expect(originOf(sessions, '')).toBeNull();
    expect(originOf(sessions, undefined)).toBeNull();
  });

  it('forgets the oldest session past the cap, and a repeat refreshes it', () => {
    const sessions = new Map();
    for (let i = 0; i < DELEGATED_SESSION_CAP; i++) recordSessionOrigin(sessions, origin(`s${i}`));
    recordSessionOrigin(sessions, origin('s0')); // continued task: now the newest
    recordSessionOrigin(sessions, origin('fresh'));
    expect(sessions.size).toBe(DELEGATED_SESSION_CAP);
    expect(originOf(sessions, 's0')).not.toBeNull();
    expect(originOf(sessions, 's1')).toBeNull();
    expect(originOf(sessions, 'fresh')).not.toBeNull();
  });
});

const snap = (...sessionIds) => ({
  agents: sessionIds.map((id, i) => ({
    control_channel_id: `c${i}`,
    task: id === null ? null : { id: `t${i}`, session_id: id, state: 'running' },
  })),
});

describe('live task sessions (ruling B-F2)', () => {
  it('a session with an origin but no running task is not delegated', () => {
    const origins = new Map();
    const live = new Set();
    recordSessionOrigin(origins, origin('s1'));
    expect(originOf(origins, 's1')).toEqual({ pairing: 'chan-1' });
    expect(isDelegated(live, 's1')).toBe(false);
    expect(hasLiveTask(live, 's1')).toBe(false);
  });

  it('a task in the snapshot makes its session delegated; a later snapshot without it releases it', () => {
    const live = new Set();
    expect(applyStatusSnapshot(live, snap('s1', 's2'))).toBe(true);
    expect(isDelegated(live, 's1')).toBe(true);
    expect(isDelegated(live, 's2')).toBe(true);
    expect(applyStatusSnapshot(live, snap('s2'))).toBe(true);
    expect(isDelegated(live, 's1')).toBe(false);
    expect(isDelegated(live, 's2')).toBe(true);
    expect(applyStatusSnapshot(live, { agents: [] })).toBe(true);
    expect(live.size).toBe(0);
  });

  it('a re-sent session_origin after the task ended does not hold the session again', () => {
    const origins = new Map();
    const live = new Set();
    recordSessionOrigin(origins, origin('s1'));
    applyStatusSnapshot(live, snap('s1'));
    expect(isDelegated(live, 's1')).toBe(true);
    applyStatusSnapshot(live, snap(null));
    recordSessionOrigin(origins, origin('s1')); // end-of-turn re-send (R19)
    expect(isDelegated(live, 's1')).toBe(false);
  });

  it('every snapshot replaces the set, offline agents included, agents without a task skipped', () => {
    const live = new Set(['stale']);
    applyStatusSnapshot(live, {
      agents: [
        { control_channel_id: 'a', online: false, task: { id: 't', session_id: 'off' } },
        { control_channel_id: 'b', task: null },
        { control_channel_id: 'c' },
        { control_channel_id: 'd', task: { id: 'u', session_id: '' } },
      ],
    });
    expect([...live]).toEqual(['off']);
  });

  it('a malformed snapshot changes nothing', () => {
    const live = new Set(['s1']);
    for (const bad of [undefined, null, {}, { agents: 'x' }]) {
      expect(applyStatusSnapshot(live, bad)).toBe(false);
    }
    expect([...live]).toEqual(['s1']);
  });

  it('applying an identical snapshot reports no change', () => {
    const live = new Set();
    applyStatusSnapshot(live, snap('s1'));
    expect(applyStatusSnapshot(live, snap('s1'))).toBe(false);
  });

  it('the hold predicates answer false for missing ids', () => {
    const live = new Set(['s1']);
    expect(isDelegated(live, undefined)).toBe(false);
    expect(isDelegated(live, '')).toBe(false);
  });
});

describe('askHoldMs', () => {
  it('a delegated question waits longer than the daemon', () => {
    // The daemon holds a delegated approval for 1800 s (contract §11) and
    // settles it itself; a local timer firing first would answer "timed out".
    expect(DELEGATED_ASK_HOLD_MS > 1800 * 1000).toBe(true);
    expect(askHoldMs(true, 60000)).toBe(DELEGATED_ASK_HOLD_MS);
    expect(askHoldMs(true, undefined)).toBe(DELEGATED_ASK_HOLD_MS);
  });

  it('a local question keeps the time it asked for', () => {
    expect(askHoldMs(false, 15000)).toBe(15000);
    expect(askHoldMs(false, undefined)).toBe(60000);
    expect(askHoldMs(false, -1)).toBe(60000);
  });
});

describe('Canvas tabs per session', () => {
  it("a new Canvas in session B does not close session A's Canvas tab", () => {
    const tabs = new CanvasTabs();
    tabs.set('A', 11);
    const plan = canvasOpenPlan('B', tabs, false);
    expect(plan).toEqual({ replaceTabId: null, inBackground: false, markAgent: false });
    tabs.set('B', 12);
    expect(tabs.tabFor('A')).toBe(11);
  });

  it('a new Canvas in the same session replaces only that session tab', () => {
    const tabs = new CanvasTabs();
    tabs.set('A', 11);
    tabs.set('B', 12);
    expect(canvasOpenPlan('A', tabs, false).replaceTabId).toBe(11);
  });

  it('a delegated Canvas opens in the background with the agent badge', () => {
    const tabs = new CanvasTabs();
    expect(canvasOpenPlan('muse-1', tabs, true)).toEqual({
      replaceTabId: null,
      inBackground: true,
      markAgent: true,
    });
  });

  it('a frame with no session opens nothing and touches no tab', () => {
    const tabs = new CanvasTabs();
    tabs.set('A', 11);
    for (const missing of [undefined, null, '', 42]) {
      expect(canvasOpenPlan(missing, tabs, false)).toBeNull();
    }
    expect(tabs.tabFor('A')).toBe(11);
  });

  it('a closed tab belongs to no session any more', () => {
    const tabs = new CanvasTabs();
    tabs.set('A', 11);
    tabs.set('B', 12);
    tabs.forgetTab(11);
    expect(tabs.tabFor('A')).toBeNull();
    expect(tabs.tabFor('B')).toBe(12);
    expect(tabs.tabFor('nobody')).toBeNull();
  });
});

describe('canvasChatSessionFor', () => {
  const canvas = new Map([['cs_1', { active: true }]]);

  it('a frame naming a canvas chat goes to it', () => {
    expect(canvasChatSessionFor({ session_id: 'cs_1' }, canvas, null)).toBe('cs_1');
  });

  it("another session's frame never falls back to the active canvas chat", () => {
    // A delegated task's or the sidebar's own output must not be pushed into a
    // canvas, nor end its chat with an agent_state idle.
    expect(canvasChatSessionFor({ session_id: 'muse-1' }, canvas, 'cs_1')).toBeNull();
  });

  it('only a frame without a session falls back to the active canvas chat', () => {
    expect(canvasChatSessionFor({}, canvas, 'cs_1')).toBe('cs_1');
    expect(canvasChatSessionFor(undefined, canvas, 'cs_1')).toBe('cs_1');
    expect(canvasChatSessionFor({}, canvas, 'cs_gone')).toBeNull();
    expect(canvasChatSessionFor({}, canvas, null)).toBeNull();
  });
});
