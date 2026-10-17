/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from './test-runner.mjs';
import { readFileSync } from 'node:fs';
import { agentTabsToMark } from '../../extensions/nevoflux-agent/background/agent-route.mjs';

// Source files, normalised so checks do not depend on checkout line endings.
const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

const ok = (result = {}) => ({ success: true, result });

describe('agentTabsToMark', () => {
  it('marks the tab an agent call named', () => {
    expect(
      agentTabsToMark(
        { origin: 'agent', action: 'click_by_id', tab_id: 42, params: { element_id: 'e3' } },
        ok({ clicked: true })
      )
    ).toEqual([42]);
    // Reading counts: B2 marks what the agent touched, read or write.
    expect(
      agentTabsToMark({ origin: 'agent', action: 'get_markdown', tab_id: 7, params: {} }, ok({ markdown: '' }))
    ).toEqual([7]);
  });

  it('marks the new tab browser_navigate opened', () => {
    const req = {
      origin: 'agent',
      action: 'navigate',
      tab_id: null,
      params: { url: 'https://example.com', new_tab: true, background: true },
    };
    expect(agentTabsToMark(req, ok({ url: 'https://example.com', tab_id: 314, new_tab: true }))).toEqual([314]);
    // Navigating a named tab in place marks that tab once.
    const inPlace = { origin: 'agent', action: 'navigate', tab_id: 9, params: { url: 'https://example.com' } };
    expect(agentTabsToMark(inPlace, ok({ url: 'https://example.com', tab_id: 9, new_tab: false }))).toEqual([9]);
  });

  it('get_tabs marks nothing even with a tab_id', () => {
    expect(
      agentTabsToMark({ origin: 'agent', action: 'list_tabs', tab_id: null, params: {} }, ok([{ id: 1 }, { id: 2 }]))
    ).toEqual([]);
    expect(agentTabsToMark({ origin: 'agent', action: 'list_tabs', tab_id: 5, params: {} }, ok([]))).toEqual([]);
    expect(agentTabsToMark({ origin: 'agent', action: 'query_tabs', tab_id: 5, params: {} }, ok([]))).toEqual([]);
  });

  it('a failed call marks nothing', () => {
    const req = { origin: 'agent', action: 'fill', tab_id: 42, params: {} };
    expect(agentTabsToMark(req, { success: false, error: { code: -1, message: 'x' } })).toEqual([]);
    expect(agentTabsToMark(req, undefined)).toEqual([]);
    const nav = { origin: 'agent', action: 'navigate', tab_id: null, params: { url: 'https://e.com', new_tab: true } };
    expect(agentTabsToMark(nav, { success: false, result: { tab_id: 314 } })).toEqual([]);
  });

  it('native-chat calls mark nothing', () => {
    expect(agentTabsToMark({ action: 'click', tab_id: 42, params: {} }, ok())).toEqual([]);
    expect(agentTabsToMark({ origin: 'Agent', action: 'click', tab_id: 42, params: {} }, ok())).toEqual([]);
  });

  it('ignores tab ids that are not positive integers and never repeats one', () => {
    for (const bad of [0, -3, 1.5, '42', null, undefined]) {
      expect(agentTabsToMark({ origin: 'agent', action: 'scroll', tab_id: bad, params: {} }, ok())).toEqual([]);
    }
    const nav = { origin: 'agent', action: 'navigate', tab_id: null, params: { url: 'https://e.com', new_tab: true } };
    expect(agentTabsToMark(nav, ok({ tab_id: '314' }))).toEqual([]);
    const both = { origin: 'agent', action: 'navigate', tab_id: 314, params: { url: 'https://e.com', new_tab: true } };
    expect(agentTabsToMark(both, ok({ tab_id: 314 }))).toEqual([314]);
  });
});

const PARENT = '../../engine-overlays/browser/components/extensions/parent/ext-nevoflux.js';
const SCHEMA = '../../engine-overlays/browser/components/extensions/schemas/nevoflux.json';

// The real method out of ext-nevoflux.js, run against a fake `extension`.
// The file is a privileged parent-process script Node cannot load whole, but
// this one method only touches `extension` and the tab element, so running
// its own text tests the behaviour, not just that a name appears.
function loadSetTabAgentMark(extension) {
  const src = read(PARENT);
  const start = src.indexOf('        async setTabAgentMark(tabId, on) {');
  if (start < 0) {
    throw new Error('setTabAgentMark not found in ext-nevoflux.js');
  }
  const end = src.indexOf('\n        },\n', start);
  const method = src.slice(start, end + '\n        }'.length);
  // eslint-disable-next-line no-new-func
  return new Function('extension', `return ({\n${method}\n}).setTabAgentMark;`)(extension);
}

function fakeTab() {
  const attrs = new Set();
  return {
    attrs,
    closing: false,
    isConnected: true,
    toggleAttribute(name, force) {
      if (force) {
        attrs.add(name);
      } else {
        attrs.delete(name);
      }
      return force;
    },
  };
}

function fakeExtension(tabs) {
  return {
    tabManager: {
      get(id) {
        if (!tabs.has(id)) {
          throw new Error(`Invalid tab ID: ${id}`);
        }
        return { nativeTab: tabs.get(id) };
      },
    },
  };
}

describe('setTabAgentMark experiment API', () => {
  it('the schema declares setTabAgentMark(tabId: integer, on: boolean)', () => {
    const ns = JSON.parse(read(SCHEMA)).find((n) => n.namespace === 'nevoflux');
    const fn = ns.functions.find((f) => f.name === 'setTabAgentMark');
    expect(fn === undefined).toBe(false);
    expect(fn.async).toBe('promise');
    expect(fn.parameters.map((p) => `${p.name}:${p.type}:${p.optional === true}`)).toEqual([
      'tabId:integer:false',
      'on:boolean:false',
    ]);
  });

  it('turns the attribute on and off on the native tab', async () => {
    const tab = fakeTab();
    const set = loadSetTabAgentMark(fakeExtension(new Map([[7, tab]])));
    expect(await set(7, true)).toEqual({ success: true });
    expect(tab.attrs.has('nevoflux-agent-touched')).toBe(true);
    await set(7, false);
    expect(tab.attrs.has('nevoflux-agent-touched')).toBe(false);
  });

  it('an unknown or closed tab is a no-op, not an error', async () => {
    const closing = fakeTab();
    closing.closing = true;
    const detached = fakeTab();
    detached.isConnected = false;
    const set = loadSetTabAgentMark(
      fakeExtension(
        new Map([
          [8, closing],
          [9, detached],
        ])
      )
    );
    expect(await set(99, true)).toEqual({ success: true });
    expect(await set(8, true)).toEqual({ success: true });
    expect(await set(9, true)).toEqual({ success: true });
    expect(closing.attrs.size + detached.attrs.size).toBe(0);
  });
});

describe('background.js marks agent tabs', () => {
  const bg = read('../../extensions/nevoflux-agent/background/background.js');

  it('marking runs after a direct agent call settles and before the answer is sent', () => {
    const start = bg.indexOf("executeBrowserTool(payload, 'direct')");
    expect(start).toBeGreaterThan(0);
    const thenBlock = bg.slice(start, bg.indexOf('.catch((err) =>', start));
    const resolved = thenBlock.indexOf('if (toolResult?.resolvedElsewhere) return;');
    const mark = thenBlock.indexOf('markAgentTabs(payload, toolResult);');
    const send = thenBlock.indexOf('channelManager.sendToAgent({');
    expect(resolved).toBeGreaterThan(0);
    expect(mark).toBeGreaterThan(resolved);
    expect(send).toBeGreaterThan(mark);
    expect(bg.includes('withoutPrivateWindowTabs, agentTabsToMark,')).toBe(true);
  });

  it('marking cannot throw into the answer', () => {
    const fnStart = bg.indexOf('function markAgentTabs(request, toolResult) {');
    expect(fnStart).toBeGreaterThan(0);
    const fn = bg.slice(fnStart, bg.indexOf('\n}\n', fnStart));
    expect(fn.includes("typeof browser.nevoflux?.setTabAgentMark !== 'function'")).toBe(true);
    expect(fn.includes('agentTabsToMark(request, toolResult)')).toBe(true);
    expect(fn.includes('.setTabAgentMark(tabId, true)')).toBe(true);
    expect(fn.includes('.catch((err) =>')).toBe(true);
    expect(fn.includes('try {')).toBe(true);
    expect(fn.includes('await ')).toBe(false);
  });
});

describe('agent tab badge stylesheet', () => {
  it('is registered in the jar and linked into the browser window', () => {
    const jar = read('../../overlays/common/jar.inc.mn');
    expect(
      jar.includes(
        'content/browser/zen-styles/nevoflux-agent-touched.css                  (../../zen/common/styles/nevoflux-agent-touched.css)'
      )
    ).toBe(true);
    const assets = read('../../engine-overlays/browser/base/content/zen-assets.inc.xhtml');
    expect(
      assets.includes(
        '<link rel="stylesheet" type="text/css" href="chrome://browser/content/zen-styles/nevoflux-agent-touched.css" />'
      )
    ).toBe(true);
  });

  it('draws on the icon stack without colliding with the sound indicator', () => {
    const css = read('../../overlays/common/styles/nevoflux-agent-touched.css');
    expect(css.includes('tab.tabbrowser-tab[nevoflux-agent-touched] .tab-icon-stack::before {')).toBe(true);
    // ::after is Zen's sound-playing note; the dot must not take it over.
    expect(css.includes('::after')).toBe(false);
    expect(css.includes('grid-area: 1 / 1;')).toBe(true);
    expect(css.includes('align-self: end;')).toBe(true);
    expect(css.includes('justify-self: end;')).toBe(true);
    expect(css.includes('var(--zen-primary-color)')).toBe(true);
    // Zen variables only, so light and dark themes both follow the accent.
    expect(/#[0-9a-fA-F]{3,8}\b/.test(css)).toBe(false);
  });
});
