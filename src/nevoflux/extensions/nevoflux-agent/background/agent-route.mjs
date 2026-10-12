/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Browser tool requests from a paired AI agent's channel (M2a §3.4).
 *
 * The daemon decides what an agent may do and which tab it acts on; it marks
 * those requests `origin: "agent"`. This module holds the extension's rules
 * for them, kept pure so Node can test them:
 *  - they run in the background script, not the sidebar (see background.js);
 *  - they never fall back to the active tab, because the active tab is the
 *    one the person is using;
 *  - the tab they act on must be showing a web page (or about:blank), never
 *    an internal, local-file or extension page.
 */

export const AGENT_ORIGIN = 'agent';

/** Whether a browser_tool_request payload came from an agent channel. */
export function isAgentRequest(request) {
  return request?.origin === AGENT_ORIGIN;
}

/** Agent actions that need no target tab. */
const AGENT_TABLESS_ACTIONS = new Set(['list_tabs', 'query_tabs']);

/**
 * Whether an agent request acts without a target tab: tab listing, or
 * navigating into a new tab.
 */
export function agentRequestIsTabless(request) {
  const action = request?.action;
  if (AGENT_TABLESS_ACTIONS.has(action)) {
    return true;
  }
  return action === 'navigate' && request.params?.new_tab === true;
}

/**
 * Why an agent request may not run as sent, or null when it may.
 *
 * Only one thing is checked: an agent request that names no tab. The daemon
 * always resolves one (the agent's own tab, or an explicit tab_id) except
 * for "navigate into a new tab" and tab listing. Anything else without a tab
 * would reach the active-tab fallback in executeBrowserTool and act on the
 * person's page, so it is refused instead. Only a positive integer counts
 * as a tab_id; 0, negatives, strings and fractions are treated as missing.
 */
export function agentTabRefusal(request) {
  if (!isAgentRequest(request)) {
    return null;
  }
  const tabId = request.tab_id;
  if (Number.isInteger(tabId) && tabId > 0) {
    return null;
  }
  if (agentRequestIsTabless(request)) {
    return null;
  }
  return {
    code: -1,
    message: `agent request '${request.action}' has no tab_id; refusing to act on the active tab`,
    recoverable: false,
  };
}

/**
 * Whether an agent may act on a tab showing `url`: http(s) pages and exactly
 * about:blank (where a new tab starts). Everything else (about: pages,
 * file:, extension and nevoflux: pages, javascript:/data: URLs, or no URL
 * at all) is off limits.
 */
export function agentTabUrlAllowed(url) {
  if (typeof url !== 'string') {
    return false;
  }
  if (url === 'about:blank') {
    return true;
  }
  const match = /^([a-z][a-z0-9+.-]*):/i.exec(url);
  if (!match) {
    return false;
  }
  const scheme = match[1].toLowerCase();
  return scheme === 'http' || scheme === 'https';
}

/**
 * Why an agent's activate_tab may not run as sent, or null when it may.
 * The tab the URL check looked at is request.tab_id; activating any other
 * tab named in params.tab_id would skip that check, so it is refused.
 */
export function agentActivateTabRefusal(request) {
  if (!isAgentRequest(request)) {
    return null;
  }
  const named = request.params?.tab_id;
  if (named === undefined || named === null || named === request.tab_id) {
    return null;
  }
  return {
    code: -1,
    message: "agent request 'activateTab' names a different tab than the one it was checked against",
    recoverable: false,
  };
}

/**
 * Whether navigate's new tab should be the active one. An agent's tab is
 * opened with `background: true` so the person's tab keeps focus (M2a §3.4,
 * Variant A, the default pending spike S1); every other caller keeps the
 * old behaviour.
 */
export function newTabActive(params) {
  return params?.background !== true;
}

/**
 * Why an agent may not act on `tab`, when it is in a private window, or null.
 * Private windows are the person's alone. The message must never read like
 * a closed tab, or the daemon would forget the tab instead of refusing.
 */
export function agentPrivateTabRefusal(action, tab) {
  if (tab?.incognito !== true) {
    return null;
  }
  return {
    code: -1,
    message: `agent request '${action}' targets a private-window tab; agents may not use private windows`,
    recoverable: false,
  };
}

function isAgentWindow(win) {
  return !!win && win.type === 'normal' && win.incognito !== true && win.id !== undefined;
}

/**
 * The window an agent's new tab opens in: the last-focused window when it is
 * a normal, non-private one, otherwise the first such window open, otherwise
 * null (only private windows are open).
 */
export function agentWindowId(lastFocused, windows) {
  if (isAgentWindow(lastFocused)) {
    return lastFocused.id;
  }
  const found = (Array.isArray(windows) ? windows : []).find(isAgentWindow);
  return found ? found.id : null;
}

/** The refusal when an agent's new tab has no non-private window to open in. */
export function agentNoWindowRefusal(action) {
  return {
    code: -1,
    message: `agent request '${action}' needs a normal browser window for its new tab, but only private windows are open; agents may not use private windows`,
    recoverable: false,
  };
}

/**
 * A tab listing without the tabs in private windows (by the listed windowId)
 * or marked incognito. Anything that is not a list passes through.
 */
export function withoutPrivateWindowTabs(tabs, windows) {
  if (!Array.isArray(tabs)) {
    return tabs;
  }
  const privateIds = new Set(
    (Array.isArray(windows) ? windows : []).filter((w) => w?.incognito === true).map((w) => w.id)
  );
  return tabs.filter((t) => t?.incognito !== true && !privateIds.has(t?.windowId));
}

/** Agent actions that only list tabs: listing every tab is not touching it. */
const AGENT_UNMARKED_ACTIONS = new Set(['list_tabs', 'query_tabs']);

/**
 * Which tabs a paired agent's call touched, for the tab badge (M2b §4.1):
 * the tab the call named, and the new tab browser_navigate opened. Nothing
 * for a failed call, and nothing for listing tabs (that would badge every
 * tab). Reading counts as touching (M2b B2). Positive integer ids only, each
 * once.
 */
export function agentTabsToMark(request, toolResult) {
  if (!isAgentRequest(request) || toolResult?.success !== true) {
    return [];
  }
  if (AGENT_UNMARKED_ACTIONS.has(request.action)) {
    return [];
  }
  const ids = [];
  const add = (id) => {
    if (Number.isInteger(id) && id > 0 && !ids.includes(id)) {
      ids.push(id);
    }
  };
  add(request.tab_id);
  if (request.action === 'navigate' && request.params?.new_tab === true) {
    add(toolResult.result?.tab_id);
  }
  return ids;
}
