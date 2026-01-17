/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

// API version for compatibility checking
const API_VERSION = "1.0.0";

// Helper to escape regex special characters for safe pattern construction
function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// Default privacy config
const DEFAULT_PRIVACY_CONFIG = {
  enabled: true,
  filters: {
    phone: true,
    idCard: true,
    email: true,
    bankCard: true,
    address: false,
    name: false,
  },
  mode: "redact",
  scope: "external_only",
};

// Privacy config storage
let privacyConfig = { ...DEFAULT_PRIVACY_CONFIG };

// Network capture state
const networkCaptures = new Map();  // handle -> { options, requests }
const networkIntercepts = new Map(); // handle -> { options, listener }
let captureCounter = 0;
let interceptCounter = 0;

this.nevoflux = class extends ExtensionAPI {
  getAPI(context) {
    const { extension } = context;
    const self = this;

    return {
      nevoflux: {
        // ========== Version ==========

        async getVersion() {
          return API_VERSION;
        },

        // ========== Data Extraction (chat mode) ==========

        async getText(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getText", { selector: selector || "body" });
        },

        async getHtml(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getHtml", { selector: selector || "body" });
        },

        async getValue(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getValue", { selector });
        },

        async getUrl(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);
          return tab?.browser?.currentURI?.spec || "";
        },

        async getTitle(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);
          return tab?.browser?.contentTitle || "";
        },

        async snapshot(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "snapshot", options);
        },

        async screenshot(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "screenshot", options);
        },

        // ========== State Checking (chat mode) ==========

        async isVisible(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "isVisible", { selector });
        },

        async exists(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "exists", { selector });
        },

        // ========== Interaction (browser_use mode) ==========

        async click(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "click", { selector, ...options });
        },

        async type(tabId, selector, text, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "type", { selector, text, ...options });
        },

        async fill(tabId, selector, text) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "fill", { selector, text });
        },

        // ========== Navigation (browser_use mode) ==========

        async open(tabId, url, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          try {
            // Use fixupURI for simpler URL handling
            const uri = Services.io.newURI(url);
            const principal = Services.scriptSecurityManager.getSystemPrincipal();

            tab.browser.loadURI(uri, {
              triggeringPrincipal: principal,
            });

            return { success: true, url: uri.spec };
          } catch (e) {
            // Fallback: try using the tabs API
            try {
              const tabsApi = extension.apiManager.getAPI("tabs", extension, "addon_parent");
              if (tabsApi?.tabs?.update) {
                await tabsApi.tabs.update(resolvedTabId, { url });
                return { success: true, url };
              }
            } catch (fallbackError) {
              // Ignore fallback error
            }
            return { success: false, error: { code: 2001, message: String(e), recoverable: true } };
          }
        },

        async reload(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          try {
            tab.browser.reload();
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 2001, message: String(e), recoverable: true } };
          }
        },

        async back(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          if (tab.browser.canGoBack) {
            tab.browser.goBack();
            return { success: true };
          }
          return { success: false, error: { code: 2002, message: "Cannot go back", recoverable: false } };
        },

        async forward(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          if (tab.browser.canGoForward) {
            tab.browser.goForward();
            return { success: true };
          }
          return { success: false, error: { code: 2002, message: "Cannot go forward", recoverable: false } };
        },

        // ========== Tab Management (browser_use mode) ==========

        async createTab(options = {}) {
          try {
            const { url, active = true, windowId, index } = options;

            // Get target window - use specified windowId or fall back to top window
            let win;
            if (windowId !== undefined) {
              const wrapper = extension.windowManager.get(windowId, extension.context);
              if (!wrapper) {
                return { success: false, error: { code: 5001, message: "Window not found", recoverable: false } };
              }
              win = wrapper;
            } else {
              win = extension.windowManager.getWrapper(extension.windowManager.topWindow);
            }

            const tab = win.window.gBrowser.addTab(url || "about:newtab", {
              triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
            });

            // Move tab to specified index if provided
            if (index !== undefined) {
              win.window.gBrowser.moveTabTo(tab, index);
            }

            if (active) {
              win.window.gBrowser.selectedTab = tab;
            }

            // Guard against tabTracker being unavailable
            if (!tabTracker) {
              return { success: false, error: { code: 6001, message: "Tab tracker unavailable", recoverable: false } };
            }

            const tabId = tabTracker.getId(tab);
            return {
              success: true,
              tab: {
                id: tabId,
                url: url || "about:newtab",
                title: "",
                active,
                index: tab._tPos,
                windowId: win.id,
                status: "loading",
              },
            };
          } catch (e) {
            return { success: false, error: { code: 6001, message: String(e), recoverable: false } };
          }
        },

        async closeTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          try {
            const nativeTab = tab.nativeTab;
            const win = nativeTab.ownerGlobal;
            win.gBrowser.removeTab(nativeTab);
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 6002, message: String(e), recoverable: false } };
          }
        },

        _getTabInfo(tab, tabId) {
          const nativeTab = tab.nativeTab;
          const browser = tab.browser;
          return {
            id: tabId,
            url: browser?.currentURI?.spec || "",
            title: browser?.contentTitle || "",
            active: nativeTab === nativeTab.ownerGlobal.gBrowser.selectedTab,
            index: nativeTab._tPos,
            windowId: extension.windowManager.getWrapper(nativeTab.ownerGlobal)?.id || 0,
            status: nativeTab.linkedBrowser?.webProgress?.isLoadingDocument ? "loading" : "complete",
          };
        },

        async getTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          return this._getTabInfo(tab, resolvedTabId);
        },

        async listTabs(windowId) {
          // Get target window - use specified windowId or fall back to current/top window
          let win;
          if (windowId !== undefined) {
            const wrapper = extension.windowManager.get(windowId, extension.context);
            if (!wrapper) {
              return [];
            }
            win = wrapper;
          } else {
            win = extension.windowManager.getWrapper(extension.windowManager.topWindow);
          }

          if (!win) {
            return [];
          }

          const tabs = [];
          for (const nativeTab of win.window.gBrowser.tabs) {
            const tabId = tabTracker.getId(nativeTab);
            const tab = extension.tabManager.get(tabId);
            if (tab) {
              tabs.push(this._getTabInfo(tab, tabId));
            }
          }
          return tabs;
        },

        async queryTabs(filter = {}) {
          const allTabs = await this.listTabs();

          return allTabs.filter(tab => {
            if (filter.active !== undefined && tab.active !== filter.active) return false;
            if (filter.windowId !== undefined && tab.windowId !== filter.windowId) return false;
            if (filter.url) {
              // Escape regex special chars first, then convert wildcards to regex
              const escaped = escapeRegExp(filter.url);
              const pattern = escaped.replace(/\\\*/g, ".*");
              const regex = new RegExp(`^${pattern}$`);
              if (!regex.test(tab.url)) return false;
            }
            if (filter.title) {
              // Escape regex special chars first, then convert wildcards to regex
              const escaped = escapeRegExp(filter.title);
              const pattern = escaped.replace(/\\\*/g, ".*");
              const regex = new RegExp(`^${pattern}$`, "i");
              if (!regex.test(tab.title)) return false;
            }
            return true;
          });
        },

        async activateTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          try {
            const nativeTab = tab.nativeTab;
            const win = nativeTab.ownerGlobal;
            win.gBrowser.selectedTab = nativeTab;
            win.focus();
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
          }
        },

        async createWindow(options = {}) {
          try {
            const { url, incognito = false, width, height, left, top } = options;

            const features = [];
            if (width) features.push(`width=${width}`);
            if (height) features.push(`height=${height}`);
            if (left !== undefined) features.push(`left=${left}`);
            if (top !== undefined) features.push(`top=${top}`);

            let newWindow;
            if (incognito) {
              newWindow = Services.ww.openWindow(
                null,
                "chrome://browser/content/browser.xhtml",
                "_blank",
                `chrome,dialog=no,all,private${features.length ? "," + features.join(",") : ""}`,
                null
              );
            } else {
              newWindow = Services.ww.openWindow(
                null,
                "chrome://browser/content/browser.xhtml",
                "_blank",
                `chrome,dialog=no,all${features.length ? "," + features.join(",") : ""}`,
                null
              );
            }

            await new Promise(resolve => {
              newWindow.addEventListener("load", resolve, { once: true });
            });

            if (url) {
              newWindow.gBrowser.loadURI(Services.io.newURI(url), {
                triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
              });
            }

            const windowId = extension.windowManager.getWrapper(newWindow)?.id || 0;
            return { success: true, windowId };
          } catch (e) {
            return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
          }
        },

        async closeWindow(windowId) {
          try {
            let targetWindow;
            if (windowId !== undefined) {
              const wrapper = extension.windowManager.get(windowId, extension.context);
              targetWindow = wrapper?.window;
            } else {
              targetWindow = extension.windowManager.topWindow;
            }

            if (!targetWindow) {
              return { success: false, error: { code: 5001, message: "Window not found", recoverable: false } };
            }

            targetWindow.close();
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
          }
        },

        // ========== Wait (browser_use mode) ==========

        async waitForSelector(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "waitForSelector", { selector, ...options });
        },

        async waitForTimeout(ms) {
          // In extension parent context, use lazy getter for ChromeUtils
          const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
            "resource://gre/modules/Timer.sys.mjs"
          );
          await new Promise(resolve => chromeSetTimeout(resolve, ms));
          return { success: true };
        },

        // ========== Keyboard/Mouse Control ==========

        async keyPress(tabId, key, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "keyPress", { key, ...options });
        },

        async keyDown(tabId, key, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "keyDown", { key, ...options });
        },

        async keyUp(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "keyUp", { key });
        },

        async mouseMove(tabId, x, y, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "mouseMove", { x, y, ...options });
        },

        async mouseDown(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "mouseDown", options);
        },

        async mouseUp(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "mouseUp", options);
        },

        async wheel(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "wheel", options);
        },

        async dblclick(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "dblclick", { selector, ...options });
        },

        async drag(tabId, fromSelector, toSelector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "drag", { fromSelector, toSelector, ...options });
        },

        async focus(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "focus", { selector });
        },

        async clear(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "clear", { selector });
        },

        // ========== Privacy (all modes) ==========

        async filterSensitive(text, options = {}) {
          // For now, use simple regex filtering
          // TODO: Integrate with Rust kernel via UniFFI when available
          return self.filterText(text, options);
        },

        async getPrivacyConfig() {
          return privacyConfig;
        },

        async setPrivacyConfig(config) {
          privacyConfig = { ...privacyConfig, ...config };
          if (config.filters) {
            privacyConfig.filters = { ...privacyConfig.filters, ...config.filters };
          }
          return privacyConfig;
        },

        // ========== Cookie Management ==========

        async getCookies(filter = {}) {
          try {
            const cookieManager = Services.cookies;
            const cookies = [];

            // Validate filter.url if provided
            let filterHostname = null;
            if (filter.url) {
              try {
                const parsedUrl = new URL(filter.url);
                filterHostname = parsedUrl.hostname;
              } catch (urlError) {
                return { success: false, error: { code: 7002, message: `Invalid URL in filter: ${filter.url}`, recoverable: false } };
              }
            }

            for (const cookie of cookieManager.cookies) {
              if (filter.name && cookie.name !== filter.name) continue;
              if (filter.domain && !cookie.host.endsWith(filter.domain)) continue;
              if (filterHostname && !cookie.host.endsWith(filterHostname)) continue;

              cookies.push({
                name: cookie.name,
                value: cookie.value,
                domain: cookie.host,
                path: cookie.path,
                secure: cookie.isSecure,
                httpOnly: cookie.isHttpOnly,
                sameSite: ["none", "lax", "strict"][cookie.sameSite] || "none",
                expirationDate: cookie.expiry,
                session: cookie.isSession
              });
            }

            return cookies;
          } catch (e) {
            return { success: false, error: { code: 7001, message: String(e), recoverable: false } };
          }
        },

        async setCookie(cookie) {
          try {
            const { url, name, value, domain, path = "/", secure = false, httpOnly = false, sameSite = "lax", expirationDate } = cookie;

            // Validate required parameters
            if (!url) {
              return { success: false, error: { code: 7002, message: "Missing required parameter: url", recoverable: false } };
            }
            if (!name) {
              return { success: false, error: { code: 7002, message: "Missing required parameter: name", recoverable: false } };
            }
            if (value === undefined || value === null) {
              return { success: false, error: { code: 7002, message: "Missing required parameter: value", recoverable: false } };
            }

            let parsedUrl;
            try {
              parsedUrl = new URL(url);
            } catch (urlError) {
              return { success: false, error: { code: 7002, message: `Invalid URL: ${url}`, recoverable: false } };
            }

            const cookieDomain = domain || parsedUrl.hostname;
            const sameSiteMap = { "none": 0, "lax": 1, "strict": 2 };

            Services.cookies.add(
              cookieDomain,
              path,
              name,
              value,
              secure,
              httpOnly,
              !expirationDate,  // isSession
              expirationDate || Math.floor(Date.now() / 1000) + 86400 * 365,
              {},  // originAttributes
              sameSiteMap[sameSite] || 1,
              Ci.nsICookie.SCHEME_HTTPS
            );

            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 7001, message: String(e), recoverable: false } };
          }
        },

        async deleteCookies(filter = {}) {
          try {
            const cookiesToDelete = await this.getCookies(filter);

            for (const cookie of cookiesToDelete) {
              Services.cookies.remove(cookie.domain, cookie.name, cookie.path, {});
            }

            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
          }
        },

        async clearCookies(domain) {
          try {
            if (domain) {
              const filter = { domain };
              return this.deleteCookies(filter);
            } else {
              Services.cookies.removeAll();
              return { success: true };
            }
          } catch (e) {
            return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
          }
        },

        // ========== Storage Management ==========

        async getLocalStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getLocalStorage", { key });
        },

        async setLocalStorage(tabId, key, value) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "setLocalStorage", { key, value });
        },

        async removeLocalStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "removeLocalStorage", { key });
        },

        async clearLocalStorage(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "clearLocalStorage", {});
        },

        async getSessionStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getSessionStorage", { key });
        },

        async setSessionStorage(tabId, key, value) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "setSessionStorage", { key, value });
        },

        async removeSessionStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "removeSessionStorage", { key });
        },

        async clearSessionStorage(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "clearSessionStorage", {});
        },

        // ========== Network ==========

        async startCapture(options = {}) {
          const handle = `capture_${++captureCounter}`;
          const captureData = {
            options,
            requests: [],
            listener: null
          };

          const { urlPattern, resourceTypes, recordBody = false } = options;

          // Create webRequest listener
          const listener = (details) => {
            // Check URL pattern
            if (urlPattern) {
              const escaped = escapeRegExp(urlPattern);
              const pattern = escaped.replace(/\\\*/g, ".*");
              const regex = new RegExp(`^${pattern}$`);
              if (!regex.test(details.url)) return;
            }

            // Check resource type
            if (resourceTypes && !resourceTypes.includes(details.type)) return;

            captureData.requests.push({
              url: details.url,
              method: details.method,
              resourceType: details.type,
              headers: details.requestHeaders ?
                Object.fromEntries(details.requestHeaders.map(h => [h.name, h.value])) : {},
              timestamp: Date.now()
            });
          };

          // Note: Full implementation would use browser.webRequest API
          // For now, store a simplified version
          captureData.listener = listener;
          networkCaptures.set(handle, captureData);

          return handle;
        },

        async stopCapture(handle) {
          const captureData = networkCaptures.get(handle);
          if (!captureData) {
            return [];
          }

          const requests = [...captureData.requests];
          networkCaptures.delete(handle);
          return requests;
        },

        async getCaptures(handle) {
          const captureData = networkCaptures.get(handle);
          if (!captureData) {
            return [];
          }
          return [...captureData.requests];
        },

        async intercept(options) {
          const handle = `intercept_${++interceptCounter}`;
          const { urlPattern, handler, mockResponse, modifyHeaders, resourceTypes } = options;

          const interceptData = {
            options,
            active: true
          };

          // Note: Full implementation would use browser.webRequest.onBeforeRequest
          // with blocking: true. For now, store configuration.
          networkIntercepts.set(handle, interceptData);

          return handle;
        },

        async removeIntercept(handle) {
          if (!networkIntercepts.has(handle)) {
            return { success: false, error: { code: 8002, message: "Intercept not found", recoverable: false } };
          }

          networkIntercepts.delete(handle);
          return { success: true };
        },

        async clearIntercepts() {
          networkIntercepts.clear();
          return { success: true };
        },

        async waitForRequest(urlPattern, timeout = 30000) {
          // Create a capture, wait for matching request, then clean up
          const handle = await this.startCapture({ urlPattern });

          const startTime = Date.now();
          while (Date.now() - startTime < timeout) {
            const requests = await this.getCaptures(handle);
            if (requests.length > 0) {
              await this.stopCapture(handle);
              return requests[0];
            }
            await new Promise(resolve => {
              const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
                "resource://gre/modules/Timer.sys.mjs"
              );
              chromeSetTimeout(resolve, 100);
            });
          }

          await this.stopCapture(handle);
          return { success: false, error: { code: 8003, message: "Timeout waiting for request", recoverable: true } };
        },

        async waitForResponse(urlPattern, timeout = 30000) {
          // Similar to waitForRequest but would wait for response
          return this.waitForRequest(urlPattern, timeout);
        },
      },
    };
  }

  // ========== Helper Methods ==========

  getActiveTabId(extension) {
    // tabTracker is a global from ext-browser.js
    const activeTab = tabTracker?.activeTab;
    if (!activeTab) {
      return null;
    }
    return tabTracker.getId(activeTab);
  }

  async executeInTab(tabId, extension, action, params) {
    const tab = extension.tabManager.get(tabId);
    if (!tab?.browser) {
      return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
    }

    try {
      const actor = tab.browser.browsingContext.currentWindowGlobal.getActor("Nevoflux");
      return actor.sendQuery("execute", { action, params });
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }
  }

  waitForLoad(browser, waitUntil, timeout) {
    // Import timer functions for extension parent context
    const { setTimeout: chromeSetTimeout, clearTimeout: chromeClearTimeout } = ChromeUtils.importESModule(
      "resource://gre/modules/Timer.sys.mjs"
    );

    return new Promise((resolve, reject) => {
      const timeoutId = chromeSetTimeout(() => {
        cleanup();
        reject(new Error("Navigation timeout"));
      }, timeout);

      const listener = {
        onStateChange(webProgress, request, flags, status) {
          const isStop = flags & Ci.nsIWebProgressListener.STATE_STOP;
          const isNetwork = flags & Ci.nsIWebProgressListener.STATE_IS_NETWORK;

          if (waitUntil === "load" && isStop && isNetwork) {
            cleanup();
            resolve();
          }
        },
        QueryInterface: ChromeUtils.generateQI(["nsIWebProgressListener", "nsISupportsWeakReference"]),
      };

      const cleanup = () => {
        chromeClearTimeout(timeoutId);
        try {
          browser.removeProgressListener(listener);
        } catch (e) {
          // Ignore if already removed
        }
      };

      browser.addProgressListener(listener, Ci.nsIWebProgress.NOTIFY_STATE_ALL);
    });
  }

  filterText(text, options = {}) {
    const config = { ...privacyConfig.filters, ...options };
    let result = text;
    let filteredCount = 0;

    // IMPORTANT: Process ID card BEFORE phone to avoid partial matches
    // ID card is 18 digits, phone is 11 digits - phone regex can match inside ID card
    if (config.idCard !== false) {
      const idRegex = /\d{17}[\dXx]/g;
      const matches = result.match(idRegex) || [];
      filteredCount += matches.length;
      result = result.replace(idRegex, "[ID_REDACTED]");
    }

    if (config.phone !== false) {
      // Use word boundary-like check to avoid matching inside other numbers
      const phoneRegex = /(?<!\d)1[3-9]\d{9}(?!\d)/g;
      const matches = result.match(phoneRegex) || [];
      filteredCount += matches.length;
      result = result.replace(phoneRegex, "[PHONE_REDACTED]");
    }

    if (config.email !== false) {
      const emailRegex = /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g;
      const matches = result.match(emailRegex) || [];
      filteredCount += matches.length;
      result = result.replace(emailRegex, "[EMAIL_REDACTED]");
    }

    return {
      text: result,
      filteredCount,
      filtered: filteredCount > 0,
    };
  }
};
