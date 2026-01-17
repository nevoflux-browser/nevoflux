/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

export class NevofluxChild extends JSWindowActorChild {
  // Consistent document access - fallback chain for different Firefox contexts
  get doc() {
    return this.document || this.contentWindow?.document;
  }

  get win() {
    return this.contentWindow || globalThis;
  }

  receiveMessage({ name, data }) {
    if (name === "execute") {
      return this.execute(data.action, data.params);
    }
    return null;
  }

  async execute(action, params) {
    // Ensure params is always an object (never null/undefined)
    const safeParams = params || {};
    const handlers = {
      getText: () => this.getText(safeParams),
      getHtml: () => this.getHtml(safeParams),
      getValue: () => this.getValue(safeParams),
      snapshot: () => this.snapshot(safeParams),
      screenshot: () => this.screenshot(safeParams),
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
    };

    const handler = handlers[action];
    if (!handler) {
      return { success: false, error: { code: 5002, message: `Unknown action: ${action}`, recoverable: false } };
    }

    try {
      return await handler();
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }
  }

  // ========== Data Extraction ==========

  getText({ selector }) {
    const el = this.document.querySelector(selector);
    return el?.textContent || "";
  }

  getHtml({ selector }) {
    const el = this.document.querySelector(selector);
    return el?.innerHTML || "";
  }

  getValue({ selector }) {
    const el = this.document.querySelector(selector);
    return el?.value || "";
  }

  snapshot({ interactive = false, compact = false, depth, root }) {
    // Ensure root has a valid default
    const rootSelector = root || "body";

    if (!this.doc) {
      return { tree: "", refs: {}, error: "No document available" };
    }

    const rootEl = this.doc.querySelector(rootSelector);
    if (!rootEl) {
      return { tree: "", refs: {}, error: `Root element '${rootSelector}' not found` };
    }

    const refs = {};
    let refCounter = 1;
    const self = this;

    const buildTree = (el, currentDepth = 0) => {
      if (!el || el.nodeType !== 1) {
        return "";
      }

      if (depth !== undefined && currentDepth > depth) {
        return "";
      }

      const role = self.inferRole(el);
      const name = self.getAccessibleName(el);

      // Filter: only interactive elements if interactive=true
      if (interactive && !self.isInteractive(el)) {
        return Array.from(el.children || [])
          .map(c => buildTree(c, currentDepth))
          .filter(Boolean)
          .join("");
      }

      // Filter: skip empty elements if compact=true
      if (compact && !self.hasContent(el) && !self.isInteractive(el)) {
        return Array.from(el.children || [])
          .map(c => buildTree(c, currentDepth))
          .filter(Boolean)
          .join("");
      }

      const refId = `e${refCounter++}`;
      refs[refId] = {
        role,
        name: name || "",
        selector: self.generateSelector(el),
        tagName: el.tagName.toLowerCase(),
      };

      const indent = "  ".repeat(currentDepth);
      const children = Array.from(el.children || [])
        .map(c => buildTree(c, currentDepth + 1))
        .filter(Boolean)
        .join("");

      const nameStr = name ? ` "${name}"` : "";
      return `${indent}- ${role}${nameStr} [ref=${refId}]\n${children}`;
    };

    return {
      tree: buildTree(rootEl),
      refs,
    };
  }

  async screenshot({ fullPage = false, type = "png", quality = 80 }) {
    const win = this.contentWindow;
    const doc = this.document;

    try {
      // Calculate dimensions
      const width = fullPage ? doc.documentElement.scrollWidth : win.innerWidth;
      const height = fullPage ? doc.documentElement.scrollHeight : win.innerHeight;

      // Create canvas
      const canvas = doc.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d");

      // Use drawWindow (privileged Firefox API available in JSWindowActors)
      // This captures the actual rendered content
      if (fullPage) {
        // For full page, capture entire document
        ctx.drawWindow(win, 0, 0, width, height, "rgb(255,255,255)");
      } else {
        // For viewport only
        const scrollX = win.scrollX;
        const scrollY = win.scrollY;
        ctx.drawWindow(win, scrollX, scrollY, width, height, "rgb(255,255,255)");
      }

      // Convert to data URL
      const mimeType = type === "jpeg" ? "image/jpeg" : "image/png";
      const qualityArg = type === "jpeg" ? quality / 100 : undefined;
      const dataUrl = canvas.toDataURL(mimeType, qualityArg);

      // Extract base64 data (remove "data:image/png;base64," prefix)
      const base64Data = dataUrl.split(",")[1] || "";

      return {
        success: true,
        data: base64Data,
        mimeType,
        width,
        height,
      };
    } catch (e) {
      return {
        success: false,
        error: { code: 5003, message: `Screenshot failed: ${e.message}`, recoverable: false },
      };
    }
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
      style.visibility !== "hidden" &&
      style.display !== "none" &&
      style.opacity !== "0"
    );
  }

  exists({ selector }) {
    return this.doc?.querySelector(selector) !== null;
  }

  // ========== Interaction ==========

  async click({ selector, button = "left", clickCount = 1, delay = 0, force = false }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true, suggestion: "Use waitForSelector first" } };
    }

    try {
      if (!force && !this.isVisible({ selector })) {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        await this.sleep(300);

        if (!this.isVisible({ selector })) {
          return { success: false, error: { code: 1002, message: "Element not visible", recoverable: true, suggestion: "Use force: true to click anyway" } };
        }
      }

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
        el.dispatchEvent(new MouseEvent("mouseover", eventInit));
        await this.sleep(10);
        el.dispatchEvent(new MouseEvent("mousedown", eventInit));
        await this.sleep(50);
        el.dispatchEvent(new MouseEvent("mouseup", eventInit));
        el.dispatchEvent(new MouseEvent("click", eventInit));

        if (delay > 0 && i < clickCount - 1) {
          await this.sleep(delay);
        }
      }
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    return { success: true };
  }

  type({ selector, text }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc || !win) {
      return { success: false, error: { code: 5001, message: "No document/window available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true } };
    }

    try {
      // Focus the element first
      el.focus();

      // Try to use windowUtils for real keyboard simulation
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendKeyEvent === "function") {
        // Use Firefox's privileged API for real keyboard events
        for (const char of text) {
          const charCode = char.charCodeAt(0);
          // sendKeyEvent(type, keyCode, charCode, modifiers, aAdditionalFlags)
          // keyCode=0 means use charCode, modifiers=0 means no modifiers
          domUtils.sendKeyEvent("keydown", 0, charCode, 0);
          domUtils.sendKeyEvent("keypress", 0, charCode, 0);
          domUtils.sendKeyEvent("keyup", 0, charCode, 0);
        }
      } else {
        // Fallback: direct value manipulation
        for (const char of text) {
          el.value += char;
          el.dispatchEvent(new Event("input", { bubbles: true }));
        }
      }
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }

    return { success: true };
  }

  fill({ selector, text }) {
    const doc = this.doc;
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true } };
    }

    try {
      el.focus();
      el.value = "";
      el.value = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    return { success: true };
  }

  // ========== Wait ==========

  async waitForSelector({ selector, timeout = 30000, state = "visible" }) {
    if (!this.doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
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

    return { success: false, error: { code: 4001, message: `Timeout waiting for ${selector}`, recoverable: true } };
  }

  // ========== Keyboard Control ==========

  keyPress({ key, modifiers = [], delay = 0 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils || typeof domUtils.sendKeyEvent !== "function") {
        return { success: false, error: { code: 5001, message: "windowUtils not available", recoverable: false } };
      }

      let modifierFlags = 0;
      if (modifiers.includes("ctrl")) modifierFlags |= 0x02;
      if (modifiers.includes("alt")) modifierFlags |= 0x01;
      if (modifiers.includes("shift")) modifierFlags |= 0x04;
      if (modifiers.includes("meta")) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent("keydown", keyCode, charCode, modifierFlags);
      domUtils.sendKeyEvent("keypress", keyCode, charCode, modifierFlags);
      domUtils.sendKeyEvent("keyup", keyCode, charCode, modifierFlags);

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  keyDown({ key, modifiers = [] }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils) {
        return { success: false, error: { code: 5001, message: "windowUtils not available", recoverable: false } };
      }

      let modifierFlags = 0;
      if (modifiers.includes("ctrl")) modifierFlags |= 0x02;
      if (modifiers.includes("alt")) modifierFlags |= 0x01;
      if (modifiers.includes("shift")) modifierFlags |= 0x04;
      if (modifiers.includes("meta")) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent("keydown", keyCode, charCode, modifierFlags);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  keyUp({ key }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = win.windowUtils;
      if (!domUtils) {
        return { success: false, error: { code: 5001, message: "windowUtils not available", recoverable: false } };
      }

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent("keyup", keyCode, charCode, 0);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  _getKeyCode(key) {
    const keyCodeMap = {
      "Enter": 13, "Tab": 9, "Escape": 27, "Backspace": 8, "Delete": 46,
      "ArrowUp": 38, "ArrowDown": 40, "ArrowLeft": 37, "ArrowRight": 39,
      "Home": 36, "End": 35, "PageUp": 33, "PageDown": 34,
      "F1": 112, "F2": 113, "F3": 114, "F4": 115, "F5": 116, "F6": 117,
      "F7": 118, "F8": 119, "F9": 120, "F10": 121, "F11": 122, "F12": 123,
      "Space": 32, " ": 32,
    };
    return keyCodeMap[key] || (key.length === 1 ? key.toUpperCase().charCodeAt(0) : 0);
  }

  // ========== Mouse Control ==========

  mouseMove({ x, y, steps = 1 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        domUtils.sendMouseEvent("mousemove", x, y, 0, 0, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseDown({ button = "left" }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        const x = win.innerWidth / 2;
        const y = win.innerHeight / 2;
        domUtils.sendMouseEvent("mousedown", x, y, buttonCode, 1, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseUp({ button = "left" }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        const x = win.innerWidth / 2;
        const y = win.innerHeight / 2;
        domUtils.sendMouseEvent("mouseup", x, y, buttonCode, 1, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  wheel({ deltaX = 0, deltaY = 0 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = win.windowUtils;
      if (domUtils && typeof domUtils.sendWheelEvent === "function") {
        const x = win.innerWidth / 2;
        const y = win.innerHeight / 2;
        domUtils.sendWheelEvent(x, y, deltaX, deltaY, 0, 0, 0, 0, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  async dblclick({ selector, button = "left", delay = 0 }) {
    return this.click({ selector, button, clickCount: 2, delay });
  }

  async drag({ fromSelector, toSelector, steps = 10 }) {
    const doc = this.doc;
    const win = this.document?.defaultView || this.contentWindow;
    if (!doc || !win) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const fromEl = doc.querySelector(fromSelector);
    const toEl = doc.querySelector(toSelector);

    if (!fromEl) {
      return { success: false, error: { code: 1001, message: "Source element not found", recoverable: true } };
    }
    if (!toEl) {
      return { success: false, error: { code: 1001, message: "Target element not found", recoverable: true } };
    }

    try {
      const fromRect = fromEl.getBoundingClientRect();
      const toRect = toEl.getBoundingClientRect();

      const fromX = fromRect.left + fromRect.width / 2;
      const fromY = fromRect.top + fromRect.height / 2;
      const toX = toRect.left + toRect.width / 2;
      const toY = toRect.top + toRect.height / 2;

      fromEl.dispatchEvent(new MouseEvent("mousedown", {
        bubbles: true, cancelable: true, view: win,
        clientX: fromX, clientY: fromY, button: 0
      }));

      for (let i = 1; i <= steps; i++) {
        const progress = i / steps;
        const x = fromX + (toX - fromX) * progress;
        const y = fromY + (toY - fromY) * progress;

        fromEl.dispatchEvent(new MouseEvent("mousemove", {
          bubbles: true, cancelable: true, view: win,
          clientX: x, clientY: y, button: 0
        }));

        await this.sleep(10);
      }

      toEl.dispatchEvent(new MouseEvent("mouseup", {
        bubbles: true, cancelable: true, view: win,
        clientX: toX, clientY: toY, button: 0
      }));

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  focus({ selector }) {
    const doc = this.doc;
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true } };
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
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true } };
    }

    try {
      el.focus();
      el.value = "";
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== Helpers ==========

  sleep(ms) {
    // In JSWindowActorChild, use content window's setTimeout via document.defaultView
    // or fall back to a busy-wait as last resort
    return new Promise(resolve => {
      const win = this.document?.defaultView || this.contentWindow;
      if (win && typeof win.setTimeout === "function") {
        win.setTimeout(resolve, ms);
      } else {
        // Fallback: use performance.now() based polling (less ideal but works)
        const start = Date.now();
        const check = () => {
          if (Date.now() - start >= ms) {
            resolve();
          } else {
            Promise.resolve().then(check);
          }
        };
        check();
      }
    });
  }

  inferRole(el) {
    const roleMap = {
      A: "link",
      BUTTON: "button",
      INPUT: "textbox",
      SELECT: "combobox",
      TEXTAREA: "textbox",
      IMG: "image",
      H1: "heading",
      H2: "heading",
      H3: "heading",
      H4: "heading",
      H5: "heading",
      H6: "heading",
      NAV: "navigation",
      MAIN: "main",
      ASIDE: "complementary",
      FOOTER: "contentinfo",
      HEADER: "banner",
      FORM: "form",
      TABLE: "table",
      UL: "list",
      OL: "list",
      LI: "listitem",
    };
    return el.getAttribute("role") || roleMap[el.tagName] || "generic";
  }

  getAccessibleName(el) {
    return (
      el.getAttribute("aria-label") ||
      el.getAttribute("alt") ||
      el.getAttribute("title") ||
      (el.tagName === "INPUT" ? el.getAttribute("placeholder") : null) ||
      (el.textContent?.trim().slice(0, 50) || null)
    );
  }

  isInteractive(el) {
    const interactiveTags = ["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA"];
    const hasClickHandler = el.onclick !== null;
    const hasRole = ["button", "link", "textbox", "checkbox", "radio", "combobox"].includes(
      el.getAttribute("role")
    );
    const isTabFocusable = el.getAttribute("tabindex") !== null;

    return interactiveTags.includes(el.tagName) || hasClickHandler || hasRole || isTabFocusable;
  }

  hasContent(el) {
    return el.textContent?.trim().length > 0 || el.querySelector("img, video, canvas, svg");
  }

  generateSelector(el) {
    if (el.id) {
      return `#${CSS.escape(el.id)}`;
    }

    const path = [];
    let current = el;

    while (current && current !== this.document.body) {
      let selector = current.tagName.toLowerCase();

      if (current.className && typeof current.className === "string") {
        const classes = current.className.trim().split(/\s+/).slice(0, 2);
        if (classes.length > 0 && classes[0]) {
          selector += `.${classes.map(c => CSS.escape(c)).join(".")}`;
        }
      }

      const siblings = current.parentElement?.children || [];
      const sameTagSiblings = Array.from(siblings).filter(s => s.tagName === current.tagName);
      if (sameTagSiblings.length > 1) {
        const index = sameTagSiblings.indexOf(current) + 1;
        selector += `:nth-of-type(${index})`;
      }

      path.unshift(selector);
      current = current.parentElement;
    }

    return path.join(" > ");
  }
}
