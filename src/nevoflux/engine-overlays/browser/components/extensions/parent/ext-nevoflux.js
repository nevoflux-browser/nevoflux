/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

// API version for compatibility checking
const API_VERSION = '1.0.0';

// Lazy import for SessionStore (used for restoring discarded tabs)
ChromeUtils.defineESModuleGetters(this, {
  SessionStore: 'resource:///modules/sessionstore/SessionStore.sys.mjs',
  NevofluxNativeHostRegistrar: 'resource:///modules/NevofluxNativeHostRegistrar.sys.mjs',
});

// Default timeout for tab restoration (ms)
const DEFAULT_RESTORE_TIMEOUT = 10000;

// Helper to escape regex special characters for safe pattern construction
function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
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
  mode: 'redact',
  scope: 'external_only',
};

// Privacy config storage
let privacyConfig = { ...DEFAULT_PRIVACY_CONFIG };

// Network capture state
const networkCaptures = new Map(); // handle -> { options, requests, createdAt }
const networkIntercepts = new Map(); // handle -> { options, listener }
let captureCounter = 0;
let interceptCounter = 0;

// Maximum age for captures before auto-cleanup (5 minutes)
const CAPTURE_MAX_AGE_MS = 300000;

// Module-level: unsubscribe handle for ContentStore persist callback
let contentStorePersistUnsubscribe = null; // eslint-disable-line no-unused-vars

this.nevoflux = class extends ExtensionAPI {
  onStartup() {
    NevofluxNativeHostRegistrar.ensureRegistered().catch((err) => {
      console.error('[NevoFlux] Failed to register native messaging hosts:', err);
    });
  }

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
          return self.executeInTabWithRestore(resolvedTabId, extension, 'getText', {
            selector: selector || 'body',
          });
        },

        async getHtml(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'getHtml', {
            selector: selector || 'body',
          });
        },

        async getValue(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'getValue', { selector });
        },

        async getUrl(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);
          return tab?.browser?.currentURI?.spec || '';
        },

        async getTitle(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);
          return tab?.browser?.contentTitle || '';
        },

        async snapshot(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'snapshot', options);
        },

        async screenshot(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'screenshot', options);
        },

        async getMarkdown(tabId, options = {}) {
          // Delegate to getTabContent for unified logic (auto-restore, etc.)
          const result = await this.getTabContent(tabId, {
            ...options,
            format: 'markdown',
          });

          // Convert to original return format for backward compatibility
          if (result.success === false) {
            return result;
          }

          return {
            success: true,
            markdown: result.content,
            title: result.title,
            url: result.url,
          };
        },

        // ========== State Checking (chat mode) ==========

        async isVisible(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'isVisible', { selector });
        },

        async exists(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'exists', { selector });
        },

        // ========== Interaction (browser_use mode) ==========

        async click(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'click', {
            selector,
            ...options,
          });
        },

        async clickAtCoordinates(tabId, x, y, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'clickAtCoordinates', {
            x,
            y,
            ...options,
          });
        },

        async type(tabId, selector, text, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'type', {
            selector,
            text,
            ...options,
          });
        },

        async fill(tabId, selector, text) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'fill', { selector, text });
        },

        // ========== Navigation (browser_use mode) ==========

        async open(tabId, url, _options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
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
              const tabsApi = extension.apiManager.getAPI('tabs', extension, 'addon_parent');
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

        async reload(tabId, _options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
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
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          if (tab.browser.canGoBack) {
            tab.browser.goBack();
            return { success: true };
          }
          return {
            success: false,
            error: { code: 2002, message: 'Cannot go back', recoverable: false },
          };
        },

        async forward(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          if (tab.browser.canGoForward) {
            tab.browser.goForward();
            return { success: true };
          }
          return {
            success: false,
            error: { code: 2002, message: 'Cannot go forward', recoverable: false },
          };
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
              nativeWindow = Services.wm.getMostRecentWindow('navigator:browser');
            }

            // Validate window
            if (!nativeWindow) {
              return {
                success: false,
                error: { code: 5002, message: 'No browser window found', recoverable: false },
              };
            }

            // Validate gBrowser
            if (!nativeWindow.gBrowser) {
              return {
                success: false,
                error: { code: 5003, message: 'Window has no gBrowser', recoverable: false },
              };
            }

            // Create the tab
            let tab;
            try {
              tab = nativeWindow.gBrowser.addTab(url || 'about:newtab', {
                triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
              });
            } catch (e) {
              return {
                success: false,
                error: { code: 6002, message: `Failed to add tab: ${e}`, recoverable: false },
              };
            }

            if (!tab) {
              return {
                success: false,
                error: { code: 6003, message: 'addTab returned null', recoverable: false },
              };
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
                  tab.setAttribute('zen-workspace-id', gZenWorkspaces.activeWorkspace);
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
                return {
                  success: false,
                  error: { code: 6004, message: 'Tab tracker unavailable', recoverable: false },
                };
              }
              tabId = tabTracker.getId(tab);
            } catch (e) {
              return {
                success: false,
                error: { code: 6005, message: `Failed to get tab ID: ${e}`, recoverable: false },
              };
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
              if (tab && typeof tab._tPos === 'number') {
                tabPosition = tab._tPos;
              }
            } catch (e) {
              // Tab might be in invalid state, use default
            }

            return {
              success: true,
              tab: {
                id: tabId,
                url: url || 'about:newtab',
                title: '',
                active,
                index: tabPosition,
                windowId: resolvedWindowId,
                status: 'loading',
              },
            };
          } catch (e) {
            // Catch-all for any unexpected errors
            return {
              success: false,
              error: {
                code: 6000,
                message: `Unexpected error in createTab: ${e}`,
                recoverable: false,
              },
            };
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
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const win = nativeTab.ownerGlobal;
          if (!win || !win.gBrowser) {
            return {
              success: false,
              error: {
                code: 6007,
                message: "Tab's window is no longer available",
                recoverable: false,
              },
            };
          }

          const gBrowser = win.gBrowser;

          // Ensure tab has zen-workspace-id for Zen compatibility
          if (!nativeTab.hasAttribute('zen-workspace-id')) {
            try {
              const gZenWorkspaces = win.gZenWorkspaces;
              if (gZenWorkspaces && gZenWorkspaces.activeWorkspace) {
                nativeTab.setAttribute('zen-workspace-id', gZenWorkspaces.activeWorkspace);
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
              const tabActuallyClosed =
                nativeTab.closing || !nativeTab.parentNode || !gBrowser.tabs.includes(nativeTab);
              if (tabActuallyClosed) {
                resolve({ success: true });
              } else if (internalError) {
                resolve({
                  success: false,
                  error: {
                    code: 6002,
                    message: `closeTab error: ${internalError.message || internalError}`,
                    recoverable: false,
                  },
                });
              } else {
                resolve({
                  success: false,
                  error: { code: 6008, message: 'Tab was not closed', recoverable: false },
                });
              }
            }, 0);
          });
        },

        _getTabInfo(tab, tabId) {
          try {
            const nativeTab = tab.nativeTab;
            const browser = tab.browser;
            let windowId = 0;
            try {
              windowId = extension.windowManager.getWrapper(nativeTab.ownerGlobal)?.id || 0;
            } catch (e) {
              // ownerGlobal may be a dead wrapper
            }
            let active = false;
            try {
              active = nativeTab === nativeTab.ownerGlobal?.gBrowser?.selectedTab;
            } catch (e) {
              // ownerGlobal may be unavailable
            }
            return {
              id: tabId,
              zenSyncId: nativeTab.id || null, // Zen Browser's persistent tab ID for session association
              url: browser?.currentURI?.spec || '',
              title: browser?.contentTitle || '',
              active,
              index: nativeTab._tPos ?? 0,
              windowId,
              status: nativeTab.linkedBrowser?.webProgress?.isLoadingDocument
                ? 'loading'
                : 'complete',
            };
          } catch (e) {
            // Return minimal info on failure to avoid crashing the entire listTabs call
            return {
              id: tabId,
              url: '',
              title: '',
              active: false,
              index: 0,
              windowId: 0,
              status: 'unknown',
            };
          }
        },

        async getTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          return this._getTabInfo(tab, resolvedTabId);
        },

        async listTabs(windowId) {
          // Get target window - use specified windowId or fall back to current/top window
          let win;
          try {
            if (windowId !== undefined) {
              const wrapper = extension.windowManager.get(windowId, extension.context);
              if (!wrapper) {
                return [];
              }
              win = wrapper;
            } else {
              const topWin = extension.windowManager.topWindow;
              if (!topWin) {
                return [];
              }
              win = extension.windowManager.getWrapper(topWin);
            }
          } catch (e) {
            return [];
          }

          if (!win?.window?.gBrowser?.tabs) {
            return [];
          }

          const tabs = [];
          for (const nativeTab of win.window.gBrowser.tabs) {
            try {
              const tabId = tabTracker.getId(nativeTab);
              const tab = extension.tabManager.get(tabId);
              if (tab) {
                tabs.push(this._getTabInfo(tab, tabId));
              }
            } catch (e) {
              // Skip tabs that can't be queried (e.g., closing or dead wrappers)
            }
          }
          return tabs;
        },

        async queryTabs(filter = {}) {
          const allTabs = await this.listTabs();

          return allTabs.filter((tab) => {
            if (filter.active !== undefined && tab.active !== filter.active) return false;
            if (filter.windowId !== undefined && tab.windowId !== filter.windowId) return false;
            if (filter.url) {
              // Escape regex special chars first, then convert wildcards to regex
              const escaped = escapeRegExp(filter.url);
              const pattern = escaped.replace(/\\\*/g, '.*');
              const regex = new RegExp(`^${pattern}$`);
              if (!regex.test(tab.url)) return false;
            }
            if (filter.title) {
              // Escape regex special chars first, then convert wildcards to regex
              const escaped = escapeRegExp(filter.title);
              const pattern = escaped.replace(/\\\*/g, '.*');
              const regex = new RegExp(`^${pattern}$`, 'i');
              if (!regex.test(tab.title)) return false;
            }
            return true;
          });
        },

        async activateTab(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          try {
            const nativeTab = tab.nativeTab;
            const win = nativeTab.ownerGlobal;
            win.gBrowser.selectedTab = nativeTab;
            win.focus();
            return { success: true };
          } catch (e) {
            return {
              success: false,
              error: { code: 5001, message: String(e), recoverable: false },
            };
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
                'chrome://browser/content/browser.xhtml',
                '_blank',
                `chrome,dialog=no,all,private${features.length ? ',' + features.join(',') : ''}`,
                null
              );
            } else {
              newWindow = Services.ww.openWindow(
                null,
                'chrome://browser/content/browser.xhtml',
                '_blank',
                `chrome,dialog=no,all${features.length ? ',' + features.join(',') : ''}`,
                null
              );
            }

            await new Promise((resolve) => {
              newWindow.addEventListener('load', resolve, { once: true });
            });

            if (url) {
              newWindow.gBrowser.loadURI(Services.io.newURI(url), {
                triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
              });
            }

            const windowId = extension.windowManager.getWrapper(newWindow)?.id || 0;
            return { success: true, windowId };
          } catch (e) {
            return {
              success: false,
              error: { code: 5001, message: String(e), recoverable: false },
            };
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
              return {
                success: false,
                error: { code: 5001, message: 'Window not found', recoverable: false },
              };
            }

            targetWindow.close();
            return { success: true };
          } catch (e) {
            return {
              success: false,
              error: { code: 5001, message: String(e), recoverable: false },
            };
          }
        },

        // ========== Wait (browser_use mode) ==========

        async waitForSelector(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'waitForSelector', {
            selector,
            ...options,
          });
        },

        async waitForTimeout(ms) {
          // In extension parent context, use lazy getter for ChromeUtils
          const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
            'resource://gre/modules/Timer.sys.mjs'
          );
          await new Promise((resolve) => chromeSetTimeout(resolve, ms));
          return { success: true };
        },

        // ========== Keyboard/Mouse Control ==========

        async keyPress(tabId, key, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'keyPress', {
            key,
            ...options,
          });
        },

        async keyDown(tabId, key, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'keyDown', {
            key,
            ...options,
          });
        },

        async keyUp(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'keyUp', { key });
        },

        async mouseMove(tabId, x, y, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'mouseMove', {
            x,
            y,
            ...options,
          });
        },

        async mouseDown(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'mouseDown', options);
        },

        async mouseUp(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'mouseUp', options);
        },

        async wheel(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'wheel', options);
        },

        async scroll(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'scroll', options);
        },

        async waitForStable(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'waitForStable', options);
        },

        async dblclick(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'dblclick', {
            selector,
            ...options,
          });
        },

        async drag(tabId, fromSelector, toSelector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'drag', {
            fromSelector,
            toSelector,
            ...options,
          });
        },

        async focus(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'focus', { selector });
        },

        async clear(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'clear', { selector });
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
                return {
                  success: false,
                  error: {
                    code: 7002,
                    message: `Invalid URL in filter: ${filter.url}`,
                    recoverable: false,
                  },
                };
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
                sameSite: ['none', 'lax', 'strict'][cookie.sameSite] || 'none',
                expirationDate: cookie.expiry,
                session: cookie.isSession,
              });
            }

            return cookies;
          } catch (e) {
            return {
              success: false,
              error: { code: 7001, message: String(e), recoverable: false },
            };
          }
        },

        async setCookie(cookie) {
          try {
            const {
              url,
              name,
              value,
              domain,
              path = '/',
              secure = false,
              httpOnly = false,
              sameSite = 'lax',
              expirationDate,
            } = cookie;

            // Validate required parameters
            if (!url) {
              return {
                success: false,
                error: {
                  code: 7002,
                  message: 'Missing required parameter: url',
                  recoverable: false,
                },
              };
            }
            if (!name) {
              return {
                success: false,
                error: {
                  code: 7002,
                  message: 'Missing required parameter: name',
                  recoverable: false,
                },
              };
            }
            if (value === undefined || value === null) {
              return {
                success: false,
                error: {
                  code: 7002,
                  message: 'Missing required parameter: value',
                  recoverable: false,
                },
              };
            }

            let parsedUrl;
            try {
              parsedUrl = new URL(url);
            } catch (urlError) {
              return {
                success: false,
                error: { code: 7002, message: `Invalid URL: ${url}`, recoverable: false },
              };
            }

            const cookieDomain = domain || parsedUrl.hostname;
            const sameSiteMap = { none: 0, lax: 1, strict: 2 };

            Services.cookies.add(
              cookieDomain,
              path,
              name,
              value,
              secure,
              httpOnly,
              !expirationDate, // isSession
              expirationDate || Math.floor(Date.now() / 1000) + 86400 * 365,
              {}, // originAttributes
              sameSiteMap[sameSite] || 1,
              Ci.nsICookie.SCHEME_HTTPS
            );

            return { success: true };
          } catch (e) {
            return {
              success: false,
              error: { code: 7001, message: String(e), recoverable: false },
            };
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
            return {
              success: false,
              error: { code: 5001, message: String(e), recoverable: false },
            };
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
            return {
              success: false,
              error: { code: 5001, message: String(e), recoverable: false },
            };
          }
        },

        // ========== Storage Management ==========

        async getLocalStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'getLocalStorage', { key });
        },

        async setLocalStorage(tabId, key, value) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'setLocalStorage', {
            key,
            value,
          });
        },

        async removeLocalStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'removeLocalStorage', {
            key,
          });
        },

        async clearLocalStorage(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'clearLocalStorage', {});
        },

        async getSessionStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'getSessionStorage', {
            key,
          });
        },

        async setSessionStorage(tabId, key, value) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'setSessionStorage', {
            key,
            value,
          });
        },

        async removeSessionStorage(tabId, key) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'removeSessionStorage', {
            key,
          });
        },

        async clearSessionStorage(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'clearSessionStorage', {});
        },

        // ========== Network ==========

        async startCapture(options = {}) {
          const handle = `capture_${++captureCounter}`;
          const captureData = {
            options,
            requests: [],
            listener: null,
            createdAt: Date.now(),
          };

          const { urlPattern, resourceTypes, recordBody: _recordBody = false } = options;

          // Create webRequest listener
          const listener = (details) => {
            // Check URL pattern
            if (urlPattern) {
              const escaped = escapeRegExp(urlPattern);
              const pattern = escaped.replace(/\\\*/g, '.*');
              const regex = new RegExp(`^${pattern}$`);
              if (!regex.test(details.url)) return;
            }

            // Check resource type
            if (resourceTypes && !resourceTypes.includes(details.type)) return;

            captureData.requests.push({
              url: details.url,
              method: details.method,
              resourceType: details.type,
              headers: details.requestHeaders
                ? Object.fromEntries(details.requestHeaders.map((h) => [h.name, h.value]))
                : {},
              timestamp: Date.now(),
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
            return {
              success: false,
              error: { code: 8001, message: 'Capture not found', recoverable: false },
            };
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
            return {
              success: false,
              error: { code: 8001, message: 'Capture not found', recoverable: false },
            };
          }
          return [...captureData.requests];
        },

        async intercept(options) {
          const handle = `intercept_${++interceptCounter}`;
          // eslint-disable-next-line no-unused-vars
          const { urlPattern, handler, mockResponse, modifyHeaders, resourceTypes } = options;

          const interceptData = {
            options,
            active: true,
          };

          // Note: Full implementation would use browser.webRequest.onBeforeRequest
          // with blocking: true. For now, store configuration.
          networkIntercepts.set(handle, interceptData);

          return handle;
        },

        async removeIntercept(handle) {
          if (!networkIntercepts.has(handle)) {
            return {
              success: false,
              error: { code: 8002, message: 'Intercept not found', recoverable: false },
            };
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
            'resource://gre/modules/Timer.sys.mjs'
          );

          const startTime = Date.now();
          while (Date.now() - startTime < timeout) {
            const requests = await this.getCaptures(handle);
            if (requests.length > 0) {
              await this.stopCapture(handle);
              return requests[0];
            }
            await new Promise((resolve) => {
              chromeSetTimeout(resolve, 100);
            });
          }

          await this.stopCapture(handle);
          return {
            success: false,
            error: { code: 8003, message: 'Timeout waiting for request', recoverable: true },
          };
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
          if (!script || typeof script !== 'string') {
            return {
              success: false,
              error: {
                code: 9002,
                message: 'Missing or invalid required parameter: script',
                recoverable: false,
              },
            };
          }
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'eval', {
            script,
            ...options,
          });
        },

        async addScript(tabId, script, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'addScript', {
            script,
            ...options,
          });
        },

        async removeScript(tabId, handle) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'removeScript', { handle });
        },

        // ========== Frame Management ==========

        async listFrames(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'listFrames', {});
        },

        async switchFrame(tabId, selector) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'switchFrame', {
            selector,
          });
        },

        async frameMain(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTabWithRestore(resolvedTabId, extension, 'frameMain', {});
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
            const actor = tab.browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            return actor.sendQuery('dialogAccept', { text });
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
            const actor = tab.browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            return actor.sendQuery('dialogDismiss', {});
          } catch (e) {
            return { success: true };
          }
        },

        // ========== Download Handling ==========

        async waitForDownload(options = {}) {
          const timeout = options.timeout || 30000;

          // Import timer functions for extension parent context
          const { setTimeout: chromeSetTimeout, clearTimeout: chromeClearTimeout } =
            ChromeUtils.importESModule('resource://gre/modules/Timer.sys.mjs');

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
                  Services.obs.removeObserver(downloadListener, 'dl-start');
                } catch (e) {
                  // Listener already removed
                }
              }
            };

            timeoutId = chromeSetTimeout(() => {
              cleanup();
              resolve({
                success: false,
                error: { code: 12001, message: 'Download timeout', recoverable: true },
              });
            }, timeout);

            downloadListener = {
              observe: (subject, topic, _data) => {
                if (topic === 'dl-start') {
                  cleanup();

                  try {
                    // subject is nsIDownload
                    const download = subject.QueryInterface(Ci.nsIDownload);
                    resolve({
                      success: true,
                      url: download.source?.spec || '',
                      filename: download.targetFile?.leafName || '',
                      mimeType: download.MIMEInfo?.MIMEType || '',
                      size: download.totalBytes || -1,
                    });
                  } catch (e) {
                    // Fallback for different Firefox versions
                    resolve({
                      success: true,
                      url: '',
                      filename: '',
                      mimeType: '',
                      size: -1,
                    });
                  }
                }
              },
            };

            try {
              Services.obs.addObserver(downloadListener, 'dl-start');
            } catch (e) {
              cleanup();
              resolve({
                success: false,
                error: {
                  code: 12001,
                  message: `Failed to observe downloads: ${e}`,
                  recoverable: false,
                },
              });
            }
          });
        },

        // ========== Tab Content & State (Sidebar Bridge) ==========

        async getTabContent(tabId, options = {}) {
          const {
            format = 'markdown',
            selector = null,
            autoRestore = true,
            timeout = DEFAULT_RESTORE_TIMEOUT,
          } = options;

          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const nativeTab = tab.nativeTab;
          const wasDiscarded = self.isTabDiscarded(nativeTab);

          // Auto-restore discarded tabs if needed
          if (wasDiscarded && autoRestore) {
            const restored = await self.restoreTabIfNeeded(nativeTab, timeout);
            if (!restored) {
              return {
                success: false,
                error: {
                  code: 5005,
                  message: 'Failed to restore discarded tab',
                  recoverable: true,
                },
              };
            }

            // Wait for browsingContext to be ready
            const browser = tab.browser || nativeTab.linkedBrowser;
            const bc = await self.waitForBrowsingContext(browser, 2000);
            if (!bc?.currentWindowGlobal) {
              return {
                success: false,
                error: {
                  code: 5006,
                  message: 'Tab restored but browsingContext not ready',
                  recoverable: true,
                },
              };
            }
          }

          // Get content based on format
          let content = '';
          let title = '';
          let url = '';

          if (format === 'markdown') {
            const result = await self.executeInTab(resolvedTabId, extension, 'getMarkdown', {
              selector,
            });
            if (result.success === false) {
              return result;
            }
            content = result.markdown || '';
            title = result.title || '';
            url = result.url || '';
          } else if (format === 'html') {
            const result = await self.executeInTab(resolvedTabId, extension, 'getHtml', {
              selector: selector || 'body',
            });
            content = typeof result === 'string' ? result : result || '';
            const browser = tab.browser || nativeTab.linkedBrowser;
            title = browser?.contentTitle || '';
            url = browser?.currentURI?.spec || '';
          } else if (format === 'text') {
            const result = await self.executeInTab(resolvedTabId, extension, 'getText', {
              selector: selector || 'body',
            });
            content = typeof result === 'string' ? result : result || '';
            const browser = tab.browser || nativeTab.linkedBrowser;
            title = browser?.contentTitle || '';
            url = browser?.currentURI?.spec || '';
          } else {
            return {
              success: false,
              error: { code: 4001, message: `Unsupported format: ${format}`, recoverable: false },
            };
          }

          return {
            tabId: resolvedTabId,
            url,
            title,
            content,
            format,
            extractedAt: Date.now(),
            wasDiscarded,
          };
        },

        async getTabState(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const nativeTab = tab.nativeTab;
          const browser = tab.browser || nativeTab.linkedBrowser;
          const discarded = self.isTabDiscarded(nativeTab);

          // Determine loading status
          let status = 'unloaded';
          if (!discarded && browser) {
            const webProgress = browser.webProgress;
            if (webProgress?.isLoadingDocument) {
              status = 'loading';
            } else if (browser.browsingContext?.currentWindowGlobal) {
              status = 'complete';
            }
          }

          return {
            discarded,
            status,
            url: browser?.currentURI?.spec || '',
            title: browser?.contentTitle || nativeTab.label || '',
          };
        },

        async pickElement(tabId, options = {}) {
          const { filter = 'any', timeout = 60000, highlightColor = '#6366f1' } = options;

          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const nativeTab = tab.nativeTab;

          // Restore tab if needed
          await self.restoreTabIfNeeded(nativeTab, 30000);

          const browser = tab.browser || nativeTab.linkedBrowser;
          if (!browser?.browsingContext?.currentWindowGlobal) {
            return {
              success: false,
              error: { code: 5002, message: 'Tab not fully loaded', recoverable: true },
            };
          }

          try {
            const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');

            // Start picker with timeout
            const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
              'resource://gre/modules/Timer.sys.mjs'
            );

            const timeoutPromise = new Promise((_, reject) => {
              chromeSetTimeout(() => reject(new Error('Picker timeout')), timeout);
            });

            const pickerPromise = actor.sendQuery('startPicker', {
              filter,
              highlightColor,
            });

            const result = await Promise.race([pickerPromise, timeoutPromise]);
            if (!result.success) {
              return {
                success: false,
                error: { code: 10001, message: result.error || 'Picker failed', recoverable: true },
              };
            }
            return result.data;
          } catch (e) {
            // Ensure picker is stopped on error
            try {
              const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
              await actor.sendQuery('stopPicker', {});
            } catch {}
            return {
              success: false,
              error: { code: 10001, message: e.message, recoverable: true },
            };
          }
        },

        async cancelPicker(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const browser = tab.browser || tab.nativeTab.linkedBrowser;
          if (!browser?.browsingContext?.currentWindowGlobal) {
            return { success: true }; // No picker to cancel
          }

          try {
            const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            await actor.sendQuery('stopPicker', {});
            return { success: true };
          } catch (e) {
            return { success: true }; // Picker already stopped
          }
        },

        async getSelection(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return null;
          }

          const nativeTab = tab.nativeTab;
          if (self.isTabDiscarded(nativeTab)) {
            return null;
          }

          const browser = tab.browser || nativeTab.linkedBrowser;
          if (!browser?.browsingContext?.currentWindowGlobal) {
            return null;
          }

          try {
            const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            const result = await actor.sendQuery('getSelection', {});
            return result.success ? result.data : null;
          } catch (e) {
            return null;
          }
        },

        async lockPage(tabId, options = {}) {
          const { showOverlay = true, message = '' } = options;

          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const browser = tab.browser || tab.nativeTab.linkedBrowser;
          if (!browser?.browsingContext?.currentWindowGlobal) {
            return {
              success: false,
              error: { code: 5002, message: 'Tab not fully loaded', recoverable: true },
            };
          }

          try {
            const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            await actor.sendQuery('lockPage', { showOverlay, message });
            return { success: true };
          } catch (e) {
            return {
              success: false,
              error: { code: 11001, message: e.message, recoverable: false },
            };
          }
        },

        async unlockPage(tabId) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab) {
            return {
              success: false,
              error: { code: 3001, message: 'Tab not found', recoverable: false },
            };
          }

          const browser = tab.browser || tab.nativeTab.linkedBrowser;
          if (!browser?.browsingContext?.currentWindowGlobal) {
            return { success: true }; // Nothing to unlock
          }

          try {
            const actor = browser.browsingContext.currentWindowGlobal.getActor('Nevoflux');
            await actor.sendQuery('unlockPage', {});
            return { success: true };
          } catch (e) {
            return { success: true }; // Already unlocked
          }
        },

        // ========== Artifact Management ==========

        async createArtifact({
          id,
          type,
          title,
          code,
          files,
          entry: entryPoint,
          options,
          state,
          source,
          permissions,
        }) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          console.log(
            `[ext-nevoflux] createArtifact: id=${id}, type=${type}, codeLen=${code?.length}, filesCount=${files ? Object.keys(files).length : 0}, entry=${entryPoint}, state=${state}`
          );
          const now = Date.now();
          const entry = {
            type: type || 'html',
            title: title || 'Untitled',
            content: code || '',
            state: state || 'streaming',
            source: source || 'agent',
            permissions: permissions || [],
            createdAt: now,
            updatedAt: now,
          };
          if (files) entry.files = files;
          if (entryPoint) entry.entry = entryPoint;
          if (options) entry.options = options;
          console.log(
            `[ext-nevoflux] createArtifact SET: key=canvas:${id}, type=${entry.type}, contentLen=${entry.content?.length}, hasFiles=${!!entry.files}, entry=${entry.entry}`
          );
          NevofluxContentStore.set(`canvas:${id}`, entry);

          // Tab opening is now handled by background.js for foreground + reuse

          return { success: true, id };
        },

        async updateArtifact(
          id,
          { code, state, title, type: artifactType, files, entry: entryPoint }
        ) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          const existing = NevofluxContentStore.get(`canvas:${id}`);
          if (!existing) {
            console.log(
              `[ext-nevoflux] updateArtifact: id=${id} NOT FOUND, state=${state}, type=${artifactType}`
            );
            return {
              success: false,
              error: { code: 12001, message: 'Artifact not found', recoverable: false },
            };
          }

          console.log(
            `[ext-nevoflux] updateArtifact: id=${id}, existingType=${existing.type}, newState=${state}, newType=${artifactType}, newCodeLen=${code?.length}, newFiles=${files ? Object.keys(files).length : 0}`
          );
          if (code != null) existing.content = code;
          if (state != null) existing.state = state;
          if (title != null) existing.title = title;
          if (artifactType != null) existing.type = artifactType;
          if (files != null) existing.files = files;
          if (entryPoint != null) existing.entry = entryPoint;
          existing.updatedAt = Date.now();

          NevofluxContentStore.set(`canvas:${id}`, existing);
          return { success: true };
        },

        async getArtifact(id) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          const entry = NevofluxContentStore.get(`canvas:${id}`);
          return entry
            ? { success: true, data: entry }
            : {
                success: false,
                error: { code: 12001, message: 'Artifact not found', recoverable: false },
              };
        },

        async readArtifact(id, params = {}) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          const entry = NevofluxContentStore.get(`canvas:${id}`);
          if (!entry) {
            return {
              success: false,
              error: { code: 12001, message: 'Artifact not found', recoverable: false },
            };
          }

          const content = entry.content || '';
          const allLines = content.split('\n');
          const totalLines = allLines.length;
          const MAX_LINES = 500;

          // grep mode: find matching lines with context
          if (params.grep) {
            const ctxLines = params.context || 5;
            const matchIndices = [];
            const needle = params.grep.toLowerCase();
            for (let i = 0; i < allLines.length; i++) {
              if (allLines[i].toLowerCase().includes(needle)) {
                matchIndices.push(i);
              }
            }
            if (matchIndices.length === 0) {
              return {
                success: true,
                content: '',
                totalLines,
                matches: 0,
                truncated: false,
                title: entry.title,
                type: entry.type,
              };
            }
            // Collect unique line ranges around matches
            const lineSet = new Set();
            for (const idx of matchIndices) {
              for (
                let j = Math.max(0, idx - ctxLines);
                j <= Math.min(allLines.length - 1, idx + ctxLines);
                j++
              ) {
                lineSet.add(j);
              }
            }
            const sortedLines = [...lineSet].sort((a, b) => a - b);
            const sections = [];
            let prev = -2;
            for (const ln of sortedLines) {
              if (ln !== prev + 1 && sections.length > 0) {
                sections.push('...');
              }
              sections.push(`${ln + 1}\t${allLines[ln]}`);
              prev = ln;
            }
            return {
              success: true,
              content: sections.join('\n'),
              totalLines,
              matches: matchIndices.length,
              truncated: false,
              title: entry.title,
              type: entry.type,
            };
          }

          // offset/limit mode
          if (params.offset || params.limit) {
            const offset = Math.max(0, (params.offset || 1) - 1); // 1-based to 0-based
            const limit = params.limit || MAX_LINES;
            const sliced = allLines.slice(offset, offset + limit);
            const numbered = sliced.map((line, i) => `${offset + i + 1}\t${line}`);
            return {
              success: true,
              content: numbered.join('\n'),
              totalLines,
              truncated: offset + limit < totalLines,
              title: entry.title,
              type: entry.type,
            };
          }

          // Full read with auto-truncation
          if (totalLines > MAX_LINES) {
            const numbered = allLines.slice(0, MAX_LINES).map((line, i) => `${i + 1}\t${line}`);
            return {
              success: true,
              content:
                numbered.join('\n') +
                `\n\n[Truncated at line ${MAX_LINES} of ${totalLines}. Use offset/limit or grep to read more.]`,
              totalLines,
              truncated: true,
              title: entry.title,
              type: entry.type,
            };
          }

          return {
            success: true,
            content,
            totalLines,
            truncated: false,
            title: entry.title,
            type: entry.type,
          };
        },

        async editArtifact(id, oldStr, newStr) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          const entry = NevofluxContentStore.get(`canvas:${id}`);
          if (!entry) {
            return {
              success: false,
              error: { code: 12001, message: 'Artifact not found', recoverable: false },
            };
          }
          if (entry.state === 'streaming') {
            return {
              success: false,
              error: {
                code: 12004,
                message: 'Artifact is still generating. Wait for completion.',
                recoverable: true,
              },
            };
          }

          const content = entry.content || '';
          const count = content.split(oldStr).length - 1;

          if (count === 0) {
            return {
              success: false,
              error: {
                code: 12005,
                message:
                  'old_str not found in artifact. Use browser_read_artifact to verify the current content.',
                recoverable: true,
              },
            };
          }
          if (count > 1) {
            return {
              success: false,
              error: {
                code: 12006,
                message: `old_str matches ${count} locations. Provide more surrounding context to make it unique.`,
                recoverable: true,
              },
            };
          }

          entry.content = content.replace(oldStr, newStr);
          entry.updatedAt = Date.now();
          NevofluxContentStore.set(`canvas:${id}`, entry);

          return { success: true, lines: entry.content.split('\n').length };
        },

        async deleteArtifact(id) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          NevofluxContentStore.delete(`canvas:${id}`);
          return { success: true };
        },

        async listArtifacts() {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          const entries = NevofluxContentStore.query('canvas:');
          return {
            success: true,
            data: entries.map((e) => ({ id: e.key.replace('canvas:', ''), ...e.value })),
          };
        },

        async openCanvasTab(id, options) {
          const opts = options || {};
          const win = Services.wm.getMostRecentBrowserWindow();
          if (!win?.gBrowser) {
            return {
              success: false,
              error: { code: 12003, message: 'No browser window', recoverable: true },
            };
          }
          const url = `nevoflux://canvas/${id}`;
          const nativeTab = win.gBrowser.addTab(url, {
            triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
            inBackground: !!opts.inBackground,
          });
          if (!opts.inBackground) {
            win.gBrowser.selectedTab = nativeTab;
          }
          // Return WebExtension tab ID for tracking
          let extTabId;
          try {
            extTabId = tabTracker.getId(nativeTab);
          } catch (e) {
            console.warn('[ext-nevoflux] openCanvasTab: could not get tab ID:', e);
          }
          return { success: true, id, tabId: extTabId };
        },

        // ========== Page Navigation ==========

        async openPage(url, options) {
          const opts = options || {};
          const win = Services.wm.getMostRecentBrowserWindow();
          if (!win?.gBrowser) {
            return {
              success: false,
              error: { code: 12003, message: 'No browser window', recoverable: true },
            };
          }
          const nativeTab = win.gBrowser.addTab(url, {
            triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
            inBackground: !!opts.inBackground,
          });
          if (!opts.inBackground) {
            win.gBrowser.selectedTab = nativeTab;
          }
          let extTabId;
          try {
            extTabId = tabTracker.getId(nativeTab);
          } catch (e) {
            console.warn('[ext-nevoflux] openPage: could not get tab ID:', e);
          }
          return { success: true, tabId: extTabId };
        },

        // ========== Settings ==========

        async getSettings(key) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          return { success: true, data: NevofluxContentStore.get(`config:${key}`) };
        },

        async setSettings(key, value) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          NevofluxContentStore.set(`config:${key}`, value);
          return { success: true };
        },

        // ========== ContentStore Persistence ==========

        async contentStoreLoad(entries) {
          const { NevofluxContentStore } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxContentStore.sys.mjs'
          );
          console.log(`[ext-nevoflux] contentStoreLoad: ${entries.length} entries`);
          NevofluxContentStore._loading = true;
          try {
            for (const entry of entries) {
              if (entry.key && entry.value !== undefined) {
                // Skip canvas artifact entries entirely during load.
                // Canvas artifacts are session-specific — they're created by the
                // streaming protocol (ARTIFACT_START/DELTA/COMPLETE) and the
                // create_artifact tool call. Loading stale persisted data would
                // overwrite correct data due to timing races.
                if (entry.key.startsWith('canvas:')) {
                  console.log(
                    `[ext-nevoflux] contentStoreLoad: SKIPPING canvas entry ${entry.key} (type=${entry.value?.type}, state=${entry.value?.state})`
                  );
                  continue;
                }
                NevofluxContentStore.set(entry.key, entry.value);
              }
            }
          } finally {
            NevofluxContentStore._loading = false;
          }
          return { success: true };
        },

        // ========== Bridge (nevoflux:// pages → background) ==========

        async bridgeRespond(id, result) {
          const { NevofluxBridgeRouter } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxBridgeRouter.sys.mjs'
          );
          NevofluxBridgeRouter.respond(id, result);
          return { success: true };
        },

        async bridgePush(sessionId, message) {
          const { NevofluxBridgeRouter } = ChromeUtils.importESModule(
            'resource:///modules/NevofluxBridgeRouter.sys.mjs'
          );
          const delivered = NevofluxBridgeRouter.push(sessionId, message);
          return { success: true, delivered };
        },

        // ========== Sidebar Layout ==========

        async setSidebarWidth(width) {
          const win = Services.wm.getMostRecentWindow('navigator:browser');
          if (!win) {
            return {
              success: false,
              error: { code: -1, message: 'No browser window', recoverable: true },
            };
          }
          const sidebarBox = win.document.getElementById('sidebar-box');
          if (sidebarBox) {
            sidebarBox.style.setProperty('width', width + 'px', 'important');
            sidebarBox.style.setProperty('min-width', width + 'px', 'important');
            sidebarBox.style.setProperty('max-width', width + 'px', 'important');
          }
          return { success: true };
        },

        onBridgeRequest: new EventManager({
          context,
          module: 'nevoflux',
          event: 'onBridgeRequest',
          register: (fire) => {
            const { NevofluxBridgeRouter } = ChromeUtils.importESModule(
              'resource:///modules/NevofluxBridgeRouter.sys.mjs'
            );
            const handler = (id, type, payload) => {
              fire.async(id, type, payload);
            };
            NevofluxBridgeRouter.setHandler(handler);
            return () => {
              NevofluxBridgeRouter.removeHandler();
            };
          },
        }).api(),

        onContentStoreChanged: new EventManager({
          context,
          module: 'nevoflux',
          event: 'onContentStoreChanged',
          register: (fire) => {
            const { NevofluxContentStore } = ChromeUtils.importESModule(
              'resource:///modules/NevofluxContentStore.sys.mjs'
            );
            const unsubscribe = NevofluxContentStore.onPersist((op, key, value) => {
              fire.async(op, key, value);
            });
            contentStorePersistUnsubscribe = unsubscribe;
            return () => {
              unsubscribe();
              contentStorePersistUnsubscribe = null;
            };
          },
        }).api(),
      },
    };
  }

  // ========== Helper Methods ==========

  getActiveTabId(_extension) {
    // tabTracker is a global from ext-browser.js
    const activeTab = tabTracker?.activeTab;
    if (!activeTab) {
      return null;
    }
    return tabTracker.getId(activeTab);
  }

  /**
   * Check if a tab is discarded (has "pending" attribute)
   * @param {object} nativeTab - The native tab element
   * @returns {boolean} True if tab is discarded
   */
  isTabDiscarded(nativeTab) {
    if (!nativeTab) {
      return false;
    }
    return nativeTab.hasAttribute('pending');
  }

  /**
   * Restore a discarded tab using SessionStore.restoreTabContent
   * This does NOT switch the visible tab - it restores the tab in the background.
   * @param {object} nativeTab - The native tab element
   * @param {number} timeout - Maximum time to wait for restoration (ms)
   * @returns {Promise<boolean>} True if restoration successful
   */
  async restoreTabIfNeeded(nativeTab, timeout = DEFAULT_RESTORE_TIMEOUT) {
    if (!nativeTab) {
      return false;
    }

    // Check if tab is actually discarded
    if (!this.isTabDiscarded(nativeTab)) {
      return true; // Already restored
    }

    const { setTimeout: chromeSetTimeout, clearTimeout: chromeClearTimeout } =
      ChromeUtils.importESModule('resource://gre/modules/Timer.sys.mjs');

    return new Promise((resolve) => {
      let timeoutId;
      let pollId;
      let resolved = false;

      const cleanup = () => {
        if (resolved) return;
        resolved = true;
        if (timeoutId) chromeClearTimeout(timeoutId);
        if (pollId) nativeTab.ownerGlobal?.clearInterval(pollId);
        nativeTab.removeEventListener('SSTabRestored', onRestored);
      };

      const onRestored = () => {
        cleanup();
        resolve(true);
      };

      // Set up timeout
      timeoutId = chromeSetTimeout(() => {
        // Last chance: check if pending attribute was already removed
        if (!nativeTab.hasAttribute('pending')) {
          cleanup();
          resolve(true);
          return;
        }
        cleanup();
        resolve(false);
      }, timeout);

      // Listen for restoration complete
      nativeTab.addEventListener('SSTabRestored', onRestored);

      // Also poll for attribute removal as fallback (SSTabRestored may not fire)
      pollId = nativeTab.ownerGlobal?.setInterval?.(() => {
        if (!nativeTab.hasAttribute('pending')) {
          onRestored();
        }
      }, 200);

      // Trigger restoration (does NOT switch tabs)
      try {
        SessionStore.restoreTabContent(nativeTab);
      } catch (e) {
        console.warn('[ext-nevoflux] restoreTabContent failed, trying tab activation:', e);
        // Fallback: activate the tab to force load
        try {
          const tabbrowser = nativeTab.ownerGlobal?.gBrowser;
          if (tabbrowser) {
            tabbrowser.selectedTab = nativeTab;
          }
        } catch (e2) {
          cleanup();
          resolve(false);
        }
      }
    });
  }

  /**
   * Execute action in tab with auto-restore for discarded tabs
   * @param {number} tabId - The tab ID
   * @param {object} extension - The extension context
   * @param {string} action - The action to execute
   * @param {object} params - Action parameters
   * @param {object} options - Options including autoRestore and timeout
   * @returns {Promise<object>} Result from executeInTab
   */
  async executeInTabWithRestore(tabId, extension, action, params, options = {}) {
    const { autoRestore = true, timeout = DEFAULT_RESTORE_TIMEOUT } = options;

    const tab = extension.tabManager.get(tabId);
    if (!tab) {
      return {
        success: false,
        error: { code: 3001, message: 'Tab not found', recoverable: false },
      };
    }

    const nativeTab = tab.nativeTab;

    // Auto-restore discarded tabs if needed
    if (autoRestore && this.isTabDiscarded(nativeTab)) {
      console.log(
        `[ext-nevoflux] executeInTabWithRestore: Tab ${tabId} is discarded, restoring...`
      );
      const restored = await this.restoreTabIfNeeded(nativeTab, timeout);
      if (!restored) {
        return {
          success: false,
          error: { code: 5005, message: 'Failed to restore discarded tab', recoverable: true },
        };
      }

      // Wait for browsingContext to be ready
      const browser = tab.browser || nativeTab.linkedBrowser;
      const bc = await this.waitForBrowsingContext(browser, 2000);
      if (!bc?.currentWindowGlobal) {
        return {
          success: false,
          error: {
            code: 5006,
            message: 'Tab restored but browsingContext not ready',
            recoverable: true,
          },
        };
      }
      console.log(`[ext-nevoflux] executeInTabWithRestore: Tab ${tabId} restored successfully`);
    }

    return this.executeInTab(tabId, extension, action, params);
  }

  async executeInTab(tabId, extension, action, params) {
    console.log(
      `[ext-nevoflux] executeInTab: action=${action}, tabId=${tabId}, params=`,
      JSON.stringify(params)
    );
    const tab = extension.tabManager.get(tabId);
    if (!tab) {
      console.log('[ext-nevoflux] executeInTab: Tab not found');
      return {
        success: false,
        error: { code: 3001, message: 'Tab not found', recoverable: false },
      };
    }

    try {
      // Get the browser - try multiple approaches
      let browser = tab.browser || tab.linkedBrowser;
      const nativeTab = tab.nativeTab;

      console.log(`[ext-nevoflux] executeInTab: browser=${!!browser}, nativeTab=${!!nativeTab}`);

      // If no browser directly, try to get it from the native tab
      if (!browser && nativeTab) {
        browser = nativeTab.linkedBrowser;
        console.log(
          `[ext-nevoflux] executeInTab: got browser from nativeTab.linkedBrowser=${!!browser}`
        );
      }

      if (!browser) {
        console.log('[ext-nevoflux] executeInTab: No browser element found');
        return {
          success: false,
          error: { code: 3002, message: 'No browser element found', recoverable: false },
        };
      }

      let bc = browser.browsingContext;
      console.log(`[ext-nevoflux] executeInTab: initial bc=${!!bc}`);

      // If no browsingContext, the tab needs to be activated first
      if (!bc) {
        const isPending = nativeTab?.hasAttribute('pending');
        const isDiscarded = nativeTab?.hasAttribute('discarded');
        console.log(
          `[ext-nevoflux] executeInTab: No browsingContext, isPending=${isPending}, isDiscarded=${isDiscarded}`
        );

        return {
          success: false,
          error: {
            code: 5004,
            message: 'Tab not loaded',
            recoverable: true,
            reason: isPending ? 'pending' : isDiscarded ? 'discarded' : 'unknown',
            suggestion: 'Use activate_tab to load the tab first',
          },
        };
      }

      // Check if browsing context is discarded
      if (bc.isDiscarded) {
        console.log('[ext-nevoflux] executeInTab: BrowsingContext is discarded');
        return {
          success: false,
          error: { code: 5003, message: 'BrowsingContext is discarded', recoverable: false },
        };
      }

      // Strategy 1: Try currentWindowGlobal first (fast path for active tabs)
      let windowGlobal = bc.currentWindowGlobal;
      console.log(`[ext-nevoflux] executeInTab: bc=${!!bc}, cwg=${!!windowGlobal}`);

      // Strategy 2: If null, try getWindowGlobals() to find any valid WindowGlobal
      if (!windowGlobal) {
        console.log(
          '[ext-nevoflux] executeInTab: currentWindowGlobal is null, trying getWindowGlobals()'
        );
        const windowGlobals = bc.getWindowGlobals();
        console.log(`[ext-nevoflux] executeInTab: found ${windowGlobals.length} windowGlobals`);

        // Find a valid (non-closed, non-BFCache) window global
        windowGlobal = windowGlobals.find((wg) => !wg.isClosed && !wg.isInBFCache);

        if (windowGlobal) {
          console.log(`[ext-nevoflux] executeInTab: using windowGlobal from getWindowGlobals()`);
        }
      }

      // Strategy 3: If still null, wait briefly for tab transition
      if (!windowGlobal) {
        console.log('[ext-nevoflux] executeInTab: waiting for windowGlobal...');
        windowGlobal = await this.waitForWindowGlobal(bc, 500);
      }

      // Final check
      if (!windowGlobal) {
        console.log('[ext-nevoflux] executeInTab: No windowGlobal available after all strategies');
        return {
          success: false,
          error: {
            code: 5002,
            message: 'No windowGlobal available (tab may be unloaded)',
            recoverable: true,
            suggestion: 'Try activating the tab first',
          },
        };
      }

      const actor = windowGlobal.getActor('Nevoflux');
      console.log(`[ext-nevoflux] executeInTab: actor=${!!actor}, sending query...`);
      const result = await actor.sendQuery('execute', { action, params });
      console.log(`[ext-nevoflux] executeInTab: result=`, result);
      return result;
    } catch (e) {
      console.error(`[ext-nevoflux] executeInTab: Error - ${e.message}`, e.stack);
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }
  }

  async waitForWindowGlobal(bc, maxWaitMs = 500) {
    const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
      'resource://gre/modules/Timer.sys.mjs'
    );

    const waitInterval = 50;
    let waited = 0;

    while (waited < maxWaitMs) {
      await new Promise((resolve) => chromeSetTimeout(resolve, waitInterval));
      waited += waitInterval;

      // Check currentWindowGlobal first
      if (bc.currentWindowGlobal) {
        return bc.currentWindowGlobal;
      }

      // Also check getWindowGlobals()
      const windowGlobals = bc.getWindowGlobals();
      const validWg = windowGlobals.find((wg) => !wg.isClosed && !wg.isInBFCache);
      if (validWg) {
        return validWg;
      }
    }

    return null;
  }

  async waitForBrowsingContext(browser, maxWaitMs = 2000) {
    const { setTimeout: chromeSetTimeout } = ChromeUtils.importESModule(
      'resource://gre/modules/Timer.sys.mjs'
    );

    const waitInterval = 100;
    let waited = 0;

    while (waited < maxWaitMs) {
      if (browser.browsingContext) {
        console.log(`[ext-nevoflux] waitForBrowsingContext: got browsingContext after ${waited}ms`);
        return browser.browsingContext;
      }

      await new Promise((resolve) => chromeSetTimeout(resolve, waitInterval));
      waited += waitInterval;
    }

    console.log(`[ext-nevoflux] waitForBrowsingContext: timeout after ${maxWaitMs}ms`);
    return null;
  }

  waitForLoad(browser, waitUntil, timeout) {
    // Import timer functions for extension parent context
    const { setTimeout: chromeSetTimeout, clearTimeout: chromeClearTimeout } =
      ChromeUtils.importESModule('resource://gre/modules/Timer.sys.mjs');

    return new Promise((resolve, reject) => {
      const timeoutId = chromeSetTimeout(() => {
        cleanup();
        reject(new Error('Navigation timeout'));
      }, timeout);

      const listener = {
        onStateChange(webProgress, request, flags, _status) {
          const isStop = flags & Ci.nsIWebProgressListener.STATE_STOP;
          const isNetwork = flags & Ci.nsIWebProgressListener.STATE_IS_NETWORK;

          if (waitUntil === 'load' && isStop && isNetwork) {
            cleanup();
            resolve();
          }
        },
        QueryInterface: ChromeUtils.generateQI([
          'nsIWebProgressListener',
          'nsISupportsWeakReference',
        ]),
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
      result = result.replace(idRegex, '[ID_REDACTED]');
    }

    if (config.phone !== false) {
      // Use word boundary-like check to avoid matching inside other numbers
      const phoneRegex = /(?<!\d)1[3-9]\d{9}(?!\d)/g;
      const matches = result.match(phoneRegex) || [];
      filteredCount += matches.length;
      result = result.replace(phoneRegex, '[PHONE_REDACTED]');
    }

    if (config.email !== false) {
      const emailRegex = /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g;
      const matches = result.match(emailRegex) || [];
      filteredCount += matches.length;
      result = result.replace(emailRegex, '[EMAIL_REDACTED]');
    }

    return {
      text: result,
      filteredCount,
      filtered: filteredCount > 0,
    };
  }
};
