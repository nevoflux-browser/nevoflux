/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

// API version for compatibility checking
const API_VERSION = "1.0.0";

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

          const { timeout = 30000, waitUntil = "load" } = options;

          try {
            const loadPromise = self.waitForLoad(tab.browser, waitUntil, timeout);

            tab.browser.loadURI(Services.io.newURI(url), {
              triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal(),
            });

            await loadPromise;
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 2001, message: e.message, recoverable: true } };
          }
        },

        async reload(tabId, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          const tab = extension.tabManager.get(resolvedTabId);

          if (!tab?.browser) {
            return { success: false, error: { code: 3001, message: "Tab not found", recoverable: false } };
          }

          try {
            const { timeout = 30000, waitUntil = "load" } = options;
            const loadPromise = self.waitForLoad(tab.browser, waitUntil, timeout);
            tab.browser.reload();
            await loadPromise;
            return { success: true };
          } catch (e) {
            return { success: false, error: { code: 2001, message: e.message, recoverable: true } };
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

        // ========== Wait (browser_use mode) ==========

        async waitForSelector(tabId, selector, options = {}) {
          const resolvedTabId = tabId ?? (await self.getActiveTabId(extension));
          return self.executeInTab(resolvedTabId, extension, "waitForSelector", { selector, ...options });
        },

        async waitForTimeout(ms) {
          await new Promise(resolve => setTimeout(resolve, ms));
          return { success: true };
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
      },
    };
  }

  // ========== Helper Methods ==========

  async getActiveTabId(extension) {
    const tabs = await extension.tabManager.query({ active: true, currentWindow: true });
    return tabs[0]?.id;
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
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
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
        clearTimeout(timeoutId);
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

    if (config.phone !== false) {
      const phoneRegex = /1[3-9]\d{9}/g;
      const matches = result.match(phoneRegex) || [];
      filteredCount += matches.length;
      result = result.replace(phoneRegex, "[PHONE_REDACTED]");
    }

    if (config.idCard !== false) {
      const idRegex = /\d{17}[\dXx]/g;
      const matches = result.match(idRegex) || [];
      filteredCount += matches.length;
      result = result.replace(idRegex, "[ID_REDACTED]");
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
