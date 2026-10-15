/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import {
  isAllowedMethod,
  isForbiddenCommand,
  appKey,
  appPrefix,
  legacyKeyReadable,
  TOMBSTONE,
  isTombstone,
  storageGetPlan,
  ownQueryResults,
  mintSessionId,
  isOwnSession,
  chatRequest,
  sidebarText,
  notifyContent,
  isReservedTopic,
  shareArgs,
  ownShares,
  isOwnShare,
  unwrapBridgeResult,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/canvas-bridge-policy.mjs';

describe('canvas-bridge-policy: methods', () => {
  it('allows the SDK methods canvases use today', () => {
    for (const m of ['callTool', 'agent.chat', 'agent.cancel', 'agent.sendCommand', 'storage.get',
      'storage.set', 'storage.delete', 'storage.query', 'sidebar.open', 'sidebar.send',
      'sidebar.notify', 'system.getInfo', 'canvas.share', 'canvas.tool.invoke', 'events.publish']) {
      expect(isAllowedMethod(m)).toBe(true);
    }
  });
  it('refuses anything else, including bridge types the SDK never had', () => {
    for (const m of ['agent:command', 'getCache', 'system.getConfig', 'storage.subscribe', '', undefined]) {
      expect(isAllowedMethod(m)).toBe(false);
    }
  });
});

describe('canvas-bridge-policy: commands', () => {
  it('forbids settings, pairing, accounts and the rest of the permanent list', () => {
    for (const c of ['config.llm.set', 'remote.agent_set_tier', 'account.status', 'content_store.set',
      'soul.write', 'local.install', 'kb.wizard.install_bun', 'mcp.add', 'pack.install', 'jev.set',
      'models.download', 'session.delete', 'session.clone', 'file.pick']) {
      expect(isForbiddenCommand(c)).toBe(true);
    }
  });
  it('matches prefixes on whole segments only', () => {
    for (const c of ['configure', 'accounting.report', 'mcp.list', 'session.list', 'schedule.run_now', 'daemon.info']) {
      expect(isForbiddenCommand(c)).toBe(false);
    }
  });
});

describe('canvas-bridge-policy: storage', () => {
  it('namespaces keys by artifact', () => {
    expect(appKey('a1', 'notes:items')).toBe('app:a1:notes:items');
    expect(appKey('a1', 'config:settings')).toBe('app:a1:config:settings');
    expect(appPrefix('a1', 'notes:')).toBe('app:a1:notes:');
    expect(appPrefix('a1')).toBe('app:a1:');
  });
  it('refuses a missing artifact id or a non-string key', () => {
    expect(() => appKey('', 'k')).toThrow();
    expect(() => appKey('a1', '')).toThrow();
    expect(() => appKey('a1', 42)).toThrow();
    expect(() => appPrefix(null, 'x')).toThrow();
  });
  it('refuses an artifact id containing a colon', () => {
    expect(() => appKey('a:b', 'k')).toThrow();
    expect(() => appPrefix('a:b', 'k')).toThrow();
  });
  it('legacy fallback reads a non-reserved flat key', () => {
    expect(legacyKeyReadable('notes:items')).toBe(true);
    for (const k of ['config:settings', 'canvas:x', 'session:y', 'app:other:k', '', undefined]) {
      expect(legacyKeyReadable(k)).toBe(false);
    }
  });
  it('query returns own namespace only, stripped, without tombstones', () => {
    const rows = [
      { key: 'app:a1:notes:1', value: 1 },
      { key: 'app:a1:gone', value: TOMBSTONE },
      { key: 'app:b2:notes:1', value: 'other app' },
      { key: 'notes:2', value: 'legacy' },
      { key: 'config:settings', value: 'secret' },
    ];
    expect(ownQueryResults('a1', rows)).toEqual([{ key: 'notes:1', value: 1 }]);
    expect(ownQueryResults('a1', undefined)).toEqual([]);
  });
  it('recognises the tombstone only', () => {
    expect(isTombstone(TOMBSTONE)).toBe(true);
    expect(isTombstone({ __nevofluxDeleted: true })).toBe(true);
    for (const v of [null, undefined, 0, '', 'x', {}, { __nevofluxDeleted: false }, [], false]) {
      expect(isTombstone(v)).toBe(false);
    }
  });
  it('get after delete is deleted and does not consult legacy', () => {
    expect(storageGetPlan(TOMBSTONE)).toBe('deleted');
    expect(storageGetPlan(undefined)).toBe('legacy');
    expect(storageGetPlan(null)).toBe('legacy');
    expect(storageGetPlan(0)).toBe('value');
    expect(storageGetPlan({ a: 1 })).toBe('value');
  });
});

describe('canvas-bridge-policy: sessions', () => {
  const U = '123e4567-e89b-12d3-a456-426614174000';
  it('mints ids bound to the canvas', () => {
    expect(mintSessionId('a1', U)).toBe(`cs_a1_${U}`);
    expect(isOwnSession('a1', mintSessionId('a1', U))).toBe(true);
    expect(isOwnSession('a1', `cs_a1_${U.toUpperCase()}`)).toBe(true);
  });
  it('refuses foreign, prefix-collision and malformed ids', () => {
    expect(isOwnSession('a1', `cs_${U}`)).toBe(false);
    expect(isOwnSession('a', `cs_a_b_${U}`)).toBe(false);
    expect(isOwnSession('a_b', `cs_a_b_${U}`)).toBe(true);
    expect(isOwnSession('a1', `cs_a2_${U}`)).toBe(false);
    expect(isOwnSession('a1', `cs_a1_${U}x`)).toBe(false);
    expect(isOwnSession('a1', 'cs_a1_')).toBe(false);
    for (const v of [undefined, null, 42, {}, '']) expect(isOwnSession('a1', v)).toBe(false);
    expect(isOwnSession('', `cs__${U}`)).toBe(false);
  });
});

describe('canvas-bridge-policy: agent.chat', () => {
  it('keeps images and turns paths into a note instead of local_files', () => {
    const out = chatRequest({
      message: 'look',
      attachments: [
        { name: 'p.png', mime_type: 'image/png', data: 'AAA' },
        { path: 'C:/Windows/win.ini' },
        { path: '/home/u/proj', is_directory: true },
      ],
    });
    expect(out.attachments).toEqual([{ name: 'p.png', mime_type: 'image/png', data: 'AAA' }]);
    expect(out.message.startsWith('look')).toBe(true);
    expect(out.message).toContain('- C:/Windows/win.ini');
    expect(out.message).toContain('- /home/u/proj (directory)');
  });
  it('sends a plain message unchanged and with no attachments field', () => {
    expect(chatRequest({ message: 'hi' })).toEqual({ message: 'hi' });
    expect(chatRequest({ message: { a: 1 } })).toEqual({ message: '{"a":1}' });
  });
});

describe('canvas-bridge-policy: sidebar', () => {
  it('labels text with the canvas it came from', () => {
    expect(sidebarText({ message: 'hello' }, 'a1')).toBe('[Canvas a1] hello');
  });
  it('refuses non-strings and overlong text', () => {
    expect(() => sidebarText({ message: { type: 'x' } }, 'a1')).toThrow();
    expect(() => sidebarText({ message: 'x'.repeat(4001) }, 'a1')).toThrow();
    expect(sidebarText({ message: 'x'.repeat(4000) }, 'a1').length).toBe(4000 + '[Canvas a1] '.length);
  });
  it('notify becomes a fixed title and message', () => {
    expect(notifyContent('Saved', 'All good')).toEqual({ title: 'Saved', message: 'All good' });
    expect(notifyContent('t', { message: 'm' })).toEqual({ title: 't', message: 'm' });
    expect(notifyContent(undefined, { type: 'session_title', payload: {} })).toEqual({ title: 'Canvas', message: '' });
    expect(notifyContent('x'.repeat(200), 'y'.repeat(500)).title.length).toBe(80);
    expect(notifyContent('t', 'y'.repeat(500)).message.length).toBe(300);
  });
  it('notify title names the canvas when a label is given', () => {
    expect(notifyContent(undefined, 'hi', 'Sales')).toEqual({ title: 'Canvas: Sales', message: 'hi' });
    expect(notifyContent('Saved', 'hi', 'Sales').title).toBe('Canvas: Sales \u2014 Saved');
    expect(notifyContent('t', 'x', 'l'.repeat(100)).title.length).toBeLessThan(100);
  });
});

describe('canvas-bridge-policy: events and share', () => {
  it('reserves the topics the browser itself listens on', () => {
    for (const t of ['ui:notification', 'ui:notification:agent', 'ui:soul:active', 'ui:render', 'ui:tab:3:click', 42, undefined]) {
      expect(isReservedTopic(t)).toBe(true);
    }
    for (const t of ['ui:app:a1:tick', 'ui:career:scan', 'ui:notifications-panel']) {
      expect(isReservedTopic(t)).toBe(false);
    }
  });
  it('a share always names the calling canvas', () => {
    expect(shareArgs('canvas.share', { artifact_id: 'someone-else', ttl_secs: 60 }, 'a1')).toEqual({ artifact_id: 'a1', ttl_secs: 60 });
    expect(shareArgs('canvas.share.list', { x: 1 }, 'a1')).toEqual({ x: 1 });
  });
  it('a share list shows only this canvas\'s shares', () => {
    const shares = [
      { artifact_id: 'a1', share_id: 's1' },
      { artifact_id: 'b2', share_id: 's2' },
    ];
    expect(ownShares(shares, 'a1')).toEqual([{ artifact_id: 'a1', share_id: 's1' }]);
    expect(ownShares({ shares, total: 2 }, 'a1')).toEqual({ shares: [{ artifact_id: 'a1', share_id: 's1' }], total: 2 });
    expect(ownShares(null, 'a1')).toEqual([]);
  });
  it('extend and delete reach only this canvas\'s own shares', () => {
    const list = { shares: [{ artifact_id: 'a1', share_id: 's1' }, { artifact_id: 'b2', share_id: 's2' }] };
    expect(isOwnShare(list, 'a1', 's1')).toBe(true);
    expect(isOwnShare(list, 'a1', 's2')).toBe(false);
    expect(isOwnShare(list, 'a1', '')).toBe(false);
    expect(isOwnShare(list, 'a1', undefined)).toBe(false);
    expect(isOwnShare(undefined, 'a1', 's1')).toBe(false);
  });
});

describe('canvas-bridge-policy: unwrapBridgeResult', () => {
  it('returns the inner data of a successful double-wrapped result', () => {
    expect(unwrapBridgeResult({ success: true, data: { success: true, data: { v: 1 } } }, 'x')).toEqual({ v: 1 });
    expect(unwrapBridgeResult({ success: true, data: { v: 2 } }, 'x')).toEqual({ v: 2 });
  });
  it('throws on either layer failing', () => {
    expect(() => unwrapBridgeResult({ success: false, error: { message: 'refused' } }, 'x')).toThrow();
    expect(() => unwrapBridgeResult({ success: true, data: { success: false, error: { message: 'FORBIDDEN_COMMAND: no' } } }, 'x')).toThrow();
    expect(() => unwrapBridgeResult(null, 'x')).toThrow();
  });
});
