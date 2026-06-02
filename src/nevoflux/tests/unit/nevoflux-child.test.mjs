/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for NevofluxChild.sys.mjs (Content Process)
 * Tests all P1 methods: keyboard, mouse, storage, execute
 */

import { describe, it, beforeEach, expect, skip } from './test-runner.mjs';
import {
  createMockDocument,
  createMockWindow,
  createMockStorage,
  createMockIframe,
} from './mocks/browser-mocks.mjs';

// Create a mock NevofluxChild class that mirrors the real implementation
class MockNevofluxChild {
  constructor() {
    this._document = createMockDocument();
    this._contentWindow = createMockWindow(this._document);
    // P2: Frame context tracking
    this._currentFrameSelector = null;
  }

  get doc() {
    return this._document || this._contentWindow?.document;
  }

  get document() {
    return this._document;
  }

  get contentWindow() {
    return this._contentWindow;
  }

  // P2: Current document respecting frame context
  get currentDoc() {
    if (!this._currentFrameSelector) {
      return this.doc;
    }
    const iframe = this.doc?.querySelector(this._currentFrameSelector);
    return iframe?.contentDocument || this.doc;
  }

  get currentWin() {
    if (!this._currentFrameSelector) {
      return this.contentWindow;
    }
    const iframe = this.doc?.querySelector(this._currentFrameSelector);
    return iframe?.contentWindow || this.contentWindow;
  }

  // ========== Execute Handler ==========
  async execute(action, params) {
    const safeParams = params || {};
    const handlers = {
      getText: () => this.getText(safeParams),
      getHtml: () => this.getHtml(safeParams),
      getValue: () => this.getValue(safeParams),
      snapshot: () => this.snapshot(safeParams),
      isVisible: () => this.isVisible(safeParams),
      exists: () => this.exists(safeParams),
      click: () => this.click(safeParams),
      type: () => this.type(safeParams),
      fill: () => this.fill(safeParams),
      waitForSelector: () => this.waitForSelector(safeParams),
      keyPress: () => this.keyPress(safeParams),
      keyDown: () => this.keyDown(safeParams),
      keyUp: () => this.keyUp(safeParams),
      mouseMove: () => this.mouseMove(safeParams),
      mouseDown: () => this.mouseDown(safeParams),
      mouseUp: () => this.mouseUp(safeParams),
      wheel: () => this.wheel(safeParams),
      dblclick: () => this.dblclick(safeParams),
      drag: () => this.drag(safeParams),
      focus: () => this.focus(safeParams),
      clear: () => this.clear(safeParams),
      getLocalStorage: () => this.getLocalStorage(safeParams),
      setLocalStorage: () => this.setLocalStorage(safeParams),
      removeLocalStorage: () => this.removeLocalStorage(safeParams),
      clearLocalStorage: () => this.clearLocalStorage(safeParams),
      getSessionStorage: () => this.getSessionStorage(safeParams),
      setSessionStorage: () => this.setSessionStorage(safeParams),
      removeSessionStorage: () => this.removeSessionStorage(safeParams),
      clearSessionStorage: () => this.clearSessionStorage(safeParams),
      eval: () => this.evalScript(safeParams),
      addScript: () => this.addScript(safeParams),
      removeScript: () => this.removeScript(safeParams),
      // P2: Frame Management
      listFrames: () => this.listFrames(safeParams),
      switchFrame: () => this.switchFrame(safeParams),
      frameMain: () => this.frameMain(safeParams),
    };

    const handler = handlers[action];
    if (!handler) {
      return {
        success: false,
        error: { code: 5002, message: `Unknown action: ${action}`, recoverable: false },
      };
    }

    try {
      return await handler();
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }
  }

  // ========== Shadow-piercing query helpers ==========
  // Mirror of the real NevofluxChild implementation. A flat querySelector()
  // stops at every open shadow boundary; these descend open shadow roots so
  // selector resolution (probe/click/input/data-ai-id) can reach editors that
  // sites render inside web components (e.g. LinkedIn's Quill composer).
  _deepQuerySelector(selector, doc = this.currentDoc) {
    if (!doc || !selector) {
      return null;
    }
    let flat;
    try {
      flat = doc.querySelector(selector);
    } catch {
      return null;
    }
    if (flat) {
      return flat;
    }
    const roots = [doc];
    while (roots.length) {
      const root = roots.shift();
      let all;
      try {
        all = root.querySelectorAll('*');
      } catch {
        continue;
      }
      for (const el of all) {
        const sr = el.shadowRoot;
        if (!sr) {
          continue;
        }
        try {
          const hit = sr.querySelector(selector);
          if (hit) {
            return hit;
          }
        } catch {}
        roots.push(sr);
      }
    }
    return null;
  }

  _deepQuerySelectorAll(selector, doc = this.currentDoc) {
    const out = [];
    if (!doc || !selector) {
      return out;
    }
    try {
      out.push(...doc.querySelectorAll(selector));
    } catch {
      return out;
    }
    const roots = [doc];
    while (roots.length) {
      const root = roots.shift();
      let all;
      try {
        all = root.querySelectorAll('*');
      } catch {
        continue;
      }
      for (const el of all) {
        const sr = el.shadowRoot;
        if (!sr) {
          continue;
        }
        try {
          out.push(...sr.querySelectorAll(selector));
        } catch {}
        roots.push(sr);
      }
    }
    return out;
  }

  // Shadow-aware active element (mirror of the real NevofluxChild). Descends
  // each focused host's shadowRoot.activeElement so a node focused inside a
  // shadow root is correctly recognized despite doc.activeElement retargeting.
  _deepActiveElement(doc = this.currentDoc) {
    let active = doc?.activeElement || null;
    try {
      while (active?.shadowRoot?.activeElement) {
        active = active.shadowRoot.activeElement;
      }
    } catch {}
    return active;
  }

  // ========== Data Extraction ==========
  getText({ selector }) {
    const el = this.currentDoc?.querySelector(selector);
    return el?.textContent || '';
  }

  getHtml({ selector }) {
    const el = this.currentDoc?.querySelector(selector);
    return el?.innerHTML || '';
  }

  getValue({ selector }) {
    const el = this.document.querySelector(selector);
    return el?.value || '';
  }

  // ========== State Checking ==========
  isVisible({ selector }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc || !win) return false;

    const el = doc.querySelector(selector);
    if (!el) return false;

    const rect = el.getBoundingClientRect();
    const style = win.getComputedStyle(el);

    return (
      rect.width > 0 &&
      rect.height > 0 &&
      style.visibility !== 'hidden' &&
      style.display !== 'none' &&
      style.opacity !== '0'
    );
  }

  exists({ selector }) {
    return this.currentDoc?.querySelector(selector) !== null;
  }

  // ========== Keyboard ==========
  async keyPress({ key, modifiers = [], delay = 0 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    if (!key || typeof key !== 'string') {
      return {
        success: false,
        error: {
          code: 9002,
          message: 'Missing or invalid required parameter: key',
          recoverable: false,
        },
      };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils || typeof domUtils.sendKeyEvent !== 'function') {
        return {
          success: false,
          error: { code: 5001, message: 'windowUtils not available', recoverable: false },
        };
      }

      let modifierFlags = 0;
      if (modifiers.includes('ctrl')) modifierFlags |= 0x02;
      if (modifiers.includes('alt')) modifierFlags |= 0x01;
      if (modifiers.includes('shift')) modifierFlags |= 0x04;
      if (modifiers.includes('meta')) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent('keydown', keyCode, charCode, modifierFlags);
      if (delay > 0) await this.sleep(delay);
      domUtils.sendKeyEvent('keypress', keyCode, charCode, modifierFlags);
      if (delay > 0) await this.sleep(delay);
      domUtils.sendKeyEvent('keyup', keyCode, charCode, modifierFlags);

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  keyDown({ key, modifiers = [] }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils || typeof domUtils.sendKeyEvent !== 'function') {
        return {
          success: false,
          error: { code: 5001, message: 'windowUtils not available', recoverable: false },
        };
      }

      let modifierFlags = 0;
      if (modifiers.includes('ctrl')) modifierFlags |= 0x02;
      if (modifiers.includes('alt')) modifierFlags |= 0x01;
      if (modifiers.includes('shift')) modifierFlags |= 0x04;
      if (modifiers.includes('meta')) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent('keydown', keyCode, charCode, modifierFlags);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  keyUp({ key, modifiers = [] }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils || typeof domUtils.sendKeyEvent !== 'function') {
        return {
          success: false,
          error: { code: 5001, message: 'windowUtils not available', recoverable: false },
        };
      }

      let modifierFlags = 0;
      if (modifiers.includes('ctrl')) modifierFlags |= 0x02;
      if (modifiers.includes('alt')) modifierFlags |= 0x01;
      if (modifiers.includes('shift')) modifierFlags |= 0x04;
      if (modifiers.includes('meta')) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent('keyup', keyCode, charCode, modifierFlags);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  _getKeyCode(key) {
    const keyCodeMap = {
      Enter: 13,
      Tab: 9,
      Escape: 27,
      Backspace: 8,
      Delete: 46,
      ArrowUp: 38,
      ArrowDown: 40,
      ArrowLeft: 37,
      ArrowRight: 39,
      Home: 36,
      End: 35,
      PageUp: 33,
      PageDown: 34,
      F1: 112,
      F2: 113,
      F3: 114,
      F4: 115,
      F5: 116,
      F6: 117,
      F7: 118,
      F8: 119,
      F9: 120,
      F10: 121,
      F11: 122,
      F12: 123,
      Space: 32,
      ' ': 32,
    };
    return keyCodeMap[key] || (key.length === 1 ? key.toUpperCase().charCodeAt(0) : 0);
  }

  // ========== Mouse ==========
  mouseMove({ x, y, steps = 1 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === 'function') {
        domUtils.sendMouseEvent('mousemove', x, y, 0, 0, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseDown({ button = 'left', x, y }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === 'function') {
        const posX = x !== undefined ? x : win.innerWidth / 2;
        const posY = y !== undefined ? y : win.innerHeight / 2;
        domUtils.sendMouseEvent('mousedown', posX, posY, buttonCode, 1, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseUp({ button = 'left', x, y }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === 'function') {
        const posX = x !== undefined ? x : win.innerWidth / 2;
        const posY = y !== undefined ? y : win.innerHeight / 2;
        domUtils.sendMouseEvent('mouseup', posX, posY, buttonCode, 1, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  wheel({ deltaX = 0, deltaY = 0 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendWheelEvent === 'function') {
        const x = win.innerWidth / 2;
        const y = win.innerHeight / 2;
        domUtils.sendWheelEvent(x, y, deltaX, deltaY, 0, 0, 0, 0, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  async dblclick({ selector, button = 'left', delay = 0 }) {
    return this.click({ selector, button, clickCount: 2, delay });
  }

  async click({ selector, button = 'left', clickCount = 1, delay = 0, force = false }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return {
        success: false,
        error: {
          code: 1001,
          message: 'Element not found',
          recoverable: true,
          suggestion: 'Use waitForSelector first',
        },
      };
    }

    try {
      const rect = el.getBoundingClientRect();
      const x = rect.left + rect.width / 2;
      const y = rect.top + rect.height / 2;
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;

      const eventInit = {
        bubbles: true,
        cancelable: true,
        view: win,
        clientX: x,
        clientY: y,
        button: buttonCode,
      };

      for (let i = 0; i < clickCount; i++) {
        el.dispatchEvent(new win.MouseEvent('mouseover', eventInit));
        el.dispatchEvent(new win.MouseEvent('mousedown', eventInit));
        el.dispatchEvent(new win.MouseEvent('mouseup', eventInit));
        el.dispatchEvent(new win.MouseEvent('click', eventInit));

        if (delay > 0 && i < clickCount - 1) {
          await this.sleep(delay);
        }
      }
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    return { success: true };
  }

  async drag({ fromSelector, toSelector, steps = 10 }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc || !win) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const fromEl = doc.querySelector(fromSelector);
    const toEl = doc.querySelector(toSelector);

    if (!fromEl) {
      return {
        success: false,
        error: { code: 1001, message: 'Source element not found', recoverable: true },
      };
    }
    if (!toEl) {
      return {
        success: false,
        error: { code: 1001, message: 'Target element not found', recoverable: true },
      };
    }

    try {
      const fromRect = fromEl.getBoundingClientRect();
      const toRect = toEl.getBoundingClientRect();

      const fromX = fromRect.left + fromRect.width / 2;
      const fromY = fromRect.top + fromRect.height / 2;
      const toX = toRect.left + toRect.width / 2;
      const toY = toRect.top + toRect.height / 2;

      fromEl.dispatchEvent(
        new win.MouseEvent('mousedown', {
          bubbles: true,
          cancelable: true,
          view: win,
          clientX: fromX,
          clientY: fromY,
          button: 0,
        })
      );

      for (let i = 1; i <= steps; i++) {
        const progress = i / steps;
        const x = fromX + (toX - fromX) * progress;
        const y = fromY + (toY - fromY) * progress;

        fromEl.dispatchEvent(
          new win.MouseEvent('mousemove', {
            bubbles: true,
            cancelable: true,
            view: win,
            clientX: x,
            clientY: y,
            button: 0,
          })
        );

        await this.sleep(10);
      }

      toEl.dispatchEvent(
        new win.MouseEvent('mouseup', {
          bubbles: true,
          cancelable: true,
          view: win,
          clientX: toX,
          clientY: toY,
          button: 0,
        })
      );

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  focus({ selector }) {
    const doc = this.doc;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return {
        success: false,
        error: { code: 1001, message: 'Element not found', recoverable: true },
      };
    }

    try {
      el.focus();
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  clear({ selector }) {
    const doc = this.doc;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return {
        success: false,
        error: { code: 1001, message: 'Element not found', recoverable: true },
      };
    }

    try {
      el.focus();
      el.value = '';
      el.dispatchEvent(new this.contentWindow.Event('input', { bubbles: true }));
      el.dispatchEvent(new this.contentWindow.Event('change', { bubbles: true }));
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== Storage ==========
  getLocalStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      if (key) {
        const value = win.localStorage.getItem(key);
        let parsedValue;
        try {
          parsedValue = JSON.parse(value);
        } catch {
          parsedValue = value;
        }
        return { success: true, data: parsedValue };
      } else {
        const result = {};
        for (let i = 0; i < win.localStorage.length; i++) {
          const k = win.localStorage.key(i);
          const v = win.localStorage.getItem(k);
          try {
            result[k] = JSON.parse(v);
          } catch {
            result[k] = v;
          }
        }
        return { success: true, data: result };
      }
    } catch (e) {
      return { success: false, error: { code: 7004, message: String(e), recoverable: false } };
    }
  }

  setLocalStorage({ key, value }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const serialized = typeof value === 'string' ? value : JSON.stringify(value);
      win.localStorage.setItem(key, serialized);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 7003, message: String(e), recoverable: false } };
    }
  }

  removeLocalStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    if (!key) {
      return {
        success: false,
        error: { code: 7002, message: 'Missing required parameter: key', recoverable: false },
      };
    }

    try {
      win.localStorage.removeItem(key);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  clearLocalStorage() {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      win.localStorage.clear();
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  getSessionStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      if (key) {
        const value = win.sessionStorage.getItem(key);
        let parsedValue;
        try {
          parsedValue = JSON.parse(value);
        } catch {
          parsedValue = value;
        }
        return { success: true, data: parsedValue };
      } else {
        const result = {};
        for (let i = 0; i < win.sessionStorage.length; i++) {
          const k = win.sessionStorage.key(i);
          const v = win.sessionStorage.getItem(k);
          try {
            result[k] = JSON.parse(v);
          } catch {
            result[k] = v;
          }
        }
        return { success: true, data: result };
      }
    } catch (e) {
      return { success: false, error: { code: 7004, message: String(e), recoverable: false } };
    }
  }

  setSessionStorage({ key, value }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const serialized = typeof value === 'string' ? value : JSON.stringify(value);
      win.sessionStorage.setItem(key, serialized);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 7003, message: String(e), recoverable: false } };
    }
  }

  removeSessionStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    if (!key) {
      return {
        success: false,
        error: { code: 7002, message: 'Missing required parameter: key', recoverable: false },
      };
    }

    try {
      win.sessionStorage.removeItem(key);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  clearSessionStorage() {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      win.sessionStorage.clear();
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== JavaScript Execution ==========
  evalScript({ script, returnValue = true }) {
    const win = this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

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

    try {
      // REPL-style evaluation (mirror of the real NevofluxChild). Evaluate
      // as-is first (preserves bare expressions / IIFEs and their completion
      // values); on a top-level-`return` SyntaxError, retry wrapped in a
      // function so explicit-return scripts work too.
      let result;
      try {
        result = win.eval(script);
      } catch (inner) {
        const isTopLevelReturn =
          (inner?.name === 'SyntaxError' || inner instanceof SyntaxError) &&
          /\breturn\b/.test(inner?.message || '');
        if (!isTopLevelReturn) {
          throw inner;
        }
        result = win.eval(`(function(){\n${script}\n})();`);
      }

      if (!returnValue) {
        return { success: true };
      }

      let serialized;
      let type = typeof result;

      try {
        if (result === undefined) {
          serialized = undefined;
          type = 'undefined';
        } else if (result === null) {
          serialized = null;
          type = 'null';
        } else {
          serialized = JSON.parse(JSON.stringify(result));
        }
      } catch {
        serialized = String(result);
        type = 'string';
      }

      return {
        success: true,
        value: serialized,
        type,
      };
    } catch (e) {
      return {
        success: false,
        error: {
          code: 9001,
          message: e.message,
          recoverable: false,
        },
      };
    }
  }

  addScript({ script, runAt = 'document_idle' }) {
    const doc = this.doc;
    const win = this.contentWindow;
    if (!doc || !win) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

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

    try {
      const scriptEl = doc.createElement('script');
      scriptEl.textContent = script;
      scriptEl.id = `nevoflux_script_${Date.now()}_${Math.random().toString(36).slice(2)}`;

      if (runAt === 'document_start') {
        doc.documentElement.prepend(scriptEl);
      } else {
        doc.body.appendChild(scriptEl);
      }

      return { success: true, handle: scriptEl.id };
    } catch (e) {
      return { success: false, error: { code: 9001, message: String(e), recoverable: false } };
    }
  }

  removeScript({ handle }) {
    const doc = this.doc;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    if (!handle) {
      return {
        success: false,
        error: { code: 9002, message: 'Missing required parameter: handle', recoverable: false },
      };
    }

    try {
      const scriptEl = doc.getElementById(handle);
      if (scriptEl) {
        scriptEl.remove();
        return { success: true };
      }
      return {
        success: false,
        error: { code: 9003, message: 'Script not found', recoverable: false },
      };
    } catch (e) {
      return { success: false, error: { code: 9001, message: String(e), recoverable: false } };
    }
  }

  // ========== Helpers ==========
  sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  _escapeHtml(s) {
    return String(s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  _generatePathSelector(el) {
    const doc = this.doc;
    if (!doc) return '';
    const parts = [];
    let cur = el;
    let reachedAnchor = false;

    while (cur) {
      if (cur === doc.body || cur === doc.documentElement) {
        reachedAnchor = true;
        break;
      }
      if (cur.id && doc.getElementById && doc.getElementById(cur.id) === cur) {
        parts.unshift(`#${this._cssEscape(cur.id)}`);
        reachedAnchor = true;
        break;
      }
      let part = (cur.tagName || '').toLowerCase();
      const parent = cur.parentElement;
      const siblings = (parent?.children || []).filter(
        c => c.tagName === cur.tagName
      );
      if (siblings.length > 1) {
        const idx = siblings.indexOf(cur) + 1;
        part += `:nth-of-type(${idx})`;
      }
      parts.unshift(part);
      cur = parent;
    }

    // Element was detached from the document tree — return empty path
    if (!reachedAnchor) return '';
    return parts.join(' > ');
  }

  _cssEscape(s) {
    // Minimal CSS.escape polyfill for test context
    if (typeof CSS !== 'undefined' && CSS.escape) return CSS.escape(s);
    return String(s).replace(/[^\w-]/g, ch => `\\${ch}`);
  }

  _findInnermostEditable(root) {
    if (!root) return null;

    const isEditable = (el) => {
      if (!el || typeof el.getAttribute !== 'function') return false;
      const v = el.getAttribute('contenteditable');
      return v === 'true' || v === '' || v === 'plaintext-only';
    };

    const rootIsEditable = isEditable(root);

    let deepest = rootIsEditable ? root : null;
    const queue = [{ el: root, depth: 0 }];
    let maxDepth = rootIsEditable ? 0 : -1;

    while (queue.length > 0) {
      const { el, depth } = queue.shift();
      if (isEditable(el) && depth > maxDepth) {
        deepest = el;
        maxDepth = depth;
      }
      for (const child of el.children || []) {
        queue.push({ el: child, depth: depth + 1 });
      }
    }

    return deepest;
  }

  _detectEditorFramework(_el) {
    // Not unit-tested; relies on matches()/classList which the mock doesn't support richly.
    // Verified manually via fixture HTML pages (see src/nevoflux/tests/e2e/fixtures/editors/).
    return null;
  }

  snapshot() {
    return { tree: '', refs: {} };
  }

  // ========== P2: Frame Management ==========

  listFrames() {
    const doc = this.currentDoc;
    if (!doc) {
      return [];
    }

    const iframes = doc.querySelectorAll('iframe');
    const frames = [];

    for (const iframe of iframes) {
      const rect = iframe.getBoundingClientRect();
      const style = doc.defaultView?.getComputedStyle(iframe);
      const visible =
        rect.width > 0 &&
        rect.height > 0 &&
        style?.visibility !== 'hidden' &&
        style?.display !== 'none';

      frames.push({
        selector: this.generateSelector(iframe),
        url: iframe.src || '',
        name: iframe.name || '',
        visible,
      });
    }

    return frames;
  }

  switchFrame({ selector }) {
    const doc = this.doc;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const searchDoc = this._currentFrameSelector
      ? doc.querySelector(this._currentFrameSelector)?.contentDocument || doc
      : doc;

    const iframe = searchDoc.querySelector(selector);
    if (!iframe || iframe.tagName !== 'IFRAME') {
      return {
        success: false,
        error: { code: 10001, message: `Frame not found: ${selector}`, recoverable: true },
      };
    }

    try {
      const frameDoc = iframe.contentDocument;
      if (!frameDoc) {
        return {
          success: false,
          error: { code: 10002, message: 'Frame access denied (cross-origin)', recoverable: false },
        };
      }

      if (this._currentFrameSelector) {
        this._currentFrameSelector = `${this._currentFrameSelector} ${selector}`;
      } else {
        this._currentFrameSelector = selector;
      }

      return { success: true };
    } catch (e) {
      return {
        success: false,
        error: { code: 10002, message: `Frame access denied: ${e.message}`, recoverable: false },
      };
    }
  }

  frameMain() {
    this._currentFrameSelector = null;
    return { success: true };
  }

  generateSelector(el) {
    if (el.id) {
      return `#${el.id}`;
    }
    return el.tagName?.toLowerCase() || 'unknown';
  }

  type({ selector, text }) {
    const el = this.doc.querySelector(selector);
    if (!el) return { success: false, error: { code: 1001, message: 'Element not found', recoverable: true } };
    const tag = (el.tagName || '').toLowerCase();
    const isStandardInput = tag === 'input' || tag === 'textarea';
    if (isStandardInput) {
      for (const ch of String(text)) {
        const current = el.value ?? '';
        el.value = current + ch;
      }
      return { success: true };
    }
    for (const ch of String(text)) {
      const r = this.paste({ selector, text: ch });
      if (!r.success) return r;
    }
    return { success: true };
  }

  fill({ selector, text }) {
    const el = this.doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: 'Element not found', recoverable: true } };
    }
    const tag = (el.tagName || '').toLowerCase();
    const isStandardInput = (tag === 'input' || tag === 'textarea') && typeof el.value === 'string';
    if (isStandardInput) {
      el.value = '';
      el.value = text;
      return { success: true };
    }
    return this.fillRichText({ selector, text });
  }

  async waitForSelector({ selector, timeout = 30000, state = 'visible' }) {
    if (!this.doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const el = this.doc.querySelector(selector);

      const stateChecks = {
        attached: () => el !== null,
        detached: () => el === null,
        visible: () => el && this.isVisible({ selector }),
        hidden: () => !el || !this.isVisible({ selector }),
      };

      if (stateChecks[state]?.()) {
        return { success: true };
      }

      await this.sleep(100);
    }

    return {
      success: false,
      error: { code: 4001, message: `Timeout waiting for ${selector}`, recoverable: true },
    };
  }

  queryAll({ selector, limit = 50 }) {
    if (!selector || typeof selector !== 'string') {
      return { success: false, error: { code: 1007, message: 'Invalid selector', recoverable: false } };
    }
    const doc = this.doc;
    let matches;
    try {
      matches = doc.querySelectorAll(selector);
    } catch (e) {
      return { success: false, error: { code: 1007, message: e.message, recoverable: false } };
    }
    const arr = Array.isArray(matches) ? matches : Array.from(matches || []);
    const clamp = Math.max(1, Math.min(Number(limit) || 50, 500));
    const results = [];
    for (let i = 0; i < Math.min(arr.length, clamp); i++) {
      const el = arr[i];
      const rect = el.getBoundingClientRect?.() || { width: 0, height: 0 };
      results.push({
        tag: (el.tagName || '').toLowerCase(),
        id: el.id || null,
        text: (el.textContent || '').substring(0, 100),
        visible: rect.width > 0 && rect.height > 0,
        path_selector: this._generatePathSelector(el),
      });
    }
    return { success: true, result: { count: arr.length, elements: results } };
  }

  probe({ selector }) {
    if (!selector) {
      return { success: false, error: { code: 1007, message: 'selector required', recoverable: false } };
    }
    const doc = this.doc;
    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: `Element not found: ${selector}`, recoverable: true } };
    }

    // contentEditable detection — mirrors NevofluxChild.sys.mjs probe fingerprint.
    // Walk ancestors for any editable contenteditable value (true / "" / plaintext-only).
    let isCE = false;
    let cEHost = null;
    for (let cur = el; cur && cur !== doc; cur = cur.parentElement) {
      if (typeof cur.getAttribute === 'function') {
        const v = cur.getAttribute('contenteditable');
        if (v === 'true' || v === '' || v === 'plaintext-only') {
          isCE = true;
          cEHost = cur;
          break;
        }
      }
    }
    const innermostEl = isCE ? this._findInnermostEditable(cEHost) : null;
    const innermostSelector = innermostEl ? this._generatePathSelector(innermostEl) : null;

    return {
      success: true,
      result: {
        tag: (el.tagName || '').toLowerCase(),
        input_type: null,
        has_value_property: false,
        is_content_editable: isCE,
        disabled: false,
        readonly: false,
        is_visible: true,
        is_focusable: false,
        editor_framework: null,
        react_fiber_present: false,
        inside_iframe: false,
        shadow_root_depth: 0,
        innermost_editable_selector: innermostSelector,
        computed_role: typeof el.getAttribute === 'function' ? el.getAttribute('role') : null,
      },
    };
  }

  paste({ selector, text }) {
    if (!selector || text === undefined) {
      return { success: false, error: { code: 9002, message: 'selector and text required', recoverable: false } };
    }
    const el = this.doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: `Element not found: ${selector}`, recoverable: true } };
    }
    // Mock: record that paste was called with given text on given target
    el._lastPastedText = String(text);
    return { success: true };
  }

  fillRichText({ selector, text }) {
    if (!selector || text === undefined) {
      return { success: false, error: { code: 9002, message: 'selector and text required', recoverable: false } };
    }
    const el = this.doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: `Element not found: ${selector}`, recoverable: true } };
    }
    // Mock: clear + paste
    el.textContent = '';
    return this.paste({ selector, text });
  }
}

// ========== TEST SUITES ==========

let child;

describe('NevofluxChild - Execute Handler', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('should return error for unknown action', async () => {
    const result = await child.execute('unknownAction', {});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(5002);
    expect(result.error.message).toContain('Unknown action');
  });

  it('should handle null params', async () => {
    const result = await child.execute('getLocalStorage', null);
    expect(result.success).toBe(true);
  });
});

describe('NevofluxChild - Keyboard Methods', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('keyPress should succeed with valid key', async () => {
    const result = await child.keyPress({ key: 'Enter' });
    expect(result.success).toBe(true);
  });

  it('keyPress should fail without key', async () => {
    const result = await child.keyPress({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('keyPress should handle modifiers', async () => {
    const result = await child.keyPress({ key: 'a', modifiers: ['ctrl', 'shift'] });
    expect(result.success).toBe(true);
  });

  it('keyDown should succeed', () => {
    const result = child.keyDown({ key: 'Shift' });
    expect(result.success).toBe(true);
  });

  it('keyUp should succeed', () => {
    const result = child.keyUp({ key: 'Shift' });
    expect(result.success).toBe(true);
  });

  it('_getKeyCode should return correct codes', () => {
    expect(child._getKeyCode('Enter')).toBe(13);
    expect(child._getKeyCode('Tab')).toBe(9);
    expect(child._getKeyCode('Escape')).toBe(27);
    expect(child._getKeyCode('ArrowUp')).toBe(38);
    expect(child._getKeyCode('a')).toBe(65);
  });
});

describe('NevofluxChild - Mouse Methods', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('mouseMove should succeed', () => {
    const result = child.mouseMove({ x: 100, y: 200 });
    expect(result.success).toBe(true);
  });

  it('mouseDown should succeed with default button', () => {
    const result = child.mouseDown({});
    expect(result.success).toBe(true);
  });

  it('mouseDown should handle button parameter', () => {
    const result = child.mouseDown({ button: 'right' });
    expect(result.success).toBe(true);
  });

  it('mouseDown should use custom coordinates', () => {
    const result = child.mouseDown({ x: 50, y: 75 });
    expect(result.success).toBe(true);
  });

  it('mouseUp should succeed', () => {
    const result = child.mouseUp({});
    expect(result.success).toBe(true);
  });

  it('wheel should succeed with deltaY', () => {
    const result = child.wheel({ deltaY: 100 });
    expect(result.success).toBe(true);
  });

  it('wheel should succeed with deltaX', () => {
    const result = child.wheel({ deltaX: 50 });
    expect(result.success).toBe(true);
  });

  it('focus should fail for non-existent element', () => {
    const result = child.focus({ selector: '#nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(1001);
  });

  it('clear should fail for non-existent element', () => {
    const result = child.clear({ selector: '#nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(1001);
  });
});

describe('NevofluxChild - Click and Drag', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    // Add test elements
    const input = child.document.createElement('input');
    input.id = 'testInput';
    child.document.body.appendChild(input);

    const from = child.document.createElement('div');
    from.id = 'fromEl';
    child.document.body.appendChild(from);

    const to = child.document.createElement('div');
    to.id = 'toEl';
    child.document.body.appendChild(to);
  });

  it('click should succeed on existing element', async () => {
    const result = await child.click({ selector: '#testInput' });
    expect(result.success).toBe(true);
  });

  it('click should fail on non-existent element', async () => {
    const result = await child.click({ selector: '#nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(1001);
  });

  it('dblclick should succeed', async () => {
    const result = await child.dblclick({ selector: '#testInput' });
    expect(result.success).toBe(true);
  });

  it('drag should succeed with valid selectors', async () => {
    const result = await child.drag({ fromSelector: '#fromEl', toSelector: '#toEl' });
    expect(result.success).toBe(true);
  });

  it('drag should fail with invalid from selector', async () => {
    const result = await child.drag({ fromSelector: '#nonexistent', toSelector: '#toEl' });
    expect(result.success).toBe(false);
    expect(result.error.message).toContain('Source element');
  });

  it('drag should fail with invalid to selector', async () => {
    const result = await child.drag({ fromSelector: '#fromEl', toSelector: '#nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.message).toContain('Target element');
  });
});

describe('NevofluxChild - LocalStorage', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    child.contentWindow.localStorage.clear();
  });

  it('setLocalStorage should set string value', () => {
    const result = child.setLocalStorage({ key: 'test', value: 'hello' });
    expect(result.success).toBe(true);
  });

  it('setLocalStorage should set object value', () => {
    const result = child.setLocalStorage({ key: 'obj', value: { foo: 'bar' } });
    expect(result.success).toBe(true);
  });

  it('getLocalStorage should return null for non-existent key', () => {
    const result = child.getLocalStorage({ key: 'nonexistent' });
    expect(result.success).toBe(true);
    expect(result.data).toBeNull();
  });

  it('getLocalStorage should return stored value', () => {
    child.setLocalStorage({ key: 'test', value: 'hello' });
    const result = child.getLocalStorage({ key: 'test' });
    expect(result.success).toBe(true);
    expect(result.data).toBe('hello');
  });

  it('getLocalStorage should parse JSON values', () => {
    child.setLocalStorage({ key: 'obj', value: { foo: 'bar' } });
    const result = child.getLocalStorage({ key: 'obj' });
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ foo: 'bar' });
  });

  it('getLocalStorage should return all items when no key provided', () => {
    child.setLocalStorage({ key: 'a', value: 1 });
    child.setLocalStorage({ key: 'b', value: 2 });
    const result = child.getLocalStorage({});
    expect(result.success).toBe(true);
    expect(result.data).toHaveProperty('a', 1);
    expect(result.data).toHaveProperty('b', 2);
  });

  it('removeLocalStorage should remove item', () => {
    child.setLocalStorage({ key: 'test', value: 'hello' });
    const result = child.removeLocalStorage({ key: 'test' });
    expect(result.success).toBe(true);
    const get = child.getLocalStorage({ key: 'test' });
    expect(get.data).toBeNull();
  });

  it('removeLocalStorage should fail without key', () => {
    const result = child.removeLocalStorage({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('clearLocalStorage should remove all items', () => {
    child.setLocalStorage({ key: 'a', value: 1 });
    child.setLocalStorage({ key: 'b', value: 2 });
    const result = child.clearLocalStorage();
    expect(result.success).toBe(true);
    const get = child.getLocalStorage({});
    expect(get.data).toEqual({});
  });
});

describe('NevofluxChild - SessionStorage', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    child.contentWindow.sessionStorage.clear();
  });

  it('setSessionStorage should set value', () => {
    const result = child.setSessionStorage({ key: 'test', value: 'session' });
    expect(result.success).toBe(true);
  });

  it('getSessionStorage should return stored value', () => {
    child.setSessionStorage({ key: 'test', value: { session: true } });
    const result = child.getSessionStorage({ key: 'test' });
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ session: true });
  });

  it('removeSessionStorage should remove item', () => {
    child.setSessionStorage({ key: 'test', value: 'hello' });
    child.removeSessionStorage({ key: 'test' });
    const result = child.getSessionStorage({ key: 'test' });
    expect(result.data).toBeNull();
  });

  it('removeSessionStorage should fail without key', () => {
    const result = child.removeSessionStorage({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(7002);
  });

  it('clearSessionStorage should remove all items', () => {
    child.setSessionStorage({ key: 'a', value: 1 });
    child.setSessionStorage({ key: 'b', value: 2 });
    child.clearSessionStorage();
    const result = child.getSessionStorage({});
    expect(result.data).toEqual({});
  });
});

describe('NevofluxChild - JavaScript Execution', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('evalScript should execute simple expression', () => {
    const result = child.evalScript({ script: '1 + 1' });
    expect(result.success).toBe(true);
    expect(result.value).toBe(2);
    expect(result.type).toBe('number');
  });

  it('evalScript should return string type', () => {
    const result = child.evalScript({ script: '"hello"' });
    expect(result.success).toBe(true);
    expect(result.value).toBe('hello');
    expect(result.type).toBe('string');
  });

  it('evalScript should return object', () => {
    const result = child.evalScript({ script: '({foo: "bar"})' });
    expect(result.success).toBe(true);
    expect(result.value).toEqual({ foo: 'bar' });
    expect(result.type).toBe('object');
  });

  it('evalScript should return null', () => {
    const result = child.evalScript({ script: 'null' });
    expect(result.success).toBe(true);
    expect(result.value).toBeNull();
    expect(result.type).toBe('null');
  });

  it('evalScript should return undefined', () => {
    const result = child.evalScript({ script: 'undefined' });
    expect(result.success).toBe(true);
    expect(result.value).toBeUndefined();
    expect(result.type).toBe('undefined');
  });

  it('evalScript should fail with invalid script', () => {
    const result = child.evalScript({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('evalScript should return error on syntax error', () => {
    const result = child.evalScript({ script: 'invalid syntax {{{' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9001);
  });

  it('evalScript with returnValue false should not return value', () => {
    const result = child.evalScript({ script: '1 + 1', returnValue: false });
    expect(result.success).toBe(true);
    expect(result.value).toBeUndefined();
  });

  // REPL-style return support — agents frequently write top-level `return ...`,
  // which is a SyntaxError at program top level and used to be swallowed as
  // "(no output)". The function-wrap fallback makes these work without
  // regressing bare-expression / IIFE completion values.
  it('evalScript supports explicit top-level return (single line)', () => {
    const result = child.evalScript({ script: 'return 41 + 1;' });
    expect(result.success).toBe(true);
    expect(result.value).toBe(42);
    expect(result.type).toBe('number');
  });

  it('evalScript supports top-level return after statements', () => {
    const result = child.evalScript({
      script: 'const a = 6;\nconst b = 7;\nreturn a * b;',
    });
    expect(result.success).toBe(true);
    expect(result.value).toBe(42);
  });

  it('evalScript supports top-level return of an object', () => {
    const result = child.evalScript({
      script: 'const o = { ok: true, n: 2 };\nreturn o;',
    });
    expect(result.success).toBe(true);
    expect(result.value).toEqual({ ok: true, n: 2 });
    expect(result.type).toBe('object');
  });

  it('evalScript still returns completion value for bare expressions (no regression)', () => {
    const result = child.evalScript({ script: '"a" + "|" + (1 + 2)' });
    expect(result.success).toBe(true);
    expect(result.value).toBe('a|3');
  });

  it('evalScript still evaluates an IIFE with return (no regression)', () => {
    const result = child.evalScript({ script: '(function(){ return 5 * 5; })()' });
    expect(result.success).toBe(true);
    expect(result.value).toBe(25);
  });

  it('addScript should inject script element', () => {
    const result = child.addScript({ script: 'console.log("test")' });
    expect(result.success).toBe(true);
    expect(result.handle).toBeDefined();
    expect(result.handle).toMatch(/^nevoflux_script_/);
  });

  it('addScript should fail without script', () => {
    const result = child.addScript({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('removeScript should remove injected script', () => {
    const add = child.addScript({ script: 'console.log("test")' });
    expect(add.success).toBe(true);
    const result = child.removeScript({ handle: add.handle });
    expect(result.success).toBe(true);
  });

  it('removeScript should fail without handle', () => {
    const result = child.removeScript({});
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9002);
  });

  it('removeScript should fail for non-existent script', () => {
    const result = child.removeScript({ handle: 'nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(9003);
  });
});

describe('NevofluxChild - Data Extraction', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    const div = child.document.createElement('div');
    div.id = 'content';
    div.textContent = 'Hello World';
    div.innerHTML = '<span>Hello</span> World';
    child.document.body.appendChild(div);

    const input = child.document.createElement('input');
    input.id = 'input';
    input.value = 'test value';
    child.document.body.appendChild(input);
  });

  it('getText should return text content', () => {
    const result = child.getText({ selector: '#content' });
    expect(result).toContain('Hello');
  });

  it('getText should return empty string for non-existent element', () => {
    const result = child.getText({ selector: '#nonexistent' });
    expect(result).toBe('');
  });

  it('getHtml should return innerHTML', () => {
    const result = child.getHtml({ selector: '#content' });
    expect(result).toContain('<span>');
  });

  it('getValue should return input value', () => {
    const result = child.getValue({ selector: '#input' });
    expect(result).toBe('test value');
  });
});

describe('NevofluxChild - State Checking', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    const visible = child.document.createElement('div');
    visible.id = 'visible';
    child.document.body.appendChild(visible);
  });

  it('exists should return true for existing element', () => {
    const result = child.exists({ selector: '#visible' });
    expect(result).toBe(true);
  });

  it('exists should return false for non-existent element', () => {
    const result = child.exists({ selector: '#nonexistent' });
    expect(result).toBe(false);
  });

  it('isVisible should return true for visible element', () => {
    const result = child.isVisible({ selector: '#visible' });
    expect(result).toBe(true);
  });

  it('isVisible should return false for non-existent element', () => {
    const result = child.isVisible({ selector: '#nonexistent' });
    expect(result).toBe(false);
  });
});

describe('NevofluxChild - Type and Fill', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    const input = child.document.createElement('input');
    input.id = 'input';
    input.value = '';
    child.document.body.appendChild(input);
  });

  it('type should append text to input', () => {
    child.type({ selector: '#input', text: 'hello' });
    const input = child.document.getElementById('input');
    expect(input.value).toBe('hello');
  });

  it('fill should replace input value', () => {
    const input = child.document.getElementById('input');
    input.value = 'old';
    child.fill({ selector: '#input', text: 'new' });
    expect(input.value).toBe('new');
  });

  it('type should fail for non-existent element', () => {
    const result = child.type({ selector: '#nonexistent', text: 'test' });
    expect(result.success).toBe(false);
  });

  it('fill should fail for non-existent element', () => {
    const result = child.fill({ selector: '#nonexistent', text: 'test' });
    expect(result.success).toBe(false);
  });
});

describe('NevofluxChild - Snapshot', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('snapshot should return tree and refs', () => {
    const result = child.snapshot({});
    expect(result).toHaveProperty('tree');
    expect(result).toHaveProperty('refs');
  });

  it('snapshot should handle empty options', () => {
    const result = child.snapshot({});
    expect(result.tree).toBeDefined();
  });
});

describe('NevofluxChild - WaitForSelector', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    const div = child.document.createElement('div');
    div.id = 'existing';
    child.document.body.appendChild(div);
  });

  it('waitForSelector should succeed for existing element (attached)', async () => {
    const result = await child.waitForSelector({
      selector: '#existing',
      state: 'attached',
      timeout: 100,
    });
    expect(result.success).toBe(true);
  });

  it('waitForSelector should succeed for existing visible element', async () => {
    const result = await child.waitForSelector({
      selector: '#existing',
      state: 'visible',
      timeout: 100,
    });
    expect(result.success).toBe(true);
  });

  it('waitForSelector should timeout for non-existent element', async () => {
    const result = await child.waitForSelector({
      selector: '#nonexistent',
      state: 'attached',
      timeout: 100,
    });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(4001);
  });

  it('waitForSelector should succeed for detached state on non-existent', async () => {
    const result = await child.waitForSelector({
      selector: '#nonexistent',
      state: 'detached',
      timeout: 100,
    });
    expect(result.success).toBe(true);
  });
});

describe('NevofluxChild - Execute Handler Coverage', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
    const div = child.document.createElement('div');
    div.id = 'testDiv';
    child.document.body.appendChild(div);
  });

  it('execute getText should work', async () => {
    const result = await child.execute('getText', { selector: 'body' });
    expect(typeof result).toBe('string');
  });

  it('execute getHtml should work', async () => {
    const result = await child.execute('getHtml', { selector: 'body' });
    expect(typeof result).toBe('string');
  });

  it('execute getValue should work', async () => {
    const result = await child.execute('getValue', { selector: '#testDiv' });
    expect(result).toBe('');
  });

  it('execute snapshot should work', async () => {
    const result = await child.execute('snapshot', {});
    expect(result).toHaveProperty('tree');
  });

  it('execute isVisible should work', async () => {
    const result = await child.execute('isVisible', { selector: '#testDiv' });
    expect(typeof result).toBe('boolean');
  });

  it('execute exists should work', async () => {
    const result = await child.execute('exists', { selector: '#testDiv' });
    expect(result).toBe(true);
  });

  it('execute focus should work', async () => {
    const result = await child.execute('focus', { selector: '#testDiv' });
    expect(result.success).toBe(true);
  });

  it('execute mouseMove should work', async () => {
    const result = await child.execute('mouseMove', { x: 100, y: 100 });
    expect(result.success).toBe(true);
  });

  it('execute wheel should work', async () => {
    const result = await child.execute('wheel', { deltaY: 50 });
    expect(result.success).toBe(true);
  });

  it('execute keyPress with missing key should fail', async () => {
    const result = await child.execute('keyPress', {});
    expect(result.success).toBe(false);
  });
});

// ========== P2: Frame Management Tests ==========

describe('NevofluxChild - Frame Management (P2)', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('listFrames should return empty array when no iframes', () => {
    const frames = child.listFrames();
    expect(Array.isArray(frames)).toBe(true);
    expect(frames.length).toBe(0);
  });

  it('listFrames should return iframe info', () => {
    // Create mock iframe
    const iframe = child.document.createElement('iframe');
    iframe.id = 'paymentFrame';
    iframe.src = 'https://stripe.com/payment';
    iframe.name = 'payment-iframe';
    // Set up contentDocument for accessible iframe
    const iframeDoc = createMockDocument();
    const iframeWin = createMockWindow(iframeDoc);
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = iframeWin;
    child.document.body.appendChild(iframe);

    const frames = child.listFrames();
    expect(frames.length).toBe(1);
    expect(frames[0].url).toBe('https://stripe.com/payment');
    expect(frames[0].name).toBe('payment-iframe');
    expect(frames[0].visible).toBe(true);
  });

  it('listFrames should return multiple iframes', () => {
    // Create two iframes
    const iframe1 = child.document.createElement('iframe');
    iframe1.id = 'frame1';
    iframe1.src = 'https://a.com';
    iframe1.contentDocument = createMockDocument();
    child.document.body.appendChild(iframe1);

    const iframe2 = child.document.createElement('iframe');
    iframe2.id = 'frame2';
    iframe2.src = 'https://b.com';
    iframe2.contentDocument = createMockDocument();
    child.document.body.appendChild(iframe2);

    const frames = child.listFrames();
    expect(frames.length).toBe(2);
  });

  it('switchFrame should succeed for existing accessible iframe', () => {
    const iframe = child.document.createElement('iframe');
    iframe.id = 'testFrame';
    const iframeDoc = createMockDocument();
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = createMockWindow(iframeDoc);
    child.document.body.appendChild(iframe);

    const result = child.switchFrame({ selector: '#testFrame' });
    expect(result.success).toBe(true);
    expect(child._currentFrameSelector).toBe('#testFrame');
  });

  it('switchFrame should fail for non-existent iframe', () => {
    const result = child.switchFrame({ selector: '#nonexistent' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(10001);
  });

  it('switchFrame should fail for cross-origin iframe', () => {
    const iframe = child.document.createElement('iframe');
    iframe.id = 'crossOriginFrame';
    iframe.src = 'https://different-domain.com';
    // Simulate cross-origin: contentDocument is null
    iframe.contentDocument = null;
    child.document.body.appendChild(iframe);

    const result = child.switchFrame({ selector: '#crossOriginFrame' });
    expect(result.success).toBe(false);
    expect(result.error.code).toBe(10002);
  });

  it('frameMain should reset frame context', () => {
    child._currentFrameSelector = '#someFrame';
    const result = child.frameMain();
    expect(result.success).toBe(true);
    expect(child._currentFrameSelector).toBeNull();
  });

  it('switchFrame should build compound selector for nested frames', () => {
    // Set up first frame
    const iframe1 = child.document.createElement('iframe');
    iframe1.id = 'outerFrame';
    const innerDoc = createMockDocument();
    iframe1.contentDocument = innerDoc;
    iframe1.contentWindow = createMockWindow(innerDoc);
    child.document.body.appendChild(iframe1);

    // Switch to outer frame
    child.switchFrame({ selector: '#outerFrame' });
    expect(child._currentFrameSelector).toBe('#outerFrame');

    // Create inner frame in outer frame's document
    const iframe2 = innerDoc.createElement('iframe');
    iframe2.id = 'innerFrame';
    iframe2.contentDocument = createMockDocument();
    innerDoc.body.appendChild(iframe2);

    // Switch to inner frame (nested)
    child.switchFrame({ selector: '#innerFrame' });
    expect(child._currentFrameSelector).toBe('#outerFrame #innerFrame');
  });

  it('currentDoc should return frame document when switched', () => {
    const iframe = child.document.createElement('iframe');
    iframe.id = 'targetFrame';
    const iframeDoc = createMockDocument();
    // Add distinct element to iframe document
    const innerDiv = iframeDoc.createElement('div');
    innerDiv.id = 'insideFrame';
    iframeDoc.body.appendChild(innerDiv);
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = createMockWindow(iframeDoc);
    child.document.body.appendChild(iframe);

    // Before switch, currentDoc is main doc
    expect(child.currentDoc).toBe(child.doc);

    // Switch to frame
    child.switchFrame({ selector: '#targetFrame' });

    // After switch, currentDoc should be frame doc
    expect(child.currentDoc).toBe(iframeDoc);
  });

  it('execute listFrames should work', async () => {
    const result = await child.execute('listFrames', {});
    expect(Array.isArray(result)).toBe(true);
  });

  it('execute switchFrame should work', async () => {
    const iframe = child.document.createElement('iframe');
    iframe.id = 'execFrame';
    iframe.contentDocument = createMockDocument();
    child.document.body.appendChild(iframe);

    const result = await child.execute('switchFrame', { selector: '#execFrame' });
    expect(result.success).toBe(true);
  });

  it('execute frameMain should work', async () => {
    child._currentFrameSelector = '#someFrame';
    const result = await child.execute('frameMain', {});
    expect(result.success).toBe(true);
  });
});

describe('NevofluxChild - Frame Context Integration', () => {
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('getText should use frame document when switched', () => {
    // Set up iframe with distinct content
    const iframe = child.document.createElement('iframe');
    iframe.id = 'contentFrame';
    const iframeDoc = createMockDocument();
    const div = iframeDoc.createElement('div');
    div.id = 'frameContent';
    div.textContent = 'Content inside iframe';
    iframeDoc.body.appendChild(div);
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = createMockWindow(iframeDoc);
    child.document.body.appendChild(iframe);

    // Switch to frame
    child.switchFrame({ selector: '#contentFrame' });

    // getText should now search in frame document
    const text = child.getText({ selector: '#frameContent' });
    expect(text).toBe('Content inside iframe');
  });

  it('exists should check frame document when switched', () => {
    const iframe = child.document.createElement('iframe');
    iframe.id = 'existsFrame';
    const iframeDoc = createMockDocument();
    const innerEl = iframeDoc.createElement('button');
    innerEl.id = 'frameButton';
    iframeDoc.body.appendChild(innerEl);
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = createMockWindow(iframeDoc);
    child.document.body.appendChild(iframe);

    // Before switch - element doesn't exist in main doc
    expect(child.exists({ selector: '#frameButton' })).toBe(false);

    // Switch to frame
    child.switchFrame({ selector: '#existsFrame' });

    // After switch - element exists in frame doc
    expect(child.exists({ selector: '#frameButton' })).toBe(true);
  });

  it('frameMain should restore main document context', () => {
    // Add element to main doc
    const mainDiv = child.document.createElement('div');
    mainDiv.id = 'mainElement';
    child.document.body.appendChild(mainDiv);

    // Set up iframe without the main element
    const iframe = child.document.createElement('iframe');
    iframe.id = 'switchFrame';
    const iframeDoc = createMockDocument();
    iframe.contentDocument = iframeDoc;
    iframe.contentWindow = createMockWindow(iframeDoc);
    child.document.body.appendChild(iframe);

    // Switch to frame - main element not visible
    child.switchFrame({ selector: '#switchFrame' });
    expect(child.exists({ selector: '#mainElement' })).toBe(false);

    // Return to main
    child.frameMain();
    expect(child.exists({ selector: '#mainElement' })).toBe(true);
  });
});

describe('NevofluxChild — _escapeHtml helper', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('escapes ampersand', () => {
    expect(child._escapeHtml('a & b')).toBe('a &amp; b');
  });

  it('escapes less-than and greater-than', () => {
    expect(child._escapeHtml('<div>')).toBe('&lt;div&gt;');
  });

  it('escapes double quote', () => {
    expect(child._escapeHtml('say "hi"')).toBe('say &quot;hi&quot;');
  });

  it('escapes single quote', () => {
    expect(child._escapeHtml("it's")).toBe('it&#39;s');
  });

  it('escapes all entities in a mixed string', () => {
    expect(child._escapeHtml('<a href="x">y & z</a>'))
      .toBe('&lt;a href=&quot;x&quot;&gt;y &amp; z&lt;/a&gt;');
  });

  it('returns empty string for empty input', () => {
    expect(child._escapeHtml('')).toBe('');
  });

  it('returns unchanged string when no entities present', () => {
    expect(child._escapeHtml('hello world')).toBe('hello world');
  });
});

describe('NevofluxChild — _generatePathSelector helper', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('returns id selector when element has unique id', () => {
    const doc = child.doc;
    // Build: <body><div id="target"></div></body>
    const div = doc.createElement('div');
    div.id = 'target';
    doc.body.appendChild(div);
    doc._registerElementById?.('target', div); // mock-specific registration

    expect(child._generatePathSelector(div)).toBe('#target');
  });

  it('uses nth-of-type for siblings of same tag', () => {
    const doc = child.doc;
    const ul = doc.createElement('ul');
    const li1 = doc.createElement('li');
    const li2 = doc.createElement('li');
    const li3 = doc.createElement('li');
    ul.appendChild(li1);
    ul.appendChild(li2);
    ul.appendChild(li3);
    doc.body.appendChild(ul);

    expect(child._generatePathSelector(li2)).toBe('ul > li:nth-of-type(2)');
  });

  it('omits nth-of-type when only one sibling of tag', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    const p = doc.createElement('p');
    div.appendChild(p);
    doc.body.appendChild(div);

    expect(child._generatePathSelector(p)).toBe('div > p');
  });

  it('builds a multi-level path', () => {
    const doc = child.doc;
    const outer = doc.createElement('div');
    const mid = doc.createElement('section');
    const inner = doc.createElement('span');
    outer.appendChild(mid);
    mid.appendChild(inner);
    doc.body.appendChild(outer);

    expect(child._generatePathSelector(inner)).toBe('div > section > span');
  });

  it('returns empty string for element detached from document', () => {
    const doc = child.doc;
    const orphan = doc.createElement('div');
    // NOT appended to doc.body — element is detached
    expect(child._generatePathSelector(orphan)).toBe('');
  });
});

describe('NevofluxChild — _findInnermostEditable helper', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('returns the root itself when no deeper editable exists', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    div.attributes.set('contenteditable', 'true');
    div.getAttribute = k => div.attributes.get(k) || null;

    expect(child._findInnermostEditable(div)).toBe(div);
  });

  it('finds a deeper contenteditable child', () => {
    const doc = child.doc;
    const outer = doc.createElement('div');
    outer.attributes.set('contenteditable', 'true');
    outer.getAttribute = k => outer.attributes.get(k) || null;

    const inner = doc.createElement('div');
    inner.attributes.set('contenteditable', 'true');
    inner.getAttribute = k => inner.attributes.get(k) || null;

    outer.appendChild(inner);

    expect(child._findInnermostEditable(outer)).toBe(inner);
  });

  it('descends through non-editable wrappers to reach innermost editable', () => {
    const doc = child.doc;
    const root = doc.createElement('div');
    root.attributes.set('contenteditable', 'true');
    root.getAttribute = k => root.attributes.get(k) || null;

    const wrapper = doc.createElement('div');
    wrapper.getAttribute = () => null;
    wrapper.attributes = new Map();

    const leaf = doc.createElement('div');
    leaf.attributes.set('contenteditable', 'true');
    leaf.getAttribute = k => leaf.attributes.get(k) || null;

    wrapper.appendChild(leaf);
    root.appendChild(wrapper);

    expect(child._findInnermostEditable(root)).toBe(leaf);
  });

  it('returns null if passed a non-editable root with no editable descendants', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    div.getAttribute = () => null;

    expect(child._findInnermostEditable(div)).toBe(null);
  });

  it('accepts contenteditable="" as true (HTML attribute form)', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    div.attributes.set('contenteditable', '');
    div.getAttribute = k => div.attributes.get(k) ?? null;

    expect(child._findInnermostEditable(div)).toBe(div);
  });

  it('accepts contenteditable="plaintext-only" as editable (Gmail compose)', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    div.attributes.set('contenteditable', 'plaintext-only');
    div.getAttribute = k => div.attributes.get(k) ?? null;

    expect(child._findInnermostEditable(div)).toBe(div);
  });

  it('rejects contenteditable="false"', () => {
    const doc = child.doc;
    const div = doc.createElement('div');
    div.attributes.set('contenteditable', 'false');
    div.getAttribute = k => div.attributes.get(k) ?? null;

    expect(child._findInnermostEditable(div)).toBe(null);
  });
});

describe('NevofluxChild — queryAll method', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('rejects missing selector', () => {
    const r = child.queryAll({ selector: null });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(1007);
  });

  it('returns empty array when no matches', () => {
    const r = child.queryAll({ selector: 'nonexistent' });
    expect(r.success).toBe(true);
    expect(r.result.count).toBe(0);
    expect(r.result.elements.length).toBe(0);
  });
});

describe('NevofluxChild — probe method', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('rejects missing selector', () => {
    const r = child.probe({});
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(1007);
  });

  it('returns element-not-found for unknown selector', () => {
    const r = child.probe({ selector: '#definitely-not-there' });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(1001);
  });

  // Fix B: probe must recognise every editable contenteditable value, including
  // plaintext-only (Gmail compose). Before the fix, plaintext-only reported
  // is_content_editable:false and the daemon strategy engine aborted.
  const makeEditable = (id, ceValue) => {
    const el = child.doc.createElement('div');
    el.setAttribute('id', id);
    if (ceValue !== null) el.setAttribute('contenteditable', ceValue);
    child.doc.body.appendChild(el);
    return el;
  };

  it('reports is_content_editable=true for contenteditable="true"', () => {
    makeEditable('ce-true', 'true');
    const r = child.probe({ selector: '#ce-true' });
    expect(r.success).toBe(true);
    expect(r.result.is_content_editable).toBe(true);
  });

  it('reports is_content_editable=true for contenteditable=""', () => {
    makeEditable('ce-empty', '');
    const r = child.probe({ selector: '#ce-empty' });
    expect(r.result.is_content_editable).toBe(true);
  });

  it('reports is_content_editable=true for contenteditable="plaintext-only" (Gmail compose)', () => {
    makeEditable('ce-plain', 'plaintext-only');
    const r = child.probe({ selector: '#ce-plain' });
    expect(r.result.is_content_editable).toBe(true);
    // innermost must resolve to the host itself, not null
    expect(r.result.innermost_editable_selector).not.toBe(null);
  });

  it('reports is_content_editable=false for contenteditable="false"', () => {
    makeEditable('ce-false', 'false');
    const r = child.probe({ selector: '#ce-false' });
    expect(r.result.is_content_editable).toBe(false);
  });

  it('reports is_content_editable=false for a plain div', () => {
    makeEditable('ce-plain-div', null);
    const r = child.probe({ selector: '#ce-plain-div' });
    expect(r.result.is_content_editable).toBe(false);
  });

  it('reports is_content_editable=true for a child inside a plaintext-only host', () => {
    const host = child.doc.createElement('div');
    host.setAttribute('id', 'pt-host');
    host.setAttribute('contenteditable', 'plaintext-only');
    const inner = child.doc.createElement('span');
    inner.setAttribute('id', 'pt-inner');
    host.appendChild(inner);
    child.doc.body.appendChild(host);

    const r = child.probe({ selector: '#pt-inner' });
    expect(r.result.is_content_editable).toBe(true);
  });
});

describe('NevofluxChild — paste method', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('rejects missing text', () => {
    const r = child.paste({ selector: '#x' });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(9002);
  });

  it('returns element-not-found for unknown selector', () => {
    const r = child.paste({ selector: '#missing', text: 'hi' });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(1001);
  });
});

describe('NevofluxChild — fillRichText method', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('rejects missing text', () => {
    const r = child.fillRichText({ selector: '#x' });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(9002);
  });

  it('returns element-not-found for unknown selector', () => {
    const r = child.fillRichText({ selector: '#missing', text: 'hi' });
    expect(r.success).toBe(false);
    expect(r.error.code).toBe(1001);
  });
});

describe('NevofluxChild — type() undefined-prefix regression', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('does not produce "undefined" prefix when typing into an input', () => {
    const doc = child.doc;
    const input = doc.createElement('input');
    input.id = 'tgt';
    input.value = '';  // explicitly empty
    doc.body.appendChild(input);
    doc._registerElementById?.('tgt', input);

    child.type({ selector: '#tgt', text: 'ABC' });
    expect(input.value).toBe('ABC');
    expect(input.value.startsWith('undefined')).toBe(false);
  });

  it('handles value that was externally set to undefined', () => {
    const doc = child.doc;
    const input = doc.createElement('input');
    input.id = 'tgt2';
    input.value = undefined;  // simulate the old bug condition
    doc.body.appendChild(input);
    doc._registerElementById?.('tgt2', input);

    child.type({ selector: '#tgt2', text: 'X' });
    expect(input.value).toBe('X');
  });
});

// ===========================================================================
//  Shadow-piercing query helpers (_deepQuerySelector / _deepQuerySelectorAll)
//
//  Regression coverage for the LinkedIn post-composer failure: the editor is a
//  contenteditable/.ql-editor inside an OPEN shadowRoot, so a flat
//  doc.querySelector() returns null and probe/click/input all reported
//  "Element not found". These tests drive the traversal against a minimal fake
//  DOM that implements only the three primitives the algorithm depends on:
//  querySelector(sel), querySelectorAll('*'), and el.shadowRoot. The fake root
//  models real scoping: querySelectorAll('*') enumerates LIGHT descendants only
//  and never crosses into a nested shadow root (each shadow root is its own
//  query scope).
// ===========================================================================

const BAD_SELECTOR = '###throws';

function fakeNode(selectors, { shadowRoot = null, children = [] } = {}) {
  return { _sel: selectors, _children: children, shadowRoot };
}

function fakeRoot(children) {
  const lightDescendants = (nodes, acc) => {
    for (const n of nodes) {
      acc.push(n);
      // Light DOM only — do NOT descend into n.shadowRoot here.
      if (n._children?.length) {
        lightDescendants(n._children, acc);
      }
    }
    return acc;
  };
  return {
    querySelector(sel) {
      if (sel === BAD_SELECTOR) {
        throw new SyntaxError('invalid selector');
      }
      return lightDescendants(children, []).find((n) => n._sel.includes(sel)) || null;
    },
    querySelectorAll(sel) {
      if (sel === BAD_SELECTOR) {
        throw new SyntaxError('invalid selector');
      }
      const all = lightDescendants(children, []);
      return sel === '*' ? all : all.filter((n) => n._sel.includes(sel));
    },
  };
}

describe('NevofluxChild - Shadow-piercing resolution', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('_deepQuerySelector finds a light-DOM element via the fast path', () => {
    const editor = fakeNode(['.ql-editor', '[contenteditable]']);
    const doc = fakeRoot([editor]);
    expect(child._deepQuerySelector('.ql-editor', doc)).toBe(editor);
  });

  it('_deepQuerySelector pierces an open shadow root (the LinkedIn case)', () => {
    // .ql-editor lives ONLY inside the host's shadowRoot — flat query misses it.
    const editor = fakeNode(['.ql-editor', "[contenteditable='true']"]);
    const host = fakeNode(['div.editor-host'], { shadowRoot: fakeRoot([editor]) });
    const doc = fakeRoot([host]);

    expect(doc.querySelector('.ql-editor')).toBe(null); // flat is blind
    expect(child._deepQuerySelector('.ql-editor', doc)).toBe(editor); // deep sees it
    expect(child._deepQuerySelector("[contenteditable='true']", doc)).toBe(editor);
  });

  it('_deepQuerySelector descends NESTED shadow roots', () => {
    const editor = fakeNode(['.ql-editor']);
    const innerHost = fakeNode(['div.inner'], { shadowRoot: fakeRoot([editor]) });
    const outerHost = fakeNode(['div.outer'], { shadowRoot: fakeRoot([innerHost]) });
    const doc = fakeRoot([outerHost]);
    expect(child._deepQuerySelector('.ql-editor', doc)).toBe(editor);
  });

  it('_deepQuerySelector returns null when nothing matches anywhere', () => {
    const host = fakeNode(['div.host'], { shadowRoot: fakeRoot([fakeNode(['.something-else'])]) });
    const doc = fakeRoot([host]);
    expect(child._deepQuerySelector('.ql-editor', doc)).toBe(null);
  });

  it('_deepQuerySelector swallows invalid selectors and returns null', () => {
    const doc = fakeRoot([fakeNode(['.x'])]);
    expect(child._deepQuerySelector(BAD_SELECTOR, doc)).toBe(null);
  });

  it('_deepQuerySelector returns null for missing doc / empty selector', () => {
    expect(child._deepQuerySelector('.ql-editor', null)).toBe(null);
    expect(child._deepQuerySelector('', fakeRoot([]))).toBe(null);
  });

  it('_deepQuerySelectorAll collects matches across light DOM and shadow roots', () => {
    const lightHit = fakeNode(['.item']);
    const shadowHit1 = fakeNode(['.item']);
    const shadowHit2 = fakeNode(['.item']);
    const host = fakeNode(['div.host'], { shadowRoot: fakeRoot([shadowHit1, shadowHit2]) });
    const doc = fakeRoot([lightHit, host]);

    const all = child._deepQuerySelectorAll('.item', doc);
    expect(all.length).toBe(3);
    expect(all.includes(lightHit)).toBe(true);
    expect(all.includes(shadowHit1)).toBe(true);
    expect(all.includes(shadowHit2)).toBe(true);
  });

  it('_deepQuerySelectorAll returns [] for invalid selector or missing doc', () => {
    expect(child._deepQuerySelectorAll(BAD_SELECTOR, fakeRoot([]))).toEqual([]);
    expect(child._deepQuerySelectorAll('.item', null)).toEqual([]);
  });
});

// ===========================================================================
//  Shadow-aware active element (_deepActiveElement)
//
//  Regression coverage for the `type`/`paste` "1002: Could not focus target"
//  false negative on LinkedIn. document.activeElement is RETARGETED to the
//  shadow host when a node inside an open shadow root is focused, so
//  `doc.activeElement === target` is false even though focus() succeeded.
//  _deepActiveElement descends shadowRoot.activeElement to the true focused
//  node so the focus check passes.
// ===========================================================================
describe('NevofluxChild - Shadow-aware active element', () => {
  let child;
  beforeEach(() => {
    child = new MockNevofluxChild();
  });

  it('returns doc.activeElement for a focused light-DOM element', () => {
    const editor = { tag: 'INPUT' };
    const doc = { activeElement: editor };
    expect(child._deepActiveElement(doc)).toBe(editor);
  });

  it('descends into a shadow root (the LinkedIn case)', () => {
    // The focused node lives in a host's shadowRoot; doc.activeElement is the
    // RETARGETED host, not the editor. _deepActiveElement must return the editor.
    const editor = { tag: 'DIV' };
    const host = { tag: 'DIV', shadowRoot: { activeElement: editor } };
    const doc = { activeElement: host };
    expect(doc.activeElement).toBe(host); // retargeted — host, not editor
    expect(child._deepActiveElement(doc)).toBe(editor); // descends to editor
  });

  it('descends through NESTED shadow roots', () => {
    const editor = { tag: 'DIV' };
    const innerHost = { tag: 'DIV', shadowRoot: { activeElement: editor } };
    const outerHost = { tag: 'DIV', shadowRoot: { activeElement: innerHost } };
    const doc = { activeElement: outerHost };
    expect(child._deepActiveElement(doc)).toBe(editor);
  });

  it('stops at a host whose shadowRoot has no activeElement', () => {
    const host = { tag: 'DIV', shadowRoot: { activeElement: null } };
    const doc = { activeElement: host };
    expect(child._deepActiveElement(doc)).toBe(host);
  });

  it('returns null when nothing is focused or doc is missing', () => {
    expect(child._deepActiveElement({ activeElement: null })).toBe(null);
    expect(child._deepActiveElement(null)).toBe(null);
  });
});

export { MockNevofluxChild };
