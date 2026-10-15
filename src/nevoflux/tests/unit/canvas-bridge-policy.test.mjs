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
  mergeQueryResults,
  chatRequest,
  sidebarText,
  notifyContent,
  isReservedTopic,
  shareArgs,
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
  it('legacy fallback reads a non-reserved flat key', () => {
    expect(legacyKeyReadable('notes:items')).toBe(true);
    for (const k of ['config:settings', 'canvas:x', 'session:y', 'app:other:k', '', undefined]) {
      expect(legacyKeyReadable(k)).toBe(false);
    }
  });
  it('legacy query never surfaces reserved keys', () => {
    const own = [{ key: 'app:a1:notes:1', value: 1 }, { key: 'app:b2:notes:1', value: 'other app' }];
    const legacy = [
      { key: 'notes:1', value: 'old' },
      { key: 'notes:2', value: 2 },
      { key: 'config:settings', value: 'secret' },
      { key: 'canvas:x', value: 'artifact' },
    ];
    const { results, toMigrate } = mergeQueryResults('a1', own, legacy);
    expect(results).toEqual([{ key: 'notes:1', value: 1 }, { key: 'notes:2', value: 2 }]);
    expect(toMigrate).toEqual([{ key: 'notes:2', value: 2 }]);
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
