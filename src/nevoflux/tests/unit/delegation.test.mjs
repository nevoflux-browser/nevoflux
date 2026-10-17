/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { readFileSync } from 'node:fs';
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
  StatusSnapshotGate,
  PROVISIONAL_DELEGATION_MS,
  DELEGATED_ASK_TIMEOUT_MS,
  askIsDelegated,
  SessionQueue,
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

  it("a re-sent session_origin after the task ended is only provisional: the person's own question still shows", () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    applyStatusSnapshot(live, snap('s1'), pending);
    expect(isDelegated(live, 's1', pending, 0)).toBe(true);
    applyStatusSnapshot(live, snap(null), pending);
    recordSessionOrigin(origins, origin('s1'), pending, 1000); // end-of-turn re-send (R19)
    expect(hasLiveTask(live, 's1')).toBe(false);
    // A question the person asks there carries no delegated stamp: shown.
    expect(askIsDelegated(live, pending, 's1', 60000, 1000)).toBe(false);
    applyStatusSnapshot(live, snap(null), pending);
    expect(isDelegated(live, 's1', pending, 1000)).toBe(false);
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

describe('provisional delegation (rulings B-T2-1, B-T2-2)', () => {
  // The snapshot (event bus) and session_origin (chat proxy) travel on
  // different channels: a delegated turn's first question or Canvas can beat
  // the snapshot that lists its task.
  it('a session_origin after the latest snapshot makes the session delegated', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    applyStatusSnapshot(live, snap(null), pending);
    expect(recordSessionOrigin(origins, origin('s1'), pending, 0)).toBe(true);
    expect(isDelegated(live, 's1', pending, 0)).toBe(true);
    expect(isDelegated(live, 's2', pending, 0)).toBe(false);
  });

  it('the next snapshot without that task releases it', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    applyStatusSnapshot(live, snap('other'), pending);
    expect(isDelegated(live, 's1', pending, 0)).toBe(false);
    expect(isDelegated(live, 'other', pending, 0)).toBe(true);
  });

  it('the next snapshot with that task keeps it delegated', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    applyStatusSnapshot(live, snap('s1'), pending);
    expect(pending.size).toBe(0);
    expect(isDelegated(live, 's1', pending, 0)).toBe(true);
  });

  it('a turn-end re-sent session_origin is delegated only until the next snapshot', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    applyStatusSnapshot(live, snap('s1'), pending);
    recordSessionOrigin(origins, origin('s1'), pending, 0); // turn end (v1.3 R19)
    expect(isDelegated(live, 's1', pending, 0)).toBe(true);
    applyStatusSnapshot(live, snap(null), pending); // the task ended
    expect(isDelegated(live, 's1', pending, 0)).toBe(false);
  });

  it('a malformed snapshot or origin changes nothing provisional', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    applyStatusSnapshot(live, { agents: 'x' }, pending);
    expect(isDelegated(live, 's1', pending, 0)).toBe(true);
    expect(recordSessionOrigin(origins, { session_id: 's2', origin: { kind: 'user' } }, pending, 0)).toBe(false);
    expect(isDelegated(live, 's2', pending, 0)).toBe(false);
  });

  it('a provisional session stops counting 30 s after its origin, snapshot or not', () => {
    expect(PROVISIONAL_DELEGATION_MS).toBe(30000);
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 1000);
    expect(isDelegated(live, 's1', pending, 1000 + PROVISIONAL_DELEGATION_MS - 1)).toBe(true);
    expect(isDelegated(live, 's1', pending, 1000 + PROVISIONAL_DELEGATION_MS)).toBe(false);
    expect(pending.has('s1')).toBe(false); // expired entries are dropped
  });

  it('a re-sent origin restarts the 30 s', () => {
    const origins = new Map();
    const live = new Set();
    const pending = new Map();
    recordSessionOrigin(origins, origin('s1'), pending, 0);
    recordSessionOrigin(origins, origin('s1'), pending, 20000);
    expect(isDelegated(live, 's1', pending, 40000)).toBe(true);
    expect(isDelegated(live, 's1', pending, 50000)).toBe(false);
  });

  it('expired entries do not pile up', () => {
    const origins = new Map();
    const pending = new Map();
    for (let i = 0; i < 500; i++) recordSessionOrigin(origins, origin(`s${i}`), pending, i * PROVISIONAL_DELEGATION_MS);
    expect(pending.size <= 2).toBe(true);
  });
});

describe('askIsDelegated (rulings B-T2-2, B-T2-3)', () => {
  it("the daemon's delegated stamp is 1 800 000 ms (contract R32)", () => {
    expect(DELEGATED_ASK_TIMEOUT_MS).toBe(1800000);
  });

  it('a session with a live task: held, stamp or not', () => {
    const live = new Set(['s1']);
    expect(askIsDelegated(live, new Map(), 's1', 60000, 0)).toBe(true);
    expect(askIsDelegated(live, null, 's1', undefined, 0)).toBe(true);
  });

  it("a session with a live task: a permission ask (86 400 000) is the task's, held", () => {
    expect(askIsDelegated(new Set(['s1']), new Map(), 's1', 86400000, 0)).toBe(true);
  });

  it('a provisional session with exactly the stamp: held', () => {
    const pending = new Map();
    recordSessionOrigin(new Map(), origin('s1'), pending, 0);
    expect(askIsDelegated(new Set(), pending, 's1', 1800000, 0)).toBe(true);
  });

  it("a provisional session's permission ask (86 400 000) may be the person's own: shown", () => {
    // Local permission asks carry 86 400 000 too; one arriving in the 30 s
    // after a turn-end session_origin must not be hidden for 24 h.
    const pending = new Map();
    recordSessionOrigin(new Map(), origin('s1'), pending, 0);
    expect(askIsDelegated(new Set(), pending, 's1', 86400000, 0)).toBe(false);
  });

  it('a provisional session without the stamp: shown', () => {
    const pending = new Map();
    recordSessionOrigin(new Map(), origin('s1'), pending, 0);
    for (const t of [60000, 1799999, 1800001, '1800000', undefined, null, 'x']) {
      expect(askIsDelegated(new Set(), pending, 's1', t, 0)).toBe(false);
    }
  });

  it('neither live nor provisional: shown, whatever the stamp', () => {
    expect(askIsDelegated(new Set(), new Map(), 's1', 1800000, 0)).toBe(false);
    expect(askIsDelegated(new Set(), new Map(), undefined, 1800000, 0)).toBe(false);
  });

  it('an expired provisional session with the stamp: shown', () => {
    const pending = new Map();
    recordSessionOrigin(new Map(), origin('s1'), pending, 0);
    expect(askIsDelegated(new Set(), pending, 's1', 1800000, PROVISIONAL_DELEGATION_MS)).toBe(false);
  });
});

describe('SessionQueue', () => {
  it("runs one session's jobs one after another", async () => {
    const queue = new SessionQueue();
    const order = [];
    let release;
    const first = queue.run('A', () => new Promise((r) => { release = r; }).then(() => order.push('a1')));
    const second = queue.run('A', async () => order.push('a2'));
    await Promise.resolve();
    await Promise.resolve();
    expect(order).toEqual([]);
    release();
    await first;
    await second;
    expect(order).toEqual(['a1', 'a2']);
  });

  it('does not make another session wait', async () => {
    const queue = new SessionQueue();
    const order = [];
    let release;
    const a = queue.run('A', () => new Promise((r) => { release = r; }).then(() => order.push('a')));
    await queue.run('B', async () => order.push('b'));
    expect(order).toEqual(['b']);
    release();
    await a;
    expect(order).toEqual(['b', 'a']);
  });

  it('a failed job neither blocks the next nor is swallowed, and idle sessions are forgotten', async () => {
    const queue = new SessionQueue();
    let threw = false;
    try {
      await queue.run('A', async () => { throw new Error('boom'); });
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);
    expect(await queue.run('A', async () => 7)).toBe(7);
    await new Promise((r) => setTimeout(r, 0));
    expect(queue.tails.size).toBe(0);
  });
});

const statusEvent = (event_id, timestamp_ms, payload = snap('s1')) => ({
  event_id,
  topic: 'system:agent:status',
  timestamp_ms,
  payload,
});

describe('StatusSnapshotGate', () => {
  it('admits each new snapshot once', () => {
    const gate = new StatusSnapshotGate();
    expect(gate.admit(statusEvent('e1', 100))).toBe(true);
    // A reconnect's sticky replay redelivers it: not a new snapshot, so it
    // must not clear sessions provisionally delegated since.
    expect(gate.admit(statusEvent('e1', 100))).toBe(false);
    expect(gate.admit(statusEvent('e2', 100))).toBe(true);
    expect(gate.admit(statusEvent('e3', 200))).toBe(true);
  });

  it('never admits a snapshot older than the newest admitted', () => {
    const gate = new StatusSnapshotGate();
    gate.admit(statusEvent('e2', 200));
    expect(gate.admit(statusEvent('e1', 100))).toBe(false);
  });

  it('rejects a malformed delivery without remembering it', () => {
    const gate = new StatusSnapshotGate();
    for (const bad of [undefined, null, {}, statusEvent('m', 500, { agents: 'x' }), statusEvent('n', 500, null)]) {
      expect(gate.admit(bad)).toBe(false);
    }
    expect(gate.admit(statusEvent('e1', 100))).toBe(true);
  });

  it('admits a delivery without id or time', () => {
    const gate = new StatusSnapshotGate();
    gate.admit(statusEvent('e1', 100));
    expect(gate.admit({ payload: snap() })).toBe(true);
    expect(gate.admit({ payload: snap() })).toBe(true);
  });

  it('remembers a bounded number of event ids', () => {
    const gate = new StatusSnapshotGate();
    for (let i = 0; i < 1000; i++) gate.admit(statusEvent(`e${i}`, i));
    expect(gate.admit(statusEvent('e999', 999))).toBe(false);
    // e0 was forgotten long ago; only its old time keeps it out now.
    expect(gate.admit(statusEvent('e0', 999))).toBe(true);
  });
});

const bgSource = () =>
  readFileSync(
    new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
    'utf8'
  ).replace(/\r\n/g, '\n');

const fnBody = (src, signature) => {
  const start = src.indexOf(signature);
  expect(start).toBeGreaterThan(0);
  return src.slice(start, src.indexOf('\n}\n', start));
};

describe('background.js delegated sessions', () => {
  const bg = bgSource();

  it('imports the delegation rules', () => {
    expect(
      bg.includes(
        "import { recordSessionOrigin, isDelegated, askIsDelegated, askHoldMs, CanvasTabs, canvasOpenPlan, canvasChatSessionFor, applyStatusSnapshot, StatusSnapshotGate, SessionQueue } from './delegation.mjs';"
      )
    ).toBe(true);
  });

  it('delegated means a running task, or an origin newer than the latest snapshot', () => {
    const helper = fnBody(bg, 'function isDelegatedSession(sessionId) {');
    expect(helper.includes('return isDelegated(liveTaskSessions, sessionId, provisionalSessions, Date.now());')).toBe(true);
  });

  it('follows system:agent:status from connect, sidebar open or closed', () => {
    const ensure = fnBody(bg, 'function ensureBackgroundAgentStatusSubscription() {');
    expect(ensure.includes("eventBusSubscriptions.set('bg-agent-status', {")).toBe(true);
    expect(ensure.includes("patterns: ['system:agent:status'],")).toBe(true);
    const connect = bg.indexOf('        ensureBackgroundAgentStatusSubscription();');
    expect(connect).toBeGreaterThan(0);
    expect(connect < bg.indexOf('this.replaySubscriptions();', connect)).toBe(true);
    expect(
      bg.includes("} else if (subId === 'bg-agent-status') {\n            handleAgentStatusEvent(eventPayload.event);")
    ).toBe(true);
    const handler = fnBody(bg, 'function handleAgentStatusEvent(event) {');
    expect(handler.includes('if (!agentStatusGate.admit(event)) return;')).toBe(true);
    expect(handler.includes('applyStatusSnapshot(liveTaskSessions, event.payload, provisionalSessions);')).toBe(true);
  });

  it('consumes session_origin before anything else sees the frame', () => {
    const start = bg.indexOf('  async handleChatMessage(message) {');
    expect(start).toBeGreaterThan(0);
    const body = bg.slice(start, bg.indexOf('\n  handleMcpMessage(message) {', start));
    const origin = body.indexOf("if (msgType === 'session_origin') {");
    expect(origin).toBeGreaterThan(0);
    expect(origin < body.indexOf('msgType === MessageTypes.ARTIFACT_START ||')).toBe(true);
    const block = body.slice(origin, body.indexOf('\n    }\n', origin));
    expect(block.includes('recordSessionOrigin(originSessions, message.payload, provisionalSessions, Date.now())')).toBe(true);
    expect(block.includes('broadcastToSidebar')).toBe(false);
    expect(block.includes('return;')).toBe(true);
  });

  it("delegated requests run in the background script, like an agent's", () => {
    expect(
      bg.includes(
        'if (DIRECT_ACTIONS.has(action) || globalThis.__nf_headless === true || isAgentRequest(payload) || isDelegatedSession(payload?.session_id)) {'
      )
    ).toBe(true);
  });

  it('every Canvas open goes through the per-session helper', () => {
    expect(bg.includes('browser.tabs.remove(_canvasTabId)')).toBe(false);
    // One session's opens run one after another, so two at once cannot orphan a tab.
    const queued = fnBody(bg, 'function openSessionCanvasTab(artifactId, sessionId) {');
    expect(queued.includes('canvasOpenQueue.run(sessionId, () => openSessionCanvasTabNow(artifactId, sessionId))')).toBe(true);
    const helper = fnBody(bg, 'async function openSessionCanvasTabNow(artifactId, sessionId) {');
    expect(helper.includes('canvasOpenPlan(sessionId, canvasTabs, isDelegatedSession(sessionId))')).toBe(true);
    expect(helper.includes('browser.nevoflux.openCanvasTab(artifactId, { inBackground: true })')).toBe(true);
    expect(helper.includes('markDelegatedTab(result.tabId)')).toBe(true);
    expect(bg.includes('await openSessionCanvasTab(id, payload.session_id);')).toBe(true);
    expect(bg.includes('await openSessionCanvasTab(id, sessionId);')).toBe(true);
    expect(bg.includes('handleCreateArtifactToolCall(tc, message.payload?.session_id)')).toBe(true);
    expect(bg.includes('return await executeCanvasRender(params, request.session_id);')).toBe(true);
  });

  it("the sidebar's open canvas retargeting a tab takes it from its session", () => {
    const at = bg.indexOf('              canvasTabs.forgetTab(_canvasTabId);\n              // Navigate to the correct artifact if it changed');
    expect(at).toBeGreaterThan(0);
  });

  it('a closed tab leaves its session', () => {
    const start = bg.indexOf('browser.tabs.onRemoved.addListener((tabId) => {\n  if (tabId === _canvasTabId) {');
    expect(start).toBeGreaterThan(0);
    expect(bg.slice(start, bg.indexOf('\n});\n', start)).includes('canvasTabs.forgetTab(tabId);')).toBe(true);
  });

  it('a delegated question is neither shown nor timed out locally', () => {
    const ask = fnBody(bg, 'async function executeAskUser(params, daemonRequestId = null, sessionId = null) {');
    expect(
      ask.includes('const delegated = askIsDelegated(liveTaskSessions, provisionalSessions, sessionId, params.timeout_ms, Date.now());')
    ).toBe(true);
    // A delegated question never fired ask_open, so it must not fire ask_close.
    expect(ask.includes("if (!delegated) AvatarController.onEvent('ask_close');")).toBe(true);
    expect(
      fnBody(bg, 'function handleBrowserToolResolved(payload) {').includes(
        "if (!pending.delegated) AvatarController.onEvent('ask_close');"
      )
    ).toBe(true);
    expect(ask.includes('}, askHoldMs(delegated, timeout_ms));')).toBe(true);
    const skip = ask.indexOf('if (delegated) {');
    expect(skip).toBeGreaterThan(0);
    expect(skip < ask.indexOf('broadcastToSidebar({')).toBe(true);
    expect(bg.includes('return await executeAskUser(params, request.request_id, request.session_id);')).toBe(true);
    expect(fnBody(bg, 'function replayPendingAsks() {').includes('if (pending.delegated) continue;')).toBe(true);
  });

  it('artifact cards sent to the sidebar name their session', () => {
    const create = fnBody(bg, "async function handleCreateArtifactToolCall(toolCall, sessionId = '') {");
    expect(create.includes("session_id: sessionId || '',")).toBe(true);
    expect(create.includes("payload: { id, title, session_id: sessionId || '' },")).toBe(true);
    const render = fnBody(bg, "async function executeCanvasRender(params, sessionId = '') {");
    expect(render.includes("payload: { id, content_type: 'project', title, session_id: sessionId || '' },")).toBe(true);
    expect(render.includes("payload: { id, title, session_id: sessionId || '' },")).toBe(true);
  });

  it('canvas chats only take frames that name them, or name nobody', () => {
    expect(
      bg.includes(
        'const canvasSessionId = canvasChatSessionFor(message.payload, canvasSessions, _activeCanvasSessionId);'
      )
    ).toBe(true);
    expect(bg.includes('canvasSessionId = _activeCanvasSessionId;')).toBe(false);
  });
});
