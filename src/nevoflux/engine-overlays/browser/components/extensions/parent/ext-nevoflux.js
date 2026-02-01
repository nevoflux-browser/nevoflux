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
const networkCaptures = new Map();  // handle -> { options, requests, createdAt }
const networkIntercepts = new Map(); // handle -> { options, listener }
let captureCounter = 0;
let interceptCounter = 0;

// Maximum age for captures before auto-cleanup (5 minutes)
const CAPTURE_MAX_AGE_MS = 300000;

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

        async getMarkdown(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "getMarkdown", options);
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

            // Get target window - use Services.wm for reliable window access
            let nativeWindow;
            let windowWrapper;

            if (windowId !== undefined) {
              // Try to get window by ID through windowManager
              try {
                windowWrapper = extension.windowManager.get(windowId, extension.context);
                if (windowWrapper) {
                  nativeWindow = windowWrapper.window;
                }
              } catch (e) {
                // Fall through to use most recent window
              }
            }

            // Fallback: get most recent browser window directly
            if (!nativeWindow) {
              nativeWindow = Services.wm.getMostRecentWindow("navigator:browser");
            }

            // Validate window
            if (!nativeWindow) {
              return { success: false, error: { code: 5002, message: "No browser window found", recoverable: false } };
            }

            // Validate gBrowser
            if (!nativeWindow.gBrowser) {
              return { success: false, error: { code: 5003, message: "Window has no gBrowser", recoverable: false } };
            }

            // Create the tab
            let tab;
            try {
              tab = nativeWindow.gBrowser.addTab(url || "about:newtab", {
                triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
              });
            } catch (e) {
              return { success: false, error: { code: 6002, message: `Failed to add tab: ${e}`, recoverable: false } };
            }

            if (!tab) {
              return { success: false, error: { code: 6003, message: "addTab returned null", recoverable: false } };
            }

            // Move tab to the active workspace using Zen's workspace management
            // This is required for the tab to appear in Zen's sidebar
            // Simply setting the attribute is not enough - we must use moveTabToWorkspace
            // which inserts the tab into the correct DOM container
            try {
              const gZenWorkspaces = nativeWindow.gZenWorkspaces;
              if (gZenWorkspaces && gZenWorkspaces.activeWorkspace) {
                gZenWorkspaces.moveTabToWorkspace(tab, gZenWorkspaces.activeWorkspace);
              }
            } catch {
              // Fallback: at least set the attribute if moveTabToWorkspace fails
              try {
                const gZenWorkspaces = nativeWindow.gZenWorkspaces;
                if (gZenWorkspaces && gZenWorkspaces.activeWorkspace) {
                  tab.setAttribute("zen-workspace-id", gZenWorkspaces.activeWorkspace);
                }
              } catch {
                // Ignore if gZenWorkspaces is not available
              }
            }

            // Move tab to specified index if provided
            if (index !== undefined) {
              try {
                nativeWindow.gBrowser.moveTabTo(tab, index);
              } catch (e) {
                // Ignore move errors
              }
            }

            if (active) {
              try {
                nativeWindow.gBrowser.selectedTab = tab;
              } catch (e) {
                // Ignore activation errors
              }
            }

            // Get tab ID
            let tabId;
            try {
              if (!tabTracker) {
                return { success: false, error: { code: 6004, message: "Tab tracker unavailable", recoverable: false } };
              }
              tabId = tabTracker.getId(tab);
            } catch (e) {
              return { success: false, error: { code: 6005, message: `Failed to get tab ID: ${e}`, recoverable: false } };
            }

            // Get windowId - use wrapper if available, otherwise get from docShell
            let resolvedWindowId = null;
            if (windowWrapper) {
              resolvedWindowId = windowWrapper.id;
            } else {
              try {
                resolvedWindowId = nativeWindow.docShell?.outerWindowID || null;
              } catch (e) {
                // Ignore
              }
            }

            // Defensive check for tab position - the tab object should still be valid
            let tabPosition = 0;
            try {
              if (tab && typeof tab._tPos === "number") {
                tabPosition = tab._tPos;
              }
            } catch (e) {
              // Tab might be in invalid state, use default
            }

            return {
              success: true,
              tab: {
                id: tabId,
                url: url || "about:newtab",
                title: "",
                active,
                index: tabPosition,
                windowId: resolvedWindowId,
                status: "loading",
              },
            };
          } catch (e) {
            // Catch-all for any unexpected errors
            return { success: false, error: { code: 6000, message: `Unexpected error in createTab: ${e}`, recoverable: false } };
          }
        },

        async closeTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));

          // Get the native tab directly from tabTracker for most accurate reference
          let nativeTab;
          try {
            nativeTab = tabTracker.getTab(resolvedTabId);
          } catch (e) {
            // Fallback to tabManager
            const tab = extension.tabManager.get(resolvedTabId);
            nativeTab = tab?.nativeTab;
          }

          if (!nativeTab) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          const win = nativeTab.ownerGlobal;
          if (!win || !win.gBrowser) {
            return { success: false, error: { code: 6007, message: "Tab's window is no longer available", recoverable: false } };
          }

          const gBrowser = win.gBrowser;

          // Ensure tab has zen-workspace-id for Zen compatibility
          if (!nativeTab.hasAttribute("zen-workspace-id")) {
            try {
              const gZenWorkspaces = win.gZenWorkspaces;
              if (gZenWorkspaces && gZenWorkspaces.activeWorkspace) {
                nativeTab.setAttribute("zen-workspace-id", gZenWorkspaces.activeWorkspace);
              }
            } catch (e) {
              // Ignore
            }
          }

          // Use a Promise with setTimeout to defer the removal
          // This helps avoid race conditions with Zen's workspace event handlers
          return new Promise((resolve) => {
            // Defer removal to next event loop tick
            win.setTimeout(() => {
              let internalError = null;
              try {
                // Set bypass flag immediately before removeTab
                // This bypasses Zen's handleTabBeforeClose logic which handles
                // workspace tab management (e.g., creating empty tab when last tab closes)
                if (win.gZenWorkspaces) {
                  win.gZenWorkspaces._removedByStartupPage = true;
                }

                gBrowser.removeTab(nativeTab, {
                  animate: false,
                  skipPermitUnload: true,
                  closeWindowWithLastTab: false,
                });
              } catch (e) {
                // Capture the error but don't resolve yet - check if tab was actually closed
                internalError = e;
              } finally {
                // Reset bypass flag so user's manual tab closes still work normally
                if (win.gZenWorkspaces) {
                  win.gZenWorkspaces._removedByStartupPage = false;
                }
              }

              // Check if tab was actually closed, regardless of internal errors
              // Zen's event handlers may throw errors but the tab can still be successfully closed
              const tabActuallyClosed = nativeTab.closing || !nativeTab.parentNode || !gBrowser.tabs.includes(nativeTab);
              if (tabActuallyClosed) {
                resolve({ success: true });
              } else if (internalError) {
                resolve({ success: false, error: { code: 6002, message: `closeTab error: ${internalError.message || internalError}`, recoverable: false } });
              } else {
                resolve({ success: false, error: { code: 6008, message: "Tab was not closed", recoverable: false } });
              }
            }, 0);
          });
        },

        _getTabInfo(tab, tabId) {
          const nativeTab = tab.nativeTab;
          const browser = tab.browser;
          return {
            id: tabId,
            zenSyncId: nativeTab.id || null,  // Zen Browser's persistent tab ID for session association
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
            listener: null,
            createdAt: Date.now()
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
          // Auto-cleanup old captures first
          const now = Date.now();
          for (const [h, data] of networkCaptures) {
            if (now - data.createdAt > CAPTURE_MAX_AGE_MS) {
              networkCaptures.delete(h);
            }
          }

          const captureData = networkCaptures.get(handle);
          if (!captureData) {
            return { success: false, error: { code: 8001, message: "Capture not found", recoverable: false } };
          }

          const requests = [...captureData.requests];
          networkCaptures.delete(handle);
          return requests;
        },

        async getCaptures(handle) {
          // Auto-cleanup old captures first
          const now = Date.now();
          for (const [h, data] of networkCaptures) {
            if (now - data.createdAt > CAPTURE_MAX_AGE_MS) {
              networkCaptures.delete(h);
            }
          }

          const captureData = networkCaptures.get(handle);
          if (!captureData) {
            return { success: false, error: { code: 8001, message: "Capture not found", recoverable: false } };
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

          // Import timer functions once, outside the loop
          const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
            "resource://gre/modules/Timer.sys.mjs"
          );

          const startTime = Date.now();
          while (Date.now() - startTime < timeout) {
            const requests = await this.getCaptures(handle);
            if (requests.length > 0) {
              await this.stopCapture(handle);
              return requests[0];
            }
            await new Promise(resolve => {
              chromeSetTimeout(resolve, 100);
            });
          }

          await this.stopCapture(handle);
          return { success: false, error: { code: 8003, message: "Timeout waiting for request", recoverable: true } };
        },

        async waitForResponse(urlPattern, timeout = 30000) {
          // PLACEHOLDER: This method currently returns request data, NOT actual response data.
          // A full implementation would require hooking into webRequest.onCompleted or
          // webRequest.onResponseStarted to capture response headers, status codes, and body.
          // For now, it delegates to waitForRequest as a temporary workaround.
          // TODO: Implement proper response capture with status, headers, and body data.
          return this.waitForRequest(urlPattern, timeout);
        },

        // ========== JavaScript Execution ==========

        async eval(tabId, script, options = {}) {
          if (!script || typeof script !== "string") {
            return { success: false, error: { code: 9002, message: "Missing or invalid required parameter: script", recoverable: false } };
          }
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "eval", { script, ...options });
        },

        async addScript(tabId, script, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "addScript", { script, ...options });
        },

        async removeScript(tabId, handle) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "removeScript", { handle });
        },

        // ========== Frame Management ==========

        async listFrames(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "listFrames", {});
        },

        async switchFrame(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "switchFrame", { selector });
        },

        async frameMain(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "frameMain", {});
        },

        // ========== Dialog Handling ==========

        async dialogAccept(text) {
          // Dialog handling is at the window level, not tab level
          // Route through the current tab's actor to reach NevofluxParent
          const tabId = await self.getActiveTabId(extension);
          const tab = extension.tabManager.get(tabId);
          if (!tab?.browser) {
            // No tab, but dialog might exist - silently succeed
            return { success: true };
          }

          try {
            const actor = tab.browser.browsingContext.currentWindowGlobal.getActor("Nevoflux");
            return actor.sendQuery("dialogAccept", { text });
          } catch (e) {
            // Actor not available - silently succeed (no dialog)
            return { success: true };
          }
        },

        async dialogDismiss() {
          const tabId = await self.getActiveTabId(extension);
          const tab = extension.tabManager.get(tabId);
          if (!tab?.browser) {
            return { success: true };
          }

          try {
            const actor = tab.browser.browsingContext.currentWindowGlobal.getActor("Nevoflux");
            return actor.sendQuery("dialogDismiss", {});
          } catch (e) {
            return { success: true };
          }
        },

        // ========== Download Handling ==========

        async waitForDownload(options = {}) {
          const timeout = options.timeout || 30000;

          // Import timer functions for extension parent context
          const { setTimeout: chromeSetTimeout, clearTimeout: chromeClearTimeout } = ChromeUtils.importESModule(
            "resource://gre/modules/Timer.sys.mjs"
          );

          return new Promise((resolve) => {
            let timeoutId;
            let downloadListener;

            const cleanup = () => {
              if (timeoutId) {
                chromeClearTimeout(timeoutId);
              }
              if (downloadListener) {
                try {
                  // Remove listener using internal API
                  // Note: This uses Firefox's internal download manager
                  Services.obs.removeObserver(downloadListener, "dl-start");
                } catch (e) {
                  // Listener already removed
                }
              }
            };

            timeoutId = chromeSetTimeout(() => {
              cleanup();
              resolve({
                success: false,
                error: { code: 12001, message: "Download timeout", recoverable: true }
              });
            }, timeout);

            downloadListener = {
              observe: (subject, topic, data) => {
                if (topic === "dl-start") {
                  cleanup();

                  try {
                    // subject is nsIDownload
                    const download = subject.QueryInterface(Ci.nsIDownload);
                    resolve({
                      success: true,
                      url: download.source?.spec || "",
                      filename: download.targetFile?.leafName || "",
                      mimeType: download.MIMEInfo?.MIMEType || "",
                      size: download.totalBytes || -1
                    });
                  } catch (e) {
                    // Fallback for different Firefox versions
                    resolve({
                      success: true,
                      url: "",
                      filename: "",
                      mimeType: "",
                      size: -1
                    });
                  }
                }
              }
            };

            try {
              Services.obs.addObserver(downloadListener, "dl-start");
            } catch (e) {
              cleanup();
              resolve({
                success: false,
                error: { code: 12001, message: `Failed to observe downloads: ${e}`, recoverable: false }
              });
            }
          });
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
    console.log(`[ext-nevoflux] executeInTab: action=${action}, tabId=${tabId}, params=`, JSON.stringify(params));
    const tab = extension.tabManager.get(tabId);
    if (!tab) {
      console.log("[ext-nevoflux] executeInTab: Tab not found");
      return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
    }

    try {
      // Get the browser - try multiple approaches
      let browser = tab.browser || tab.linkedBrowser;
      const nativeTab = tab.nativeTab;

      console.log(`[ext-nevoflux] executeInTab: browser=${!!browser}, nativeTab=${!!nativeTab}`);

      // If no browser directly, try to get it from the native tab
      if (!browser && nativeTab) {
        browser = nativeTab.linkedBrowser;
        console.log(`[ext-nevoflux] executeInTab: got browser from nativeTab.linkedBrowser=${!!browser}`);
      }

      if (!browser) {
        console.log("[ext-nevoflux] executeInTab: No browser element found");
        return { success: false, error: { code: 3002, message: "No browser element found", recoverable: false } };
      }

      let bc = browser.browsingContext;
      console.log(`[ext-nevoflux] executeInTab: initial bc=${!!bc}`);

      // If no browsingContext, the tab needs to be activated first
      if (!bc) {
        const isPending = nativeTab?.hasAttribute("pending");
        const isDiscarded = nativeTab?.hasAttribute("discarded");
        console.log(`[ext-nevoflux] executeInTab: No browsingContext, isPending=${isPending}, isDiscarded=${isDiscarded}`);

        return {
          success: false,
          error: {
            code: 5004,
            message: "Tab not loaded",
            recoverable: true,
            reason: isPending ? "pending" : (isDiscarded ? "discarded" : "unknown"),
            suggestion: "Use activate_tab to load the tab first"
          }
        };
      }

      // Check if browsing context is discarded
      if (bc.isDiscarded) {
        console.log("[ext-nevoflux] executeInTab: BrowsingContext is discarded");
        return { success: false, error: { code: 5003, message: "BrowsingContext is discarded", recoverable: false } };
      }

      // Strategy 1: Try currentWindowGlobal first (fast path for active tabs)
      let windowGlobal = bc.currentWindowGlobal;
      console.log(`[ext-nevoflux] executeInTab: bc=${!!bc}, cwg=${!!windowGlobal}`);

      // Strategy 2: If null, try getWindowGlobals() to find any valid WindowGlobal
      if (!windowGlobal) {
        console.log("[ext-nevoflux] executeInTab: currentWindowGlobal is null, trying getWindowGlobals()");
        const windowGlobals = bc.getWindowGlobals();
        console.log(`[ext-nevoflux] executeInTab: found ${windowGlobals.length} windowGlobals`);

        // Find a valid (non-closed, non-BFCache) window global
        windowGlobal = windowGlobals.find(wg => !wg.isClosed && !wg.isInBFCache);

        if (windowGlobal) {
          console.log(`[ext-nevoflux] executeInTab: using windowGlobal from getWindowGlobals()`);
        }
      }

      // Strategy 3: If still null, wait briefly for tab transition
      if (!windowGlobal) {
        console.log("[ext-nevoflux] executeInTab: waiting for windowGlobal...");
        windowGlobal = await this.waitForWindowGlobal(bc, 500);
      }

      // Final check
      if (!windowGlobal) {
        console.log("[ext-nevoflux] executeInTab: No windowGlobal available after all strategies");
        return {
          success: false,
          error: {
            code: 5002,
            message: "No windowGlobal available (tab may be unloaded)",
            recoverable: true,
            suggestion: "Try activating the tab first"
          }
        };
      }

      const actor = windowGlobal.getActor("Nevoflux");
      console.log(`[ext-nevoflux] executeInTab: actor=${!!actor}, sending query...`);
      const result = await actor.sendQuery("execute", { action, params });
      console.log(`[ext-nevoflux] executeInTab: result=`, result);
      return result;
    } catch (e) {
      console.error(`[ext-nevoflux] executeInTab: Error - ${e.message}`, e.stack);
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }
  }

  async waitForWindowGlobal(bc, maxWaitMs = 500) {
    const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
      "resource://gre/modules/Timer.sys.mjs"
    );

    const waitInterval = 50;
    let waited = 0;

    while (waited < maxWaitMs) {
      await new Promise(resolve => chromeSetTimeout(resolve, waitInterval));
      waited += waitInterval;

      // Check currentWindowGlobal first
      if (bc.currentWindowGlobal) {
        return bc.currentWindowGlobal;
      }

      // Also check getWindowGlobals()
      const windowGlobals = bc.getWindowGlobals();
      const validWg = windowGlobals.find(wg => !wg.isClosed && !wg.isInBFCache);
      if (validWg) {
        return validWg;
      }
    }

    return null;
  }

  async waitForBrowsingContext(browser, maxWaitMs = 2000) {
    const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
      "resource://gre/modules/Timer.sys.mjs"
    );

    const waitInterval = 100;
    let waited = 0;

    while (waited < maxWaitMs) {
      if (browser.browsingContext) {
        console.log(`[ext-nevoflux] waitForBrowsingContext: got browsingContext after ${waited}ms`);
        return browser.browsingContext;
      }

      await new Promise(resolve => chromeSetTimeout(resolve, waitInterval));
      waited += waitInterval;
    }

    console.log(`[ext-nevoflux] waitForBrowsingContext: timeout after ${maxWaitMs}ms`);
    return null;
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
