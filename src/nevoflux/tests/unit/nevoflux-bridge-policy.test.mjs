/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { pageOf, checkActorMessage } from '../../engine-overlays/browser/modules/NevofluxBridgePolicy.sys.mjs';

const CANVAS = 'nevoflux://canvas/abc';
const SETTINGS = 'nevoflux://settings';
const req = (type, payload = {}) => ({ type, payload });
const ok = (v) => expect(v.allow).toBe(true);
const no = (v) => expect(v.allow).toBe(false);

describe('pageOf', () => {
  it('names nevoflux:// and chrome://nevoflux pages', () => {
    expect(pageOf('nevoflux://canvas/abc?mode=preview')).toBe('canvas');
    expect(pageOf('nevoflux://settings')).toBe('settings');
    expect(pageOf('nevoflux://pack-install/x')).toBe('pack-install');
    expect(pageOf('chrome://nevoflux/content/pages/render.html')).toBe('render');
    expect(pageOf('chrome://nevoflux/content/nevoflux/pages/canvas.html?id=1')).toBe('canvas');
  });
  it('is null for everything else', () => {
    for (const u of ['about:srcdoc', 'https://example.com/', 'chrome://browser/content/browser.xhtml', '', undefined]) {
      expect(pageOf(u)).toBeNull();
    }
  });
});

describe('checkActorMessage', () => {
  it('recording and dialog messages pass from any document', () => {
    for (const n of ['dialogAccept', 'dialogDismiss', 'recording:event', 'recording:isActive']) {
      ok(checkActorMessage('https://example.com/', n, {}));
    }
  });
  it('the canvas iframe gets nothing through its own actor', () => {
    for (const n of ['bridge:request', 'contentStore:set', 'contentStore:get', 'agent:subscribe', 'canvas:capturePng']) {
      no(checkActorMessage('about:srcdoc', n, req('agent:command', { command: 'daemon.info' })));
    }
  });
  it('only settings, brain and pack-install send daemon commands, each in its lane', () => {
    ok(checkActorMessage(SETTINGS, 'bridge:request', req('agent:command', { command: 'mcp.add' })));
    ok(checkActorMessage('nevoflux://brain', 'bridge:request', req('agent:command', { command: 'brain.list' })));
    no(checkActorMessage('nevoflux://brain', 'bridge:request', req('agent:command', { command: 'mcp.add' })));
    ok(checkActorMessage('nevoflux://pack-install', 'bridge:request', req('agent:command', { command: 'pack.install' })));
    no(checkActorMessage('nevoflux://pack-install', 'bridge:request', req('agent:command', { command: 'remote.unpair' })));
    no(checkActorMessage(CANVAS, 'bridge:request', req('agent:command', { command: 'daemon.info' })));
    no(checkActorMessage('nevoflux://home', 'bridge:request', req('agent:command', { command: 'session.list' })));
  });
  it('the settings page keeps every type it uses today', () => {
    for (const t of ['getCache', 'canvas.tool.save', 'canvas.tool.delete', 'canvas.tool.get_raw',
      'canvas.persist.list', 'canvas.persist.rename', 'canvas.persist.delete', 'canvas.tool.list', 'events.subscribe']) {
      ok(checkActorMessage(SETTINGS, 'bridge:request', req(t)));
    }
    ok(checkActorMessage(SETTINGS, 'contentStore:set', { key: 'config:settings', value: {} }));
    ok(checkActorMessage(SETTINGS, 'spaces:list', {}));
  });
  it('settings-only types are refused elsewhere', () => {
    for (const t of ['getCache', 'canvas.tool.save', 'canvas.persist.delete']) {
      no(checkActorMessage(CANVAS, 'bridge:request', req(t)));
    }
  });
  it('the canvas page writes only canvas:* and app:* keys', () => {
    ok(checkActorMessage(CANVAS, 'contentStore:set', { key: 'canvas:abc', value: {} }));
    ok(checkActorMessage(CANVAS, 'contentStore:set', { key: 'app:abc:notes', value: 1 }));
    no(checkActorMessage(CANVAS, 'contentStore:set', { key: 'config:settings.general.agentExecution', value: 'FullAuto' }));
    no(checkActorMessage(CANVAS, 'contentStore:delete', { key: 'session:x' }));
    no(checkActorMessage('nevoflux://home', 'contentStore:set', { key: 'config:x', value: 1 }));
    no(checkActorMessage(CANVAS, 'contentStore:set', { key: 42 }));
  });
  it('send_to_agent is held to what canvas.js and render.js send', () => {
    const sys = (command) => req('send_to_agent', { type: 'system_command', payload: { command } });
    ok(checkActorMessage(CANVAS, 'bridge:request', sys('content_store.load')));
    ok(checkActorMessage(CANVAS, 'bridge:request', sys('artifact.get')));
    no(checkActorMessage(CANVAS, 'bridge:request', sys('remote.agent_set_tier')));
    no(checkActorMessage(CANVAS, 'bridge:request', req('send_to_agent', { type: 'chat_message', payload: {} })));
    ok(checkActorMessage('nevoflux://render', 'bridge:request', req('send_to_agent', { type: 'canvas_video_render_done', payload: {} })));
    no(checkActorMessage('nevoflux://render', 'bridge:request', sys('artifact.get')));
  });
  it('canvas-only types come from the canvas page only; sidebar_send from nobody', () => {
    ok(checkActorMessage(CANVAS, 'bridge:request', req('canvas.command')));
    ok(checkActorMessage(CANVAS, 'bridge:request', req('canvas:notify')));
    no(checkActorMessage(SETTINGS, 'bridge:request', req('canvas.command')));
    no(checkActorMessage(CANVAS, 'bridge:request', req('sidebar_send')));
  });
  it('everything else a nevoflux page sends passes', () => {
    ok(checkActorMessage(CANVAS, 'bridge:request', req('exec_tool')));
    ok(checkActorMessage(CANVAS, 'bridge:request', req('agent:chat')));
    ok(checkActorMessage(CANVAS, 'canvas:capturePng', {}));
    ok(checkActorMessage('nevoflux://home', 'contentStore:query', { prefix: 'session:' }));
    no(checkActorMessage(CANVAS, 'bridge:request', req('')));
  });
});
