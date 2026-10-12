/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { readFileSync } from 'node:fs';
import {
  mayShowOsNotification,
  osNotificationContent,
} from '../../extensions/nevoflux-agent/background/notify-gate.mjs';

describe('mayShowOsNotification', () => {
  it('notify_user keeps its old rule: only when no window is focused', () => {
    expect(mayShowOsNotification('notify_user', false, true)).toBe(true);
    expect(mayShowOsNotification('notify_user', false, false)).toBe(true);
    expect(mayShowOsNotification('notify_user', true, true)).toBe(false);
    // A closed sidebar does not widen notify_user (spec §5).
    expect(mayShowOsNotification('notify_user', true, false)).toBe(false);
  });

  it('an approval outcome reaches the desktop when the sidebar is closed in the focused window', () => {
    expect(mayShowOsNotification('agent_approval', true, false)).toBe(true);
    expect(mayShowOsNotification('agent_approval', false, true)).toBe(true);
    expect(mayShowOsNotification('agent_approval', false, false)).toBe(true);
    // Focused window with the sidebar open: the sidebar toast is the one reminder.
    expect(mayShowOsNotification('agent_approval', true, true)).toBe(false);
  });

  it('other sources never reach the OS', () => {
    for (const source of [undefined, '', 'schedule', 'Agent_Approval']) {
      expect(mayShowOsNotification(source, false, false)).toBe(false);
    }
  });
});

describe('osNotificationContent', () => {
  it('passes notify_user text through as before', () => {
    expect(osNotificationContent({ source: 'notify_user', title: ' Tea ', body: ' Kettle is on ' })).toEqual({
      title: 'Tea',
      message: 'Kettle is on',
    });
    expect(osNotificationContent({ source: 'notify_user', message: 'legacy field' })).toEqual({
      title: 'NevoFlux',
      message: 'legacy field',
    });
  });

  it('an approval outcome with a body shows that body under its title', () => {
    const data = {
      event_id: 'e1',
      source: 'agent_approval',
      title: 'Muse approved',
      body: 'Click "Submit" on example.com',
      outcome: 'approved',
    };
    expect(osNotificationContent(data)).toEqual({
      title: 'Muse approved',
      message: 'Click "Submit" on example.com',
    });
  });

  it('an approval outcome without a body is still shown by its title', () => {
    expect(osNotificationContent({ source: 'agent_approval', title: 'Muse declined', body: '' })).toEqual({
      title: 'Muse declined',
      message: 'Muse declined',
    });
  });

  it('returns null for unknown sources and empty notify_user messages', () => {
    expect(osNotificationContent({ source: 'notify_user', title: 'x', body: '  ' })).toBeNull();
    expect(osNotificationContent({ source: 'agent_approval', body: '' })).toBeNull();
    expect(osNotificationContent({ source: 'other', title: 'x', body: 'y' })).toBeNull();
    expect(osNotificationContent(undefined)).toBeNull();
  });
});

describe('background.js handleUserNotification', () => {
  it('gates on the pure rule and asks the sidebar of the focused window', () => {
    const bg = readFileSync(
      new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
      'utf8'
    ).replace(/\r\n/g, '\n');
    expect(bg.includes("import { osNotificationContent, mayShowOsNotification } from './notify-gate.mjs';")).toBe(
      true
    );
    const start = bg.indexOf('async function handleUserNotification(event) {');
    expect(start).toBeGreaterThan(0);
    const fn = bg.slice(start, bg.indexOf('\n}\n', start));
    expect(fn.includes('const content = osNotificationContent(data);')).toBe(true);
    expect(fn.includes('browser.sidebarAction.isOpen({ windowId: focusedWindowId })')).toBe(true);
    expect(fn.includes('mayShowOsNotification(data.source, focusedWindowId !== null, sidebarOpen)')).toBe(true);
    expect(fn.includes("data.source !== 'notify_user'")).toBe(false);
    // Dedupe by event_id stays first: exactly one notification per approval.
    const dedupe = fn.indexOf('notifySeenEventIds.includes(eventId)');
    expect(dedupe).toBeGreaterThan(0);
    expect(dedupe < fn.indexOf('osNotificationContent(data)')).toBe(true);
  });
});
