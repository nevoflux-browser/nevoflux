/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { describe, it, expect } from "./test-runner.mjs";
import { agentTabsToMark } from "../../extensions/nevoflux-agent/background/agent-route.mjs";

const ok = (result = {}) => ({ success: true, result });

describe("agentTabsToMark", () => {
  it("marks the tab an agent call named", () => {
    expect(
      agentTabsToMark(
        {
          origin: "agent",
          action: "click_by_id",
          tab_id: 42,
          params: { element_id: "e3" },
        },
        ok({ clicked: true }),
      ),
    ).toEqual([42]);
    // Reading counts: B2 marks what the agent touched, read or write.
    expect(
      agentTabsToMark(
        { origin: "agent", action: "get_markdown", tab_id: 7, params: {} },
        ok({ markdown: "" }),
      ),
    ).toEqual([7]);
  });

  it("marks the new tab browser_navigate opened", () => {
    const req = {
      origin: "agent",
      action: "navigate",
      tab_id: null,
      params: { url: "https://example.com", new_tab: true, background: true },
    };
    expect(
      agentTabsToMark(
        req,
        ok({ url: "https://example.com", tab_id: 314, new_tab: true }),
      ),
    ).toEqual([314]);
    // Navigating a named tab in place marks that tab once.
    const inPlace = {
      origin: "agent",
      action: "navigate",
      tab_id: 9,
      params: { url: "https://example.com" },
    };
    expect(
      agentTabsToMark(
        inPlace,
        ok({ url: "https://example.com", tab_id: 9, new_tab: false }),
      ),
    ).toEqual([9]);
  });

  it("get_tabs marks nothing even with a tab_id", () => {
    expect(
      agentTabsToMark(
        { origin: "agent", action: "list_tabs", tab_id: null, params: {} },
        ok([{ id: 1 }, { id: 2 }]),
      ),
    ).toEqual([]);
    expect(
      agentTabsToMark(
        { origin: "agent", action: "list_tabs", tab_id: 5, params: {} },
        ok([]),
      ),
    ).toEqual([]);
    expect(
      agentTabsToMark(
        { origin: "agent", action: "query_tabs", tab_id: 5, params: {} },
        ok([]),
      ),
    ).toEqual([]);
  });

  it("a failed call marks nothing", () => {
    const req = { origin: "agent", action: "fill", tab_id: 42, params: {} };
    expect(
      agentTabsToMark(req, {
        success: false,
        error: { code: -1, message: "x" },
      }),
    ).toEqual([]);
    expect(agentTabsToMark(req, undefined)).toEqual([]);
    const nav = {
      origin: "agent",
      action: "navigate",
      tab_id: null,
      params: { url: "https://e.com", new_tab: true },
    };
    expect(
      agentTabsToMark(nav, { success: false, result: { tab_id: 314 } }),
    ).toEqual([]);
  });

  it("native-chat calls mark nothing", () => {
    expect(
      agentTabsToMark({ action: "click", tab_id: 42, params: {} }, ok()),
    ).toEqual([]);
    expect(
      agentTabsToMark(
        { origin: "Agent", action: "click", tab_id: 42, params: {} },
        ok(),
      ),
    ).toEqual([]);
  });

  it("ignores tab ids that are not positive integers and never repeats one", () => {
    for (const bad of [0, -3, 1.5, "42", null, undefined]) {
      expect(
        agentTabsToMark(
          { origin: "agent", action: "scroll", tab_id: bad, params: {} },
          ok(),
        ),
      ).toEqual([]);
    }
    const nav = {
      origin: "agent",
      action: "navigate",
      tab_id: null,
      params: { url: "https://e.com", new_tab: true },
    };
    expect(agentTabsToMark(nav, ok({ tab_id: "314" }))).toEqual([]);
    const both = {
      origin: "agent",
      action: "navigate",
      tab_id: 314,
      params: { url: "https://e.com", new_tab: true },
    };
    expect(agentTabsToMark(both, ok({ tab_id: 314 }))).toEqual([314]);
  });
});
