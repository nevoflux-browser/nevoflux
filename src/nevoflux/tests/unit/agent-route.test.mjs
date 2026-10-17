/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { readFileSync } from 'node:fs';
import {
  AGENT_ORIGIN,
  isAgentRequest,
  agentTabRefusal,
  agentRequestIsTabless,
  agentTabUrlAllowed,
  agentActivateTabRefusal,
  newTabActive,
  agentPrivateTabRefusal,
  agentWindowId,
  agentNoWindowRefusal,
  withoutPrivateWindowTabs,
} from '../../extensions/nevoflux-agent/background/agent-route.mjs';

// Normalised so the source-level checks below do not depend on checkout line endings.
const bg = readFileSync(
  new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
  'utf8'
).replace(/\r\n/g, '\n');

// The wire actions behind the agent channel's 16 tools (M2a §2.5), as the
// daemon's BrowserToolAction serialises them. browser_upload_file is
// orchestrated by the daemon and reaches the extension as `uploadFile`.
const AGENT_CHANNEL_ACTIONS = [
  'navigate',
  'snapshot',
  'get_markdown',
  'screenshot',
  'list_tabs',
  'wait_for',
  'click_by_id',
  'fill_by_id',
  'type_by_id',
  'click',
  'fill',
  'type',
  'key_press',
  'scroll',
  'activateTab',
  'uploadFile',
];

// executeBrowserTool's own source, from its signature to its closing brace
// (the first `}` at column 0). Searching the whole file would also match the
// other switches in background.js.
function executeBrowserToolSource() {
  const fnStart = bg.indexOf('async function executeBrowserTool(');
  const fnEnd = bg.indexOf('\n}\n', fnStart);
  return fnStart < 0 || fnEnd < 0 ? '' : bg.slice(fnStart, fnEnd);
}

describe('isAgentRequest', () => {
  it('is true only for origin "agent"', () => {
    expect(AGENT_ORIGIN).toBe('agent');
    expect(isAgentRequest({ origin: 'agent' })).toBe(true);
    expect(isAgentRequest({})).toBe(false);
    expect(isAgentRequest({ origin: 'Agent' })).toBe(false);
    expect(isAgentRequest({ origin: null })).toBe(false);
    expect(isAgentRequest(undefined)).toBe(false);
  });
});

describe('agentTabRefusal', () => {
  it('lets an agent request that names its tab through', () => {
    expect(
      agentTabRefusal({ origin: 'agent', action: 'click_by_id', tab_id: 42, params: {} })
    ).toBeNull();
  });

  it('leaves native-chat requests alone, active-tab fallback included', () => {
    expect(agentTabRefusal({ action: 'snapshot', tab_id: null, params: {} })).toBeNull();
    expect(agentTabRefusal({ action: 'click', params: {} })).toBeNull();
  });

  it('lets an agent navigate into a new tab without a tab_id', () => {
    const req = {
      origin: 'agent',
      action: 'navigate',
      tab_id: null,
      params: { url: 'https://example.com', new_tab: true, background: true },
    };
    expect(agentTabRefusal(req)).toBeNull();
  });

  it('lets an agent list tabs without a tab_id', () => {
    expect(
      agentTabRefusal({ origin: 'agent', action: 'list_tabs', tab_id: null, params: {} })
    ).toBeNull();
    expect(
      agentTabRefusal({ origin: 'agent', action: 'query_tabs', tab_id: null, params: {} })
    ).toBeNull();
  });

  it('refuses any other agent request without a tab_id rather than using the active tab', () => {
    for (const action of AGENT_CHANNEL_ACTIONS.filter((a) => a !== 'list_tabs')) {
      const refusal = agentTabRefusal({ origin: 'agent', action, tab_id: null, params: {} });
      expect(refusal === null).toBe(false);
    }
    // navigate without new_tab would navigate the person's tab in place
    const inPlace = agentTabRefusal({
      origin: 'agent',
      action: 'navigate',
      params: { url: 'https://example.com' },
    });
    expect(inPlace === null).toBe(false);
  });

  it('treats only a positive integer as a tab_id', () => {
    for (const tab_id of [null, undefined, 0, -1, '7', 1.5]) {
      const refusal = agentTabRefusal({ origin: 'agent', action: 'click', tab_id, params: {} });
      expect(refusal === null).toBe(false);
      expect(refusal.message).toBe(
        "agent request 'click' has no tab_id; refusing to act on the active tab"
      );
    }
    // The exemptions still hold whatever the tab_id looks like.
    for (const tab_id of [null, undefined, 0, -1, '7', 1.5]) {
      expect(
        agentTabRefusal({ origin: 'agent', action: 'list_tabs', tab_id, params: {} })
      ).toBeNull();
      expect(
        agentTabRefusal({
          origin: 'agent',
          action: 'navigate',
          tab_id,
          params: { url: 'https://example.com', new_tab: true },
        })
      ).toBeNull();
    }
  });

  it('refuses in a shape the daemon reports as a tool failure', () => {
    const refusal = agentTabRefusal({ origin: 'agent', action: 'fill', params: {} });
    expect(refusal.code).toBe(-1);
    expect(refusal.recoverable).toBe(false);
    expect(refusal.message).toBe(
      "agent request 'fill' has no tab_id; refusing to act on the active tab"
    );
  });
});

describe('agentRequestIsTabless', () => {
  it('is true for tab listing and navigate into a new tab (new_tab strictly true)', () => {
    expect(agentRequestIsTabless({ action: 'list_tabs' })).toBe(true);
    expect(agentRequestIsTabless({ action: 'query_tabs' })).toBe(true);
    expect(agentRequestIsTabless({ action: 'navigate', params: { new_tab: true } })).toBe(true);
  });

  it('is false for everything else, including truthy-but-not-true new_tab', () => {
    expect(agentRequestIsTabless({ action: 'click', params: {} })).toBe(false);
    expect(agentRequestIsTabless({ action: 'navigate', params: { url: 'https://a.test' } })).toBe(
      false
    );
    for (const new_tab of ['true', 1, 'yes', {}, false, null]) {
      expect(agentRequestIsTabless({ action: 'navigate', params: { new_tab } })).toBe(false);
    }
    expect(agentRequestIsTabless({ action: 'navigate' })).toBe(false);
    expect(agentRequestIsTabless(undefined)).toBe(false);
  });

  it('a truthy-but-not-true new_tab without a tab_id is refused', () => {
    for (const new_tab of ['true', 1]) {
      const refusal = agentTabRefusal({
        origin: 'agent',
        action: 'navigate',
        tab_id: null,
        params: { url: 'https://example.com', new_tab },
      });
      expect(refusal === null).toBe(false);
    }
  });
});

describe('agentActivateTabRefusal', () => {
  it('allows an activate with no params.tab_id or one equal to request.tab_id', () => {
    expect(agentActivateTabRefusal({ origin: 'agent', tab_id: 5, params: {} })).toBeNull();
    expect(agentActivateTabRefusal({ origin: 'agent', tab_id: 5 })).toBeNull();
    expect(agentActivateTabRefusal({ origin: 'agent', tab_id: 5, params: { tab_id: 5 } })).toBeNull();
  });

  it('refuses a params.tab_id that differs from the checked tab', () => {
    const r = agentActivateTabRefusal({ origin: 'agent', tab_id: 5, params: { tab_id: 9 } });
    expect(r === null).toBe(false);
    expect(r.code).toBe(-1);
    expect(r.recoverable).toBe(false);
    expect(agentActivateTabRefusal({ origin: 'agent', tab_id: 5, params: { tab_id: '5' } }) === null).toBe(false);
  });

  it('never applies to native requests', () => {
    expect(agentActivateTabRefusal({ tab_id: 5, params: { tab_id: 9 } })).toBeNull();
  });

  it('activateTab acts on the checked tab for agent requests', () => {
    const start = bg.indexOf("case 'activateTab':");
    const branch = bg.slice(start, bg.indexOf("case 'go_back':", start));
    // `rules` is set for agent (and delegated) requests.
    expect(branch.includes('if (rules) {')).toBe(true);
    expect(branch.includes('agentActivateTabRefusal(request)')).toBe(true);
    expect(branch.includes('executeActivateTabViaApi(targetTabId, {})')).toBe(true);
    expect(branch.includes('executeActivateTabViaApi(targetTabId, params)')).toBe(true);
  });
});

describe('newTabActive', () => {
  it('a background agent tab is created inactive', () => {
    expect(newTabActive({ url: 'https://example.com', new_tab: true, background: true })).toBe(
      false
    );
  });

  it('every other new tab stays active, as before', () => {
    expect(newTabActive({ url: 'https://example.com', new_tab: true })).toBe(true);
    expect(newTabActive({ url: 'https://example.com', new_tab: true, background: false })).toBe(
      true
    );
    expect(newTabActive({ url: 'https://example.com', new_tab: true, background: 'yes' })).toBe(
      true
    );
    expect(newTabActive(undefined)).toBe(true);
  });

  it('executeNavigateViaApi creates new tabs through it', () => {
    const fnStart = bg.indexOf('async function executeNavigateViaApi(');
    const fn = bg.slice(fnStart, bg.indexOf('\n}\n', fnStart));
    expect(fn.includes('const createProps = { url, active: newTabActive(params, rules) };')).toBe(true);
    expect(fn.includes('browser.tabs.create(createProps)')).toBe(true);
    expect(fn.includes('browser.tabs.create({ url, active: true })')).toBe(false);
  });
});

describe('agentTabUrlAllowed', () => {
  it('allows web pages and the blank page', () => {
    expect(agentTabUrlAllowed('http://example.com/')).toBe(true);
    expect(agentTabUrlAllowed('HTTPS://example.com/a?b=c')).toBe(true);
    expect(agentTabUrlAllowed('about:blank')).toBe(true);
  });

  it('refuses internal, local and script pages', () => {
    for (const url of [
      'about:config',
      'file:///C:/Users/me/secret.txt',
      'nevoflux://canvas/abc',
      'moz-extension://1234/sidebar.html',
      'javascript:alert(1)',
      'data:text/html,<p>hi</p>',
      '',
      undefined,
    ]) {
      expect(agentTabUrlAllowed(url)).toBe(false);
    }
  });
});

// The daemon turns these into tab_closed (agent_backend.rs is_tab_gone); a
// private-window refusal must not be mistaken for a closed tab.
function looksTabGone(message) {
  const m = message.toLowerCase();
  return (
    m.startsWith('3001:') ||
    m.includes('tab not found') ||
    m.includes('invalid tab id') ||
    m.includes('no tab with id')
  );
}

describe('agentPrivateTabRefusal', () => {
  it('refuses a tab in a private window, naming the action', () => {
    const r = agentPrivateTabRefusal('click', { id: 7, incognito: true });
    expect(r.code).toBe(-1);
    expect(r.recoverable).toBe(false);
    expect(r.message).toBe(
      "agent request 'click' targets a private-window tab; agents may not use private windows"
    );
    expect(looksTabGone(r.message)).toBe(false);
  });

  it('lets a normal tab through', () => {
    expect(agentPrivateTabRefusal('click', { id: 7, incognito: false })).toBe(null);
    expect(agentPrivateTabRefusal('click', { id: 7 })).toBe(null);
    expect(agentPrivateTabRefusal('click', undefined)).toBe(null);
  });
});

describe('agentWindowId', () => {
  const normal = (id, incognito = false) => ({ id, type: 'normal', incognito });

  it('uses the last-focused window when it is a normal, non-private one', () => {
    expect(agentWindowId(normal(2), [normal(1), normal(2)])).toBe(2);
  });

  it('skips a private last-focused window for a non-private one', () => {
    expect(agentWindowId(normal(3, true), [normal(3, true), normal(1)])).toBe(1);
  });

  it('skips non-normal and private windows in the list', () => {
    const popup = { id: 4, type: 'popup', incognito: false };
    expect(agentWindowId(popup, [popup, normal(5, true), normal(6)])).toBe(6);
  });

  it('has nothing when only private windows are open', () => {
    expect(agentWindowId(normal(3, true), [normal(3, true)])).toBe(null);
    expect(agentWindowId(undefined, [])).toBe(null);
    expect(agentWindowId(null, undefined)).toBe(null);
  });
});

describe('agentNoWindowRefusal', () => {
  it('says why, is final, and is not a closed tab', () => {
    const r = agentNoWindowRefusal('navigate');
    expect(r.code).toBe(-1);
    expect(r.recoverable).toBe(false);
    expect(r.message.includes("agent request 'navigate'")).toBe(true);
    expect(r.message.includes('private windows')).toBe(true);
    expect(looksTabGone(r.message)).toBe(false);
  });
});

describe('withoutPrivateWindowTabs', () => {
  const windows = [
    { id: 1, incognito: false },
    { id: 2, incognito: true },
  ];

  it('drops tabs in private windows and tabs marked incognito', () => {
    const tabs = [
      { id: 10, windowId: 1 },
      { id: 11, windowId: 2 },
      { id: 12, windowId: 1, incognito: true },
      { id: 13, windowId: 9 },
    ];
    expect(withoutPrivateWindowTabs(tabs, windows).map((t) => t.id)).toEqual([10, 13]);
  });

  it('passes through anything that is not a list', () => {
    expect(withoutPrivateWindowTabs(undefined, windows)).toBe(undefined);
    expect(withoutPrivateWindowTabs([{ id: 1, windowId: 2 }], undefined)).toEqual([
      { id: 1, windowId: 2 },
    ]);
  });
});

describe('background.js keeps agents out of private windows', () => {
  it('refuses an agent request whose target tab is private', () => {
    const fn = executeBrowserToolSource();
    const head = fn.slice(0, fn.indexOf('switch (action)'));
    const get = head.indexOf('agentTab = await browser.tabs.get(targetTabId)');
    const check = head.indexOf('agentPrivateTabRefusal(action, agentTab)');
    expect(get).toBeGreaterThan(0);
    expect(check).toBeGreaterThan(get);
  });

  it("opens an agent's new tab in a non-private normal window, or refuses", () => {
    const fn = executeBrowserToolSource();
    const nav = fn.slice(fn.indexOf("case 'navigate'"), fn.indexOf("case 'activateTab'"));
    expect(nav.includes("browser.windows.getAll({ windowTypes: ['normal'] })")).toBe(true);
    expect(nav.includes('browser.windows.getLastFocused()')).toBe(true);
    expect(nav.includes('agentWindowId(')).toBe(true);
    expect(nav.includes('agentNoWindowRefusal(action)')).toBe(true);
    expect(bg.includes('const tab = await browser.tabs.create(createProps);')).toBe(true);
    expect(bg.includes('createProps.windowId = windowId;')).toBe(true);
  });

  it('lists only non-private tabs to an agent', () => {
    const fn = executeBrowserToolSource();
    const list = fn.slice(fn.indexOf("case 'list_tabs'"), fn.indexOf('// Alias: get_elements'));
    expect(list.includes('if (rules) {')).toBe(true);
    // Both list_tabs and query_tabs go through agentListing for agents.
    expect((list.match(/agentListing\(/g) || []).length).toBe(2);
    const start = bg.indexOf('async function agentListing(');
    const helper = bg.slice(start, bg.indexOf('\n}\n', start));
    expect(start).toBeGreaterThan(0);
    expect(helper.includes('agentWindowId(lastFocused, windows)')).toBe(true);
    expect(helper.includes('withoutPrivateWindowTabs(listed.result.tabs, windows)')).toBe(true);
    expect(bg.includes('browser.nevoflux.listTabs(windowId ?? undefined, scope, scopeId)')).toBe(true);
  });
});

describe('background.js routing of agent requests', () => {
  it('runs agent requests in the background script, never via the sidebar', () => {
    const start = bg.indexOf('if (DIRECT_ACTIONS.has(action)');
    expect(start).toBeGreaterThan(0);
    const cond = bg.slice(start, bg.indexOf('{', start));
    expect(cond.includes('isAgentRequest(payload)')).toBe(true);
    const branchEnd = bg.indexOf('broadcastToSidebar(', start);
    const branch = bg.slice(start, branchEnd);
    expect(branch.includes("executeBrowserTool(payload, 'direct')")).toBe(true);
    // After the response is sent the branch returns before the catch-all
    // broadcast, so the sidebar never sees (or re-executes) the request.
    const afterCatch = branch.slice(branch.indexOf('.catch((err) =>'));
    expect(/\n\s*return;\s*\n/.test(afterCatch)).toBe(true);
    expect(bg.includes('import { isAgentRequest, agentTabRefusal')).toBe(true);
  });

  it('agent requests skip the active-tab fallback', () => {
    const fn = executeBrowserToolSource();
    expect(fn.length).toBeGreaterThan(0);
    const head = fn.slice(0, fn.indexOf('switch (action)'));
    expect(head.includes('channelTabRefusal(request, rules)')).toBe(true);
    // `rules` is 'agent' for every agent request (tabRulesFor).
    expect(head.includes('tabRulesFor(request,')).toBe(true);
    expect(
      head.includes('if (!targetTabId && !TAB_INDEPENDENT_ACTIONS.has(action) && !rules) {')
    ).toBe(true);
  });

  it("checks the agent's target tab is showing a page it may use", () => {
    const fn = executeBrowserToolSource();
    const head = fn.slice(0, fn.indexOf('switch (action)'));
    const fallback = head.indexOf('!TAB_INDEPENDENT_ACTIONS.has(action) && !rules) {');
    const check = head.indexOf('channelTabUrlAllowed(');
    expect(fallback).toBeGreaterThan(0);
    // After the fallback block, so it sees the tab the action will land on.
    expect(check).toBeGreaterThan(fallback);
    expect(head.includes('browser.tabs.get(targetTabId)')).toBe(true);
    expect(head.includes('targets a tab showing a page the agent may not use')).toBe(true);
    // An agent stuck on an internal page is told how to get out.
    expect(head.includes('; open a new tab with browser_navigate new_tab:true')).toBe(true);
    expect(head.includes('if (channelChecksTab(request, rules)) {')).toBe(true);
  });

  it('every action the agent channel sends has a case in executeBrowserTool', () => {
    const fn = executeBrowserToolSource();
    expect(fn.length).toBeGreaterThan(0);
    for (const action of AGENT_CHANNEL_ACTIONS) {
      expect(fn.includes(`case '${action}'`)).toBe(true);
    }
  });
});
