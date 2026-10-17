/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A delegated task's browser calls follow the agent channel's tab rules
// (ruling BF-1): no active-tab fallback, never a private window, new tabs in
// the background. A local chat's calls keep the old rules.

import { describe, it, expect } from './test-runner.mjs';
import { readFileSync } from 'node:fs';
import {
  TAB_INDEPENDENT_ACTIONS,
  tabRulesFor,
  channelTabRefusal,
  channelChecksTab,
  channelTabUrlAllowed,
  newTabActive,
  agentTabRefusal,
} from '../../extensions/nevoflux-agent/background/agent-route.mjs';

const bg = readFileSync(
  new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
  'utf8'
).replace(/\r\n/g, '\n');

function executeBrowserToolSource() {
  const fnStart = bg.indexOf('async function executeBrowserTool(');
  const fnEnd = bg.indexOf('\n}\n', fnStart);
  return fnStart < 0 || fnEnd < 0 ? '' : bg.slice(fnStart, fnEnd);
}

const SESSION = 'sess-delegated';

describe('tabRulesFor', () => {
  it('an agent request follows the agent rules, delegated or not', () => {
    expect(tabRulesFor({ origin: 'agent', action: 'click' }, false)).toBe('agent');
    expect(tabRulesFor({ origin: 'agent', action: 'click' }, true)).toBe('agent');
  });

  it("a delegated session's request follows the delegated rules", () => {
    expect(tabRulesFor({ action: 'click', session_id: SESSION }, true)).toBe('delegated');
  });

  it('a local session keeps the local rules', () => {
    expect(tabRulesFor({ action: 'click', session_id: 'local' }, false)).toBe(null);
    expect(tabRulesFor({ action: 'click' }, undefined)).toBe(null);
    expect(tabRulesFor({ action: 'click' }, 'yes')).toBe(null);
  });
});

describe('channelTabRefusal (delegated)', () => {
  it('refuses a tab-less click with the message agents get', () => {
    const request = { action: 'click', session_id: SESSION, params: { selector: '#a' } };
    const refusal = channelTabRefusal(request, 'delegated');
    expect(refusal).toEqual({
      code: -1,
      message: "agent request 'click' has no tab_id; refusing to act on the active tab",
      recoverable: false,
    });
    expect(refusal.message).toBe(agentTabRefusal({ ...request, origin: 'agent' }).message);
  });

  it('treats a non-positive or non-integer tab_id as missing', () => {
    for (const tab_id of [0, -3, 1.5, '7', null, undefined]) {
      expect(channelTabRefusal({ action: 'screenshot', tab_id }, 'delegated')).not.toBe(null);
    }
  });

  it('lets a call that names its tab run', () => {
    expect(channelTabRefusal({ action: 'click', tab_id: 12 }, 'delegated')).toBe(null);
  });

  it('lets the tab-less actions agents may run, and navigate into a new tab, run', () => {
    expect(channelTabRefusal({ action: 'list_tabs' }, 'delegated')).toBe(null);
    expect(channelTabRefusal({ action: 'query_tabs' }, 'delegated')).toBe(null);
    const nav = { action: 'navigate', params: { url: 'https://example.com', new_tab: true } };
    expect(channelTabRefusal(nav, 'delegated')).toBe(null);
  });

  it('refuses navigating in place without a tab', () => {
    const nav = { action: 'navigate', params: { url: 'https://example.com' } };
    expect(channelTabRefusal(nav, 'delegated').message).toBe(
      "agent request 'navigate' has no tab_id; refusing to act on the active tab"
    );
  });

  it('keeps Canvas and the other tab-independent actions working', () => {
    for (const action of ['canvas_render', 'canvas_eval', 'read_artifact', 'edit_artifact', 'ask_user', 'web_fetch', 'web_search', 'cache_file']) {
      expect(TAB_INDEPENDENT_ACTIONS.has(action)).toBe(true);
      expect(channelTabRefusal({ action, session_id: SESSION }, 'delegated')).toBe(null);
    }
  });

  it('refuses actions that would use the active tab (recording, get_content)', () => {
    expect(channelTabRefusal({ action: 'recording_start' }, 'delegated')).not.toBe(null);
    expect(channelTabRefusal({ action: 'get_content' }, 'delegated')).not.toBe(null);
  });

  it('a local session is never refused here', () => {
    expect(channelTabRefusal({ action: 'click' }, null)).toBe(null);
  });

  it('agent requests are decided by agentTabRefusal, unchanged', () => {
    const request = { origin: 'agent', action: 'canvas_render' };
    expect(channelTabRefusal(request, 'agent')).toEqual(agentTabRefusal(request));
  });
});

describe('channelChecksTab', () => {
  it('a delegated call that names a tab has that tab checked', () => {
    expect(channelChecksTab({ action: 'click', tab_id: 4 }, 'delegated')).toBe(true);
  });

  it('a delegated tab-less call (Canvas, new tab, listing) checks no tab', () => {
    expect(channelChecksTab({ action: 'canvas_render' }, 'delegated')).toBe(false);
    expect(channelChecksTab({ action: 'list_tabs' }, 'delegated')).toBe(false);
    const nav = { action: 'navigate', tab_id: 4, params: { url: 'https://a.b', new_tab: true } };
    expect(channelChecksTab(nav, 'delegated')).toBe(false);
  });

  it('agent calls are checked as before; local calls never', () => {
    expect(channelChecksTab({ origin: 'agent', action: 'click', tab_id: 4 }, 'agent')).toBe(true);
    expect(channelChecksTab({ origin: 'agent', action: 'list_tabs' }, 'agent')).toBe(false);
    expect(channelChecksTab({ action: 'click', tab_id: 4 }, null)).toBe(false);
  });
});

describe('channelTabUrlAllowed', () => {
  it('web pages and about:blank are allowed for both channels', () => {
    for (const rules of ['agent', 'delegated']) {
      expect(channelTabUrlAllowed({ id: 3, url: 'https://example.com/' }, rules, null)).toBe(true);
      expect(channelTabUrlAllowed({ id: 3, url: 'about:blank' }, rules, null)).toBe(true);
    }
  });

  it("a delegated task may act on its own session's Canvas tab", () => {
    expect(channelTabUrlAllowed({ id: 9, url: 'nevoflux://canvas/abc' }, 'delegated', 9)).toBe(true);
  });

  it("but not on another Canvas tab, nor its own tab once it left the Canvas", () => {
    expect(channelTabUrlAllowed({ id: 8, url: 'nevoflux://canvas/abc' }, 'delegated', 9)).toBe(false);
    expect(channelTabUrlAllowed({ id: 9, url: 'about:preferences' }, 'delegated', 9)).toBe(false);
    expect(channelTabUrlAllowed({ id: 9, url: 'nevoflux://settings' }, 'delegated', 9)).toBe(false);
    expect(channelTabUrlAllowed({ id: 9, url: 'nevoflux://canvas/abc' }, 'delegated', null)).toBe(false);
  });

  it('an agent never gets the Canvas exception', () => {
    expect(channelTabUrlAllowed({ id: 9, url: 'nevoflux://canvas/abc' }, 'agent', 9)).toBe(false);
  });

  it('internal pages stay off limits', () => {
    expect(channelTabUrlAllowed({ id: 3, url: 'about:config' }, 'delegated', null)).toBe(false);
    expect(channelTabUrlAllowed({ id: 3, url: 'file:///c:/x' }, 'delegated', null)).toBe(false);
    expect(channelTabUrlAllowed(undefined, 'delegated', null)).toBe(false);
  });
});

describe('newTabActive with delegated rules', () => {
  it("a delegated task's new tab always opens in the background", () => {
    expect(newTabActive({ url: 'https://a.b', new_tab: true }, 'delegated')).toBe(false);
    expect(newTabActive({ url: 'https://a.b', new_tab: true, background: false }, 'delegated')).toBe(
      false
    );
  });

  it('local and agent tabs keep their old behaviour', () => {
    expect(newTabActive({ url: 'https://a.b', new_tab: true }, null)).toBe(true);
    expect(newTabActive({ url: 'https://a.b', new_tab: true })).toBe(true);
    expect(newTabActive({ url: 'https://a.b', new_tab: true, background: true }, 'agent')).toBe(false);
    expect(newTabActive({ url: 'https://a.b', new_tab: true }, 'agent')).toBe(true);
  });
});

describe('background.js applies the delegated tab rules', () => {
  it('decides the rules from the session, except for Canvas panel bridge calls', () => {
    const fn = executeBrowserToolSource();
    expect(
      fn.includes(
        "const rules = tabRulesFor(request, caller !== 'bridge' && isDelegatedSession(request.session_id));"
      )
    ).toBe(true);
    expect(fn.includes('channelTabRefusal(request, rules)')).toBe(true);
  });

  it('skips the active-tab fallback for agent and delegated calls', () => {
    const head = executeBrowserToolSource().split('switch (action)')[0];
    expect(head.includes('if (!targetTabId && !TAB_INDEPENDENT_ACTIONS.has(action) && !rules) {')).toBe(
      true
    );
  });

  it("opens a delegated new tab in a non-private window via agentWindowId, in the background", () => {
    const fn = executeBrowserToolSource();
    const nav = fn.slice(fn.indexOf("case 'navigate'"), fn.indexOf("case 'activateTab'"));
    expect(nav.includes('if (rules && params?.new_tab === true) {')).toBe(true);
    expect(nav.includes('agentWindowId(lastFocused, windows)')).toBe(true);
    expect(nav.includes('executeNavigateViaApi(targetTabId, params, newTabWindowId, rules)')).toBe(true);
    expect(bg.includes('const createProps = { url, active: newTabActive(params, rules) };')).toBe(true);
  });

  it("checks a delegated call's tab: private refusal and the URL gate with its own Canvas tab", () => {
    const head = executeBrowserToolSource().split('switch (action)')[0];
    expect(head.includes('if (channelChecksTab(request, rules)) {')).toBe(true);
    expect(head.includes('agentPrivateTabRefusal(action, agentTab)')).toBe(true);
    expect(
      head.includes('channelTabUrlAllowed(agentTab, rules, canvasTabs.tabFor(request.session_id))')
    ).toBe(true);
  });

  it('lists only non-private tabs to a delegated task', () => {
    const fn = executeBrowserToolSource();
    const list = fn.slice(fn.indexOf("case 'list_tabs'"), fn.indexOf('// Alias: get_elements'));
    expect((list.match(/if \(rules\) \{/g) || []).length).toBe(2);
  });
});
