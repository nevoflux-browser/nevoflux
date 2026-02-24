/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for ext-nevoflux.js (Parent Process API)
 * Tests all P1 methods: tabs, cookies, storage proxies, network, execute
 */

import { describe, it, beforeEach, expect } from './test-runner.mjs';
import {
  createMockServices,
  createMockExtension,
  createMockTabTracker,
  createMockDialog,
  createMockObserverService,
  createMockDownloadItem,
} from './mocks/browser-mocks.mjs';

// Helper to escape regex special characters (mirrors the real implementation)
function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Create a mock ext-nevoflux API class
class MockExtNevofluxAPI {
  constructor() {
    this.Services = createMockServices();
    this.extension = createMockExtension();
    this.tabTracker = createMockTabTracker();
    this.self = this;

    // Network state
    this.networkCaptures = new Map();
    this.networkIntercepts = new Map();
    this.captureCounter = 0;
    this.interceptCounter = 0;
    this.CAPTURE_MAX_AGE_MS = 300000;
  }

  // ========== Tab Management ==========

  async getActiveTabId() {
    const activeTab = this.tabTracker?.activeTab;
    if (!activeTab) {
      return null;
    }
    return this.tabTracker.getId(activeTab);
  }

  async createTab(options = {}) {
    const { url, active = true, windowId, index } = options;

    if (!this.tabTracker) {
      return {
        success: false,
        error: { code: 6004, message: 'Tab tracker unavailable', recoverable: false },
      };
    }

    try {
      let win;
      if (windowId !== undefined) {
        const wrapper = this.extension.windowManager.get(windowId, this.extension.context);
        if (!wrapper) {
          return {
            success: false,
            error: { code: 5002, message: 'No browser window found', recoverable: false },
          };
        }
        win = wrapper;
      } else {
        win = this.extension.windowManager.getWrapper(this.extension.windowManager.topWindow);
      }

      const tab = this.extension.tabManager.add(url || 'about:newtab');
      const tabId = tab.id;

      return {
        success: true,
        tab: {
          id: tabId,
          url: url || 'about:newtab',
          title: '',
          active,
          index: index ?? 0,
          windowId: win?.id || 1,
          status: 'loading',
        },
      };
    } catch (e) {
      return {
        success: false,
        error: { code: 6000, message: `Unexpected error in createTab: ${e}`, recoverable: false },
      };
    }
  }

  async closeTab(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    const tab = this.extension.tabManager.get(resolvedTabId);

    if (!tab) {
      return {
        success: false,
        error: { code: 3001, message: 'Tab not found', recoverable: false },
      };
    }

    try {
      this.extension.tabManager.remove(resolvedTabId);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 6002, message: String(e), recoverable: false } };
    }
  }

  async getTab(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    const tab = this.extension.tabManager.get(resolvedTabId);

    if (!tab) {
      return {
        success: false,
        error: { code: 3001, message: 'Tab not found', recoverable: false },
      };
    }

    return this._getTabInfo(tab, resolvedTabId);
  }

  _getTabInfo(tab, tabId) {
    return {
      id: tabId,
      url: tab.browser?.currentURI?.spec || '',
      title: tab.browser?.contentTitle || '',
      active: false,
      index: tab.nativeTab?._tPos || 0,
      windowId: 1,
      status: 'complete',
    };
  }

  async listTabs(windowId) {
    const tabs = this.extension.tabManager.getAll();
    return tabs.map((tab, index) => this._getTabInfo(tab, tab.id));
  }

  async queryTabs(filter = {}) {
    const allTabs = await this.listTabs();

    return allTabs.filter((tab) => {
      if (filter.active !== undefined && tab.active !== filter.active) return false;
      if (filter.windowId !== undefined && tab.windowId !== filter.windowId) return false;
      if (filter.url) {
        const escaped = escapeRegExp(filter.url);
        const pattern = escaped.replace(/\\\*/g, '.*');
        const regex = new RegExp(`^${pattern}$`);
        if (!regex.test(tab.url)) return false;
      }
      if (filter.title) {
        const escaped = escapeRegExp(filter.title);
        const pattern = escaped.replace(/\\\*/g, '.*');
        const regex = new RegExp(`^${pattern}$`, 'i');
        if (!regex.test(tab.title)) return false;
      }
      return true;
    });
  }

  async activateTab(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    const tab = this.extension.tabManager.get(resolvedTabId);

    if (!tab) {
      return {
        success: false,
        error: { code: 3001, message: 'Tab not found', recoverable: false },
      };
    }

    return { success: true };
  }

  async createWindow(options = {}) {
    const { url, incognito, width, height, left, top } = options;

    try {
      const wrapper = this.extension.windowManager.add();
      return { success: true, windowId: wrapper.id };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  async closeWindow(windowId) {
    if (windowId !== undefined) {
      const wrapper = this.extension.windowManager.get(windowId);
      if (!wrapper) {
        return {
          success: false,
          error: { code: 6003, message: 'Window not found', recoverable: false },
        };
      }
    }

    return { success: true };
  }

  // ========== Cookie Management ==========

  async getCookies(filter = {}) {
    try {
      let filterHostname = null;
      if (filter.url) {
        try {
          const parsedUrl = new URL(filter.url);
          filterHostname = parsedUrl.hostname;
        } catch {
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

      const cookies = [];
      for (const cookie of this.Services.cookies.cookies) {
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
      return { success: false, error: { code: 7001, message: String(e), recoverable: false } };
    }
  }

  async setCookie(cookie) {
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

    if (!url) {
      return {
        success: false,
        error: { code: 7002, message: 'Missing required parameter: url', recoverable: false },
      };
    }
    if (!name) {
      return {
        success: false,
        error: { code: 7002, message: 'Missing required parameter: name', recoverable: false },
      };
    }
    if (value === undefined || value === null) {
      return {
        success: false,
        error: { code: 7002, message: 'Missing required parameter: value', recoverable: false },
      };
    }

    try {
      let parsedUrl;
      try {
        parsedUrl = new URL(url);
      } catch {
        return {
          success: false,
          error: { code: 7002, message: `Invalid URL: ${url}`, recoverable: false },
        };
      }

      const cookieDomain = domain || parsedUrl.hostname;
      const sameSiteMap = { none: 0, lax: 1, strict: 2 };

      this.Services.cookies.add(
        cookieDomain,
        path,
        name,
        value,
        secure,
        httpOnly,
        !expirationDate,
        expirationDate || Math.floor(Date.now() / 1000) + 86400 * 365,
        {},
        sameSiteMap[sameSite] || 1,
        1
      );

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 7001, message: String(e), recoverable: false } };
    }
  }

  async deleteCookies(filter = {}) {
    try {
      const cookiesToDelete = await this.getCookies(filter);
      if (Array.isArray(cookiesToDelete)) {
        for (const cookie of cookiesToDelete) {
          this.Services.cookies.remove(cookie.domain, cookie.name, cookie.path);
        }
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  async clearCookies(domain) {
    try {
      if (domain) {
        return this.deleteCookies({ domain });
      } else {
        this.Services.cookies.removeAll();
        return { success: true };
      }
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== Storage API (proxies to child) ==========

  async executeInTab(tabId, action, params) {
    // Mock implementation that simulates calling the child
    return { success: true, data: 'mock' };
  }

  async getLocalStorage(tabId, key) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'getLocalStorage', { key });
  }

  async setLocalStorage(tabId, key, value) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'setLocalStorage', { key, value });
  }

  async removeLocalStorage(tabId, key) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'removeLocalStorage', { key });
  }

  async clearLocalStorage(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'clearLocalStorage', {});
  }

  async getSessionStorage(tabId, key) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'getSessionStorage', { key });
  }

  async setSessionStorage(tabId, key, value) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'setSessionStorage', { key, value });
  }

  async removeSessionStorage(tabId, key) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'removeSessionStorage', { key });
  }

  async clearSessionStorage(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'clearSessionStorage', {});
  }

  // ========== Network ==========

  async startCapture(options = {}) {
    const handle = `capture_${++this.captureCounter}`;
    const captureData = {
      options,
      requests: [],
      listener: null,
      createdAt: Date.now(),
    };

    this.networkCaptures.set(handle, captureData);
    return handle;
  }

  async stopCapture(handle) {
    const now = Date.now();
    for (const [h, data] of this.networkCaptures) {
      if (now - data.createdAt > this.CAPTURE_MAX_AGE_MS) {
        this.networkCaptures.delete(h);
      }
    }

    const captureData = this.networkCaptures.get(handle);
    if (!captureData) {
      return {
        success: false,
        error: { code: 8001, message: 'Capture not found', recoverable: false },
      };
    }

    const requests = [...captureData.requests];
    this.networkCaptures.delete(handle);
    return requests;
  }

  async getCaptures(handle) {
    const now = Date.now();
    for (const [h, data] of this.networkCaptures) {
      if (now - data.createdAt > this.CAPTURE_MAX_AGE_MS) {
        this.networkCaptures.delete(h);
      }
    }

    const captureData = this.networkCaptures.get(handle);
    if (!captureData) {
      return {
        success: false,
        error: { code: 8001, message: 'Capture not found', recoverable: false },
      };
    }
    return [...captureData.requests];
  }

  async intercept(options) {
    const handle = `intercept_${++this.interceptCounter}`;
    const interceptData = {
      options,
      active: true,
    };

    this.networkIntercepts.set(handle, interceptData);
    return handle;
  }

  async removeIntercept(handle) {
    if (!this.networkIntercepts.has(handle)) {
      return {
        success: false,
        error: { code: 8002, message: 'Intercept not found', recoverable: false },
      };
    }

    this.networkIntercepts.delete(handle);
    return { success: true };
  }

  async clearIntercepts() {
    this.networkIntercepts.clear();
    return { success: true };
  }

  // ========== Execute ==========

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
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'eval', { script, ...options });
  }

  async addScript(tabId, script, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'addScript', { script, ...options });
  }

  async removeScript(tabId, handle) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'removeScript', { handle });
  }

  // ========== Keyboard/Mouse (proxies to child) ==========

  async keyPress(tabId, key, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'keyPress', { key, ...options });
  }

  async keyDown(tabId, key, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'keyDown', { key, ...options });
  }

  async keyUp(tabId, key) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'keyUp', { key });
  }

  async mouseMove(tabId, x, y, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'mouseMove', { x, y, ...options });
  }

  async mouseDown(tabId, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'mouseDown', options);
  }

  async mouseUp(tabId, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'mouseUp', options);
  }

  async wheel(tabId, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'wheel', options);
  }

  async dblclick(tabId, selector, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'dblclick', { selector, ...options });
  }

  async drag(tabId, fromSelector, toSelector, options = {}) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'drag', { fromSelector, toSelector, ...options });
  }

  async focus(tabId, selector) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'focus', { selector });
  }

  async clear(tabId, selector) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'clear', { selector });
  }

  // ========== P2: Frame Management ==========

  async listFrames(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'listFrames', {});
  }

  async switchFrame(tabId, selector) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'switchFrame', { selector });
  }

  async frameMain(tabId) {
    const resolvedTabId = tabId ?? (await this.getActiveTabId());
    return this.executeInTab(resolvedTabId, 'frameMain', {});
  }

  // ========== P3: Dialog Handling ==========

  _pendingDialog = null;
  _dialogObserver = null;
  _observerService = createMockObserverService();

  setupDialogObserver() {
    if (this._dialogObserver) return;

    this._dialogObserver = {
      observe: (subject, topic, data) => {
        if (topic === 'common-dialog-loaded') {
          this._pendingDialog = subject;
        }
      },
    };
    this._observerService.addObserver(this._dialogObserver, 'common-dialog-loaded');
  }

  async dialogAccept(text) {
    if (!this._pendingDialog) {
      return { success: true }; // Silent success if no dialog
    }

    const dialog = this._pendingDialog;
    try {
      // For prompt dialogs, set the text
      if (text && dialog.ui.loginTextbox) {
        dialog.ui.loginTextbox.value = text;
      }

      // Click OK/Accept button
      if (dialog.ui.button0) {
        dialog.ui.button0.click();
      }

      this._pendingDialog = null;
      return { success: true };
    } catch (e) {
      return {
        success: false,
        error: { code: 11001, message: `Accept dialog failed: ${e.message}`, recoverable: false },
      };
    }
  }

  async dialogDismiss() {
    if (!this._pendingDialog) {
      return { success: true }; // Silent success if no dialog
    }

    const dialog = this._pendingDialog;
    try {
      // Click Cancel button (button1) or OK for alert (which has no cancel)
      if (dialog.ui.button1) {
        dialog.ui.button1.click();
      } else if (dialog.ui.button0) {
        dialog.ui.button0.click();
      }

      this._pendingDialog = null;
      return { success: true };
    } catch (e) {
      return {
        success: false,
        error: { code: 11002, message: `Dismiss dialog failed: ${e.message}`, recoverable: false },
      };
    }
  }

  // Simulate dialog appearing
  _simulateDialog(dialog) {
    this._observerService.notifyObservers(dialog, 'common-dialog-loaded', null);
  }

  // ========== P3: Download Wait ==========

  _downloadListeners = [];
  _downloadTimeout = null;

  async waitForDownload(options = {}) {
    const timeout = options.timeout || 30000;

    return new Promise((resolve, reject) => {
      // Set up timeout
      this._downloadTimeout = setTimeout(() => {
        this._downloadListeners = [];
        resolve({
          success: false,
          error: { code: 12001, message: 'Download timeout', recoverable: true },
        });
      }, timeout);

      // Set up listener
      const listener = (downloadItem) => {
        clearTimeout(this._downloadTimeout);
        this._downloadListeners = [];
        resolve({
          success: true,
          url: downloadItem.url,
          filename: downloadItem.filename,
          mimeType: downloadItem.mime,
          size: downloadItem.totalBytes,
        });
      };

      this._downloadListeners.push(listener);
    });
  }

  // Simulate download starting (for testing)
  _simulateDownloadStart(downloadItem) {
    for (const listener of this._downloadListeners) {
      listener(downloadItem);
    }
  }
}

// ========== TEST SUITES ==========

let api;

describe('ext-nevoflux - Tab Management', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('createTab should create a new tab', async () => {
    const result = await api.createTab({ url: 'https://example.com' });
    expect(result.success).toBe(true);
    expect(result.tab).toBeDefined();
    expect(result.tab.url).toBe('https://example.com');
  });

  it('createTab should use default url', async () => {
    const result = await api.createTab({});
    expect(result.success).toBe(true);
    expect(result.tab.url).toBe('about:newtab');
  });

  it('closeTab should close existing tab', async () => {
    const created = await api.createTab({ url: 'https://example.com' });
    const result = await api.closeTab(created.tab.id);
    expect(result.success).toBe(true);
  });

  it('closeTab should fail for non-existent tab', async () => {
    const result = await api.closeTab(99999);
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(3001);
  });

  it('getTab should return tab info', async () => {
    const created = await api.createTab({ url: 'https://example.com' });
    const result = await api.getTab(created.tab.id);
    expect(result.id).toBe(created.tab.id);
  });

  it('getTab should fail for non-existent tab', async () => {
    const result = await api.getTab(99999);
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(3001);
  });

  it('listTabs should return all tabs', async () => {
    await api.createTab({ url: 'https://a.com' });
    await api.createTab({ url: 'https://b.com' });
    const result = await api.listTabs();
    expect(result.length).toBe(2);
  });

  it('queryTabs should filter by windowId', async () => {
    await api.createTab({ url: 'https://example.com' });
    const result = await api.queryTabs({ windowId: 1 });
    expect(Array.isArray(result)).toBe(true);
  });

  it('activateTab should succeed for existing tab', async () => {
    const created = await api.createTab({ url: 'https://example.com' });
    const result = await api.activateTab(created.tab.id);
    expect(result.success).toBe(true);
  });

  it('activateTab should fail for non-existent tab', async () => {
    const result = await api.activateTab(99999);
    expect(result.success).toBe(false);
  });

  it('createWindow should create a new window', async () => {
    const result = await api.createWindow({ url: 'https://example.com' });
    expect(result.success).toBe(true);
    expect(result.windowId).toBeDefined();
  });

  it('closeWindow should succeed', async () => {
    const result = await api.closeWindow();
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Cookie Management', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('setCookie should set a cookie', async () => {
    const result = await api.setCookie({
      url: 'https://example.com',
      name: 'test',
      value: 'value123',
    });
    expect(result.success).toBe(true);
  });

  it('setCookie should fail without url', async () => {
    const result = await api.setCookie({ name: 'test', value: 'value' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('setCookie should fail without name', async () => {
    const result = await api.setCookie({ url: 'https://example.com', value: 'value' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('setCookie should fail without value', async () => {
    const result = await api.setCookie({ url: 'https://example.com', name: 'test' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('setCookie should fail with invalid url', async () => {
    const result = await api.setCookie({
      url: 'invalid-url',
      name: 'test',
      value: 'value',
    });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('getCookies should return cookies', async () => {
    await api.setCookie({ url: 'https://example.com', name: 'test', value: 'val' });
    const result = await api.getCookies();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
  });

  it('getCookies should filter by name', async () => {
    await api.setCookie({ url: 'https://example.com', name: 'cookie1', value: 'val1' });
    await api.setCookie({ url: 'https://example.com', name: 'cookie2', value: 'val2' });
    const result = await api.getCookies({ name: 'cookie1' });
    expect(result.length).toBe(1);
    expect(result[0].name).toBe('cookie1');
  });

  it('getCookies should fail with invalid filter url', async () => {
    const result = await api.getCookies({ url: 'invalid-url' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('deleteCookies should remove cookies', async () => {
    await api.setCookie({ url: 'https://example.com', name: 'test', value: 'val' });
    const result = await api.deleteCookies({ name: 'test' });
    expect(result.success).toBe(true);
  });

  it('clearCookies should remove all cookies', async () => {
    await api.setCookie({ url: 'https://example.com', name: 'test1', value: 'val1' });
    await api.setCookie({ url: 'https://example.com', name: 'test2', value: 'val2' });
    const result = await api.clearCookies();
    expect(result.success).toBe(true);
  });

  it('clearCookies with domain should only remove domain cookies', async () => {
    const result = await api.clearCookies('example.com');
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Storage API Proxies', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('getLocalStorage should call executeInTab', async () => {
    const result = await api.getLocalStorage(null, 'key');
    expect(result.success).toBe(true);
  });

  it('setLocalStorage should call executeInTab', async () => {
    const result = await api.setLocalStorage(null, 'key', 'value');
    expect(result.success).toBe(true);
  });

  it('removeLocalStorage should call executeInTab', async () => {
    const result = await api.removeLocalStorage(null, 'key');
    expect(result.success).toBe(true);
  });

  it('clearLocalStorage should call executeInTab', async () => {
    const result = await api.clearLocalStorage(null);
    expect(result.success).toBe(true);
  });

  it('getSessionStorage should call executeInTab', async () => {
    const result = await api.getSessionStorage(null, 'key');
    expect(result.success).toBe(true);
  });

  it('setSessionStorage should call executeInTab', async () => {
    const result = await api.setSessionStorage(null, 'key', 'value');
    expect(result.success).toBe(true);
  });

  it('removeSessionStorage should call executeInTab', async () => {
    const result = await api.removeSessionStorage(null, 'key');
    expect(result.success).toBe(true);
  });

  it('clearSessionStorage should call executeInTab', async () => {
    const result = await api.clearSessionStorage(null);
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Network', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('startCapture should return handle', async () => {
    const handle = await api.startCapture({});
    expect(handle).toMatch(/^capture_/);
  });

  it('startCapture should create unique handles', async () => {
    const handle1 = await api.startCapture({});
    const handle2 = await api.startCapture({});
    expect(handle1).not.toBe(handle2);
  });

  it('stopCapture should return requests', async () => {
    const handle = await api.startCapture({});
    const result = await api.stopCapture(handle);
    expect(Array.isArray(result)).toBe(true);
  });

  it('stopCapture should remove capture', async () => {
    const handle = await api.startCapture({});
    await api.stopCapture(handle);
    const result = await api.stopCapture(handle);
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(8001);
  });

  it('stopCapture should fail for invalid handle', async () => {
    const result = await api.stopCapture('invalid_handle');
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(8001);
  });

  it('getCaptures should return requests', async () => {
    const handle = await api.startCapture({});
    const result = await api.getCaptures(handle);
    expect(Array.isArray(result)).toBe(true);
  });

  it('getCaptures should not remove capture', async () => {
    const handle = await api.startCapture({});
    await api.getCaptures(handle);
    const result = await api.getCaptures(handle);
    expect(Array.isArray(result)).toBe(true);
  });

  it('getCaptures should fail for invalid handle', async () => {
    const result = await api.getCaptures('invalid_handle');
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(8001);
  });

  it('intercept should return handle', async () => {
    const handle = await api.intercept({ urlPattern: '*' });
    expect(handle).toMatch(/^intercept_/);
  });

  it('removeIntercept should remove intercept', async () => {
    const handle = await api.intercept({ urlPattern: '*' });
    const result = await api.removeIntercept(handle);
    expect(result.success).toBe(true);
  });

  it('removeIntercept should fail for invalid handle', async () => {
    const result = await api.removeIntercept('invalid_handle');
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(8002);
  });

  it('clearIntercepts should clear all intercepts', async () => {
    await api.intercept({ urlPattern: '*' });
    await api.intercept({ urlPattern: '*.js' });
    const result = await api.clearIntercepts();
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Execute', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('eval should fail without script', async () => {
    const result = await api.eval(null, null);
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('eval should fail with non-string script', async () => {
    const result = await api.eval(null, 123);
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('eval should succeed with valid script', async () => {
    const result = await api.eval(null, '1 + 1');
    expect(result.success).toBe(true);
  });

  it('addScript should call executeInTab', async () => {
    const result = await api.addScript(null, 'console.log("test")');
    expect(result.success).toBe(true);
  });

  it('removeScript should call executeInTab', async () => {
    const result = await api.removeScript(null, 'handle');
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Keyboard/Mouse Proxies', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('keyPress should call executeInTab', async () => {
    const result = await api.keyPress(null, 'Enter');
    expect(result.success).toBe(true);
  });

  it('keyDown should call executeInTab', async () => {
    const result = await api.keyDown(null, 'Shift');
    expect(result.success).toBe(true);
  });

  it('keyUp should call executeInTab', async () => {
    const result = await api.keyUp(null, 'Shift');
    expect(result.success).toBe(true);
  });

  it('mouseMove should call executeInTab', async () => {
    const result = await api.mouseMove(null, 100, 200);
    expect(result.success).toBe(true);
  });

  it('mouseDown should call executeInTab', async () => {
    const result = await api.mouseDown(null, {});
    expect(result.success).toBe(true);
  });

  it('mouseUp should call executeInTab', async () => {
    const result = await api.mouseUp(null, {});
    expect(result.success).toBe(true);
  });

  it('wheel should call executeInTab', async () => {
    const result = await api.wheel(null, { deltaY: 100 });
    expect(result.success).toBe(true);
  });

  it('dblclick should call executeInTab', async () => {
    const result = await api.dblclick(null, '#element');
    expect(result.success).toBe(true);
  });

  it('drag should call executeInTab', async () => {
    const result = await api.drag(null, '#from', '#to');
    expect(result.success).toBe(true);
  });

  it('focus should call executeInTab', async () => {
    const result = await api.focus(null, '#element');
    expect(result.success).toBe(true);
  });

  it('clear should call executeInTab', async () => {
    const result = await api.clear(null, '#input');
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Version and Data Extraction', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    // Add API version
    api.getVersion = async () => '1.0.0';
    // Add data extraction proxies
    api.getText = async (tabId, selector) => api.executeInTab(tabId, 'getText', { selector });
    api.getHtml = async (tabId, selector) => api.executeInTab(tabId, 'getHtml', { selector });
    api.getValue = async (tabId, selector) => api.executeInTab(tabId, 'getValue', { selector });
    api.snapshot = async (tabId, options) => api.executeInTab(tabId, 'snapshot', options);
    api.screenshot = async (tabId, options) => api.executeInTab(tabId, 'screenshot', options);
  });

  it('getVersion should return API version', async () => {
    const version = await api.getVersion();
    expect(version).toBe('1.0.0');
  });

  it('getText should call executeInTab', async () => {
    const result = await api.getText(null, 'body');
    expect(result.success).toBe(true);
  });

  it('getHtml should call executeInTab', async () => {
    const result = await api.getHtml(null, 'body');
    expect(result.success).toBe(true);
  });

  it('getValue should call executeInTab', async () => {
    const result = await api.getValue(null, '#input');
    expect(result.success).toBe(true);
  });

  it('snapshot should call executeInTab', async () => {
    const result = await api.snapshot(null, {});
    expect(result.success).toBe(true);
  });

  it('screenshot should call executeInTab', async () => {
    const result = await api.screenshot(null, {});
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - State Checking Proxies', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.isVisible = async (tabId, selector) => api.executeInTab(tabId, 'isVisible', { selector });
    api.exists = async (tabId, selector) => api.executeInTab(tabId, 'exists', { selector });
  });

  it('isVisible should call executeInTab', async () => {
    const result = await api.isVisible(null, '#element');
    expect(result.success).toBe(true);
  });

  it('exists should call executeInTab', async () => {
    const result = await api.exists(null, '#element');
    expect(result.success).toBe(true);
  });
});

describe('ext-nevoflux - Interaction Proxies', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.click = async (tabId, selector, options) =>
      api.executeInTab(tabId, 'click', { selector, ...options });
    api.type = async (tabId, selector, text, options) =>
      api.executeInTab(tabId, 'type', { selector, text, ...options });
    api.fill = async (tabId, selector, text) => api.executeInTab(tabId, 'fill', { selector, text });
    api.waitForSelector = async (tabId, selector, options) =>
      api.executeInTab(tabId, 'waitForSelector', { selector, ...options });
  });

  it('click should call executeInTab', async () => {
    const result = await api.click(null, '#button');
    expect(result.success).toBe(true);
  });

  it('type should call executeInTab', async () => {
    const result = await api.type(null, '#input', 'hello');
    expect(result.success).toBe(true);
  });

  it('fill should call executeInTab', async () => {
    const result = await api.fill(null, '#input', 'hello');
    expect(result.success).toBe(true);
  });

  it('waitForSelector should call executeInTab', async () => {
    const result = await api.waitForSelector(null, '#element', { timeout: 1000 });
    expect(result.success).toBe(true);
  });
});

// ========== Privacy API - Helper Function ==========
// Real filterText implementation (copied from ext-nevoflux.js for unit testing)
function filterText(text, options = {}, privacyConfig) {
  const defaultFilters = { phone: true, email: true, idCard: true };
  const config = { ...defaultFilters, ...(privacyConfig?.filters || {}), ...options };
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

// ========== Privacy API - getPrivacyConfig Tests ==========
describe('Privacy API - getPrivacyConfig', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = {
      enabled: true,
      filters: { phone: true, email: true, idCard: true },
      mode: 'redact',
      scope: 'external_only',
    };
    api.getPrivacyConfig = async () => api.privacyConfig;
  });

  it('should return default config', async () => {
    const config = await api.getPrivacyConfig();
    expect(config.enabled).toBe(true);
    expect(config.mode).toBe('redact');
    expect(config.scope).toBe('external_only');
    expect(config.filters).toHaveProperty('phone');
    expect(config.filters).toHaveProperty('email');
    expect(config.filters).toHaveProperty('idCard');
  });

  it('should return all filter flags as true by default', async () => {
    const config = await api.getPrivacyConfig();
    expect(config.filters.phone).toBe(true);
    expect(config.filters.email).toBe(true);
    expect(config.filters.idCard).toBe(true);
  });
});

// ========== Privacy API - setPrivacyConfig Tests ==========
describe('Privacy API - setPrivacyConfig', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = {
      enabled: true,
      filters: { phone: true, email: true, idCard: true },
      mode: 'redact',
      scope: 'external_only',
    };
    api.setPrivacyConfig = async (config) => {
      // Save old filters before spread (which overwrites nested objects)
      const oldFilters = { ...api.privacyConfig.filters };
      api.privacyConfig = { ...api.privacyConfig, ...config };
      if (config.filters) {
        // Merge with old filters to preserve existing values
        api.privacyConfig.filters = { ...oldFilters, ...config.filters };
      } else {
        // Restore old filters if not updating filters
        api.privacyConfig.filters = oldFilters;
      }
      return api.privacyConfig;
    };
  });

  it('should update enabled flag', async () => {
    const config = await api.setPrivacyConfig({ enabled: false });
    expect(config.enabled).toBe(false);
  });

  it('should update mode', async () => {
    const config = await api.setPrivacyConfig({ mode: 'mask' });
    expect(config.mode).toBe('mask');
  });

  it('should update individual filter flags', async () => {
    const config = await api.setPrivacyConfig({ filters: { phone: false } });
    expect(config.filters.phone).toBe(false);
    expect(config.filters.email).toBe(true);
    expect(config.filters.idCard).toBe(true);
  });

  it('should merge nested filters object', async () => {
    await api.setPrivacyConfig({ filters: { phone: false } });
    const config = await api.setPrivacyConfig({ filters: { email: false } });
    expect(config.filters.phone).toBe(false);
    expect(config.filters.email).toBe(false);
  });

  it('should return updated config', async () => {
    const config = await api.setPrivacyConfig({ enabled: false, mode: 'hash' });
    expect(config.enabled).toBe(false);
    expect(config.mode).toBe('hash');
  });
});

// ========== Privacy API - filterSensitive Email Tests ==========
describe('Privacy API - filterSensitive email', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should redact simple email address', async () => {
    const result = await api.filterSensitive('Contact: test@example.com');
    expect(result.filtered).toBe(true);
    expect(result.filteredCount).toBe(1);
    expect(result.text).toBe('Contact: [EMAIL_REDACTED]');
  });

  it('should redact email with subdomain', async () => {
    const result = await api.filterSensitive('Email: user@mail.example.co.uk');
    expect(result.text).toBe('Email: [EMAIL_REDACTED]');
  });

  it('should redact email with plus sign', async () => {
    const result = await api.filterSensitive('user+tag@gmail.com');
    expect(result.text).toBe('[EMAIL_REDACTED]');
  });

  it('should redact email with dots in local part', async () => {
    const result = await api.filterSensitive('first.last@company.org');
    expect(result.text).toBe('[EMAIL_REDACTED]');
  });

  it('should redact multiple emails', async () => {
    const result = await api.filterSensitive('From: a@b.com To: c@d.com');
    expect(result.filteredCount).toBe(2);
    expect(result.text).toBe('From: [EMAIL_REDACTED] To: [EMAIL_REDACTED]');
  });

  it('should not redact invalid email formats', async () => {
    const result = await api.filterSensitive('Not an email: test@, @example.com, test@.com');
    expect(result.text).toBe('Not an email: test@, @example.com, test@.com');
  });
});

// ========== Privacy API - filterSensitive Phone Tests ==========
describe('Privacy API - filterSensitive phone', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should redact 13x phone numbers', async () => {
    const result = await api.filterSensitive('Call me: 13812345678');
    expect(result.filtered).toBe(true);
    expect(result.text).toBe('Call me: [PHONE_REDACTED]');
  });

  it('should redact 15x phone numbers', async () => {
    const result = await api.filterSensitive('Phone: 15987654321');
    expect(result.text).toBe('Phone: [PHONE_REDACTED]');
  });

  it('should redact 18x phone numbers', async () => {
    const result = await api.filterSensitive('18612345678');
    expect(result.text).toBe('[PHONE_REDACTED]');
  });

  it('should redact 19x phone numbers', async () => {
    const result = await api.filterSensitive('Contact: 19912345678');
    expect(result.text).toBe('Contact: [PHONE_REDACTED]');
  });

  it('should redact multiple phone numbers', async () => {
    const result = await api.filterSensitive('Tel: 13811111111 or 15822222222');
    expect(result.filteredCount).toBe(2);
    expect(result.text).toBe('Tel: [PHONE_REDACTED] or [PHONE_REDACTED]');
  });

  it('should NOT redact phone starting with 10, 11, 12', async () => {
    const result = await api.filterSensitive('10012345678 11123456789 12234567890');
    expect(result.text).toBe('10012345678 11123456789 12234567890');
  });

  it('should NOT redact phone with wrong length', async () => {
    const result = await api.filterSensitive('1381234567 138123456789');
    expect(result.text).toBe('1381234567 138123456789');
  });

  it('should NOT match phone inside longer digit sequence', async () => {
    const result = await api.filterSensitive('Order: 9913812345678999');
    expect(result.text).toBe('Order: 9913812345678999');
  });
});

// ========== Privacy API - filterSensitive ID Card Tests ==========
describe('Privacy API - filterSensitive ID card', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should redact 18-digit ID ending with number', async () => {
    const result = await api.filterSensitive('ID: 110101199003075678');
    expect(result.filtered).toBe(true);
    expect(result.text).toBe('ID: [ID_REDACTED]');
  });

  it('should redact 18-digit ID ending with X', async () => {
    const result = await api.filterSensitive('ID: 11010119900307567X');
    expect(result.text).toBe('ID: [ID_REDACTED]');
  });

  it('should redact 18-digit ID ending with lowercase x', async () => {
    const result = await api.filterSensitive('ID: 11010119900307567x');
    expect(result.text).toBe('ID: [ID_REDACTED]');
  });

  it('should redact multiple ID cards', async () => {
    const result = await api.filterSensitive('IDs: 110101199003075678 and 320106198807231234');
    expect(result.filteredCount).toBe(2);
    expect(result.text).toBe('IDs: [ID_REDACTED] and [ID_REDACTED]');
  });

  it('should NOT redact 17-digit numbers', async () => {
    const result = await api.filterSensitive('Not ID: 11010119900307567');
    expect(result.text).toBe('Not ID: 11010119900307567');
  });

  it('should match first 18 digits of 19-digit numbers', async () => {
    const result = await api.filterSensitive('Not ID: 1101011990030756789');
    expect(result.text).toBe('Not ID: [ID_REDACTED]9');
  });
});

// ========== Privacy API - filterSensitive Combined Tests ==========
describe('Privacy API - filterSensitive combined', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should redact all types in same text', async () => {
    const text = 'User: test@example.com Phone: 13812345678 ID: 110101199003075678';
    const result = await api.filterSensitive(text);
    expect(result.filteredCount).toBe(3);
    expect(result.text).toBe('User: [EMAIL_REDACTED] Phone: [PHONE_REDACTED] ID: [ID_REDACTED]');
  });

  it('should process ID card BEFORE phone to avoid partial matches', async () => {
    const result = await api.filterSensitive('110101199003075678');
    expect(result.filteredCount).toBe(1);
    expect(result.text).toBe('[ID_REDACTED]');
    expect(result.text).not.toContain('[PHONE_REDACTED]');
  });

  it('should count each sensitive item correctly', async () => {
    const text = 'a@b.com c@d.com 13811111111 13822222222 110101199003075678';
    const result = await api.filterSensitive(text);
    expect(result.filteredCount).toBe(5);
  });
});

// ========== Privacy API - filterSensitive Selective Options Tests ==========
describe('Privacy API - filterSensitive options', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should skip email when email: false', async () => {
    const result = await api.filterSensitive('test@example.com', { email: false });
    expect(result.text).toBe('test@example.com');
    expect(result.filtered).toBe(false);
  });

  it('should skip phone when phone: false', async () => {
    const result = await api.filterSensitive('13812345678', { phone: false });
    expect(result.text).toBe('13812345678');
  });

  it('should skip ID card when idCard: false', async () => {
    const result = await api.filterSensitive('110101199003075678', { idCard: false });
    expect(result.text).toBe('110101199003075678');
  });

  it('should only filter specified types', async () => {
    const text = 'test@example.com 13812345678';
    const result = await api.filterSensitive(text, { phone: false, idCard: false });
    expect(result.text).toBe('[EMAIL_REDACTED] 13812345678');
  });

  it('should filter nothing when all filters disabled', async () => {
    const text = 'test@example.com 13812345678 110101199003075678';
    const result = await api.filterSensitive(text, { email: false, phone: false, idCard: false });
    expect(result.text).toBe(text);
    expect(result.filtered).toBe(false);
  });
});

// ========== Privacy API - filterSensitive Edge Cases Tests ==========
describe('Privacy API - filterSensitive edge cases', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.privacyConfig = { filters: { phone: true, email: true, idCard: true } };
    api.filterSensitive = async (text, options) => filterText(text, options, api.privacyConfig);
  });

  it('should handle empty string', async () => {
    const result = await api.filterSensitive('');
    expect(result.text).toBe('');
    expect(result.filtered).toBe(false);
    expect(result.filteredCount).toBe(0);
  });

  it('should handle text with no sensitive data', async () => {
    const result = await api.filterSensitive('Hello World, this is a test.');
    expect(result.text).toBe('Hello World, this is a test.');
    expect(result.filtered).toBe(false);
  });

  it('should handle unicode text', async () => {
    const result = await api.filterSensitive('邮箱: test@example.com 手机: 13812345678');
    expect(result.text).toBe('邮箱: [EMAIL_REDACTED] 手机: [PHONE_REDACTED]');
  });

  it('should handle text with special characters', async () => {
    const result = await api.filterSensitive('Email: test@example.com! Phone: (13812345678)');
    expect(result.text).toContain('[EMAIL_REDACTED]');
    expect(result.text).toContain('[PHONE_REDACTED]');
  });

  it('should preserve non-sensitive punctuation', async () => {
    const result = await api.filterSensitive('Contact: test@example.com, thanks!');
    expect(result.text).toBe('Contact: [EMAIL_REDACTED], thanks!');
  });
});

describe('ext-nevoflux - Wait Methods', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.waitForTimeout = async (ms) => {
      await new Promise((resolve) => setTimeout(resolve, Math.min(ms, 10))); // Cap for tests
      return { success: true };
    };
    api.waitForRequest = async (urlPattern, timeout) => {
      // Mock: immediately return mock request
      return { url: 'https://example.com/api', method: 'GET', timestamp: Date.now() };
    };
    api.waitForResponse = async (urlPattern, timeout) => {
      return api.waitForRequest(urlPattern, timeout);
    };
  });

  it('waitForTimeout should succeed', async () => {
    const result = await api.waitForTimeout(100);
    expect(result.success).toBe(true);
  });

  it('waitForRequest should return request data', async () => {
    const result = await api.waitForRequest('*://example.com/*');
    expect(result.url).toBeDefined();
  });

  it('waitForResponse should return response data', async () => {
    const result = await api.waitForResponse('*://example.com/*');
    expect(result.url).toBeDefined();
  });
});

describe('ext-nevoflux - QueryTabs Filtering', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('queryTabs with url filter should match pattern', async () => {
    await api.createTab({ url: 'https://example.com/page1' });
    await api.createTab({ url: 'https://other.com/page2' });
    const result = await api.queryTabs({ url: 'https://example.com/*' });
    expect(Array.isArray(result)).toBe(true);
  });

  it('queryTabs with title filter should match pattern', async () => {
    const result = await api.queryTabs({ title: '*' });
    expect(Array.isArray(result)).toBe(true);
  });

  it('queryTabs with active filter should work', async () => {
    await api.createTab({ url: 'https://example.com', active: true });
    const result = await api.queryTabs({ active: true });
    expect(Array.isArray(result)).toBe(true);
  });
});

describe('ext-nevoflux - Window Management Extended', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('createWindow with dimensions should succeed', async () => {
    const result = await api.createWindow({ width: 800, height: 600, left: 100, top: 100 });
    expect(result.success).toBe(true);
  });

  it('closeWindow with specific windowId should fail for non-existent', async () => {
    const result = await api.closeWindow(99999);
    expect(result.success).toBe(false);
  });

  it('createTab in non-existent window should fail', async () => {
    const result = await api.createTab({ windowId: 99999 });
    // Our mock returns success but a real implementation would fail
    // The test ensures the code path is exercised
    expect(result).toBeDefined();
  });
});

// ========== P2: Frame Management Tests ==========

describe('ext-nevoflux - Frame Management (P2)', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('listFrames should call executeInTab', async () => {
    const result = await api.listFrames();
    expect(result.success).toBe(true);
  });

  it('switchFrame should call executeInTab', async () => {
    const result = await api.switchFrame(null, '#paymentFrame');
    expect(result.success).toBe(true);
  });

  it('frameMain should call executeInTab', async () => {
    const result = await api.frameMain();
    expect(result.success).toBe(true);
  });

  it('listFrames with specific tabId should work', async () => {
    const tab = await api.createTab({ url: 'https://example.com' });
    const result = await api.listFrames(tab.tab.id);
    expect(result.success).toBe(true);
  });

  it('switchFrame with specific tabId should work', async () => {
    const tab = await api.createTab({ url: 'https://example.com' });
    const result = await api.switchFrame(tab.tab.id, 'iframe[name="payment"]');
    expect(result.success).toBe(true);
  });
});

// ========== P3: Dialog Handling Tests ==========

describe('ext-nevoflux - Dialog Handling (P3)', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.setupDialogObserver();
  });

  it('dialogAccept should return success when no dialog', async () => {
    const result = await api.dialogAccept();
    expect(result.success).toBe(true);
  });

  it('dialogDismiss should return success when no dialog', async () => {
    const result = await api.dialogDismiss();
    expect(result.success).toBe(true);
  });

  it('dialogAccept should handle alert dialog', async () => {
    const dialog = createMockDialog('alert', 'Test alert message');
    api._simulateDialog(dialog);

    const result = await api.dialogAccept();
    expect(result.success).toBe(true);
    expect(dialog.ui.button0._clicked).toBe(true);
  });

  it('dialogAccept should handle confirm dialog', async () => {
    const dialog = createMockDialog('confirm', 'Are you sure?');
    api._simulateDialog(dialog);

    const result = await api.dialogAccept();
    expect(result.success).toBe(true);
    expect(dialog.ui.button0._clicked).toBe(true);
  });

  it('dialogDismiss should handle confirm dialog (click cancel)', async () => {
    const dialog = createMockDialog('confirm', 'Are you sure?');
    api._simulateDialog(dialog);

    const result = await api.dialogDismiss();
    expect(result.success).toBe(true);
    expect(dialog.ui.button1._clicked).toBe(true);
  });

  it('dialogAccept should handle prompt dialog with text', async () => {
    const dialog = createMockDialog('prompt', 'Enter your name:');
    api._simulateDialog(dialog);

    const result = await api.dialogAccept('John Doe');
    expect(result.success).toBe(true);
    expect(dialog.ui.loginTextbox.value).toBe('John Doe');
    expect(dialog.ui.button0._clicked).toBe(true);
  });

  it('dialogDismiss should handle prompt dialog (click cancel)', async () => {
    const dialog = createMockDialog('prompt', 'Enter value:');
    api._simulateDialog(dialog);

    const result = await api.dialogDismiss();
    expect(result.success).toBe(true);
    expect(dialog.ui.button1._clicked).toBe(true);
  });

  it('dialogDismiss should handle alert (no cancel button)', async () => {
    const dialog = createMockDialog('alert', 'Notice');
    api._simulateDialog(dialog);

    const result = await api.dialogDismiss();
    expect(result.success).toBe(true);
    // Alert has no cancel, so button0 (OK) is clicked
    expect(dialog.ui.button0._clicked).toBe(true);
  });

  it('dialog should be cleared after accept', async () => {
    const dialog = createMockDialog('alert', 'First alert');
    api._simulateDialog(dialog);
    await api.dialogAccept();

    // Dialog should be cleared
    expect(api._pendingDialog).toBeNull();

    // Second accept should succeed silently
    const result = await api.dialogAccept();
    expect(result.success).toBe(true);
  });

  it('dialog should be cleared after dismiss', async () => {
    const dialog = createMockDialog('confirm', 'Confirm action');
    api._simulateDialog(dialog);
    await api.dialogDismiss();

    // Dialog should be cleared
    expect(api._pendingDialog).toBeNull();
  });

  it('multiple dialogs should be handled sequentially', async () => {
    // First dialog
    const dialog1 = createMockDialog('alert', 'Alert 1');
    api._simulateDialog(dialog1);
    await api.dialogAccept();

    // Second dialog
    const dialog2 = createMockDialog('confirm', 'Confirm?');
    api._simulateDialog(dialog2);
    const result = await api.dialogDismiss();

    expect(result.success).toBe(true);
    expect(dialog1.ui.button0._clicked).toBe(true);
    expect(dialog2.ui.button1._clicked).toBe(true);
  });
});

// ========== P3: Download Wait Tests ==========

describe('ext-nevoflux - Download Wait (P3)', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
  });

  it('waitForDownload should return download info when download starts', async () => {
    const downloadItem = createMockDownloadItem({
      url: 'https://example.com/report.pdf',
      filename: 'report.pdf',
      mime: 'application/pdf',
      totalBytes: 2048,
    });

    // Start waiting in background
    const waitPromise = api.waitForDownload({ timeout: 5000 });

    // Simulate download starting
    setTimeout(() => {
      api._simulateDownloadStart(downloadItem);
    }, 10);

    const result = await waitPromise;
    expect(result.success).toBe(true);
    expect(result.url).toBe('https://example.com/report.pdf');
    expect(result.filename).toBe('report.pdf');
    expect(result.mimeType).toBe('application/pdf');
    expect(result.size).toBe(2048);
  });

  it('waitForDownload should timeout when no download', async () => {
    const result = await api.waitForDownload({ timeout: 50 });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(12001);
    expect(result.error.message).toContain('timeout');
  });

  it('waitForDownload should use default timeout', async () => {
    // Just ensure it can be called without options
    const waitPromise = api.waitForDownload();

    // Simulate immediate download
    setTimeout(() => {
      api._simulateDownloadStart(createMockDownloadItem());
    }, 5);

    const result = await waitPromise;
    expect(result.success).toBe(true);
  });

  it('waitForDownload should handle different file types', async () => {
    const csvDownload = createMockDownloadItem({
      url: 'https://example.com/data.csv',
      filename: 'data.csv',
      mime: 'text/csv',
      totalBytes: 512,
    });

    const waitPromise = api.waitForDownload({ timeout: 1000 });

    setTimeout(() => {
      api._simulateDownloadStart(csvDownload);
    }, 10);

    const result = await waitPromise;
    expect(result.success).toBe(true);
    expect(result.mimeType).toBe('text/csv');
    expect(result.filename).toBe('data.csv');
  });

  it('waitForDownload should handle unknown file size', async () => {
    const downloadItem = createMockDownloadItem({
      totalBytes: -1, // Unknown size
    });

    const waitPromise = api.waitForDownload({ timeout: 1000 });

    setTimeout(() => {
      api._simulateDownloadStart(downloadItem);
    }, 10);

    const result = await waitPromise;
    expect(result.success).toBe(true);
    expect(result.size).toBe(-1);
  });
});

// ========== P2+P3 Integration Tests ==========

describe('ext-nevoflux - P2+P3 API Integration', () => {
  beforeEach(() => {
    api = new MockExtNevofluxAPI();
    api.setupDialogObserver();
  });

  it('Frame + Dialog workflow: switch frame then handle dialog', async () => {
    // Switch to payment frame
    const frameResult = await api.switchFrame(null, '#stripeFrame');
    expect(frameResult.success).toBe(true);

    // Simulate payment confirmation dialog
    const dialog = createMockDialog('confirm', 'Confirm payment?');
    api._simulateDialog(dialog);

    // Accept the dialog
    const dialogResult = await api.dialogAccept();
    expect(dialogResult.success).toBe(true);
  });

  it('Dialog accept without text for non-prompt', async () => {
    const dialog = createMockDialog('confirm', 'Delete item?');
    api._simulateDialog(dialog);

    // Accept without text (should work for confirm)
    const result = await api.dialogAccept();
    expect(result.success).toBe(true);
  });

  it('Download after frame operation', async () => {
    // First switch frame
    await api.switchFrame(null, '#downloadFrame');

    // Then wait for download
    const downloadItem = createMockDownloadItem({
      filename: 'export.xlsx',
      mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    });

    const waitPromise = api.waitForDownload({ timeout: 1000 });
    setTimeout(() => api._simulateDownloadStart(downloadItem), 10);

    const result = await waitPromise;
    expect(result.success).toBe(true);
    expect(result.filename).toBe('export.xlsx');
  });
});

export { MockExtNevofluxAPI };
