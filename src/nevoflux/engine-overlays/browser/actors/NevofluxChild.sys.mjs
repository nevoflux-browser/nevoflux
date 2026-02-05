/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Import Turndown for HTML to Markdown conversion (with GFM tables support)
import { TurndownService, gfm } from "resource:///actors/Turndown.sys.mjs";

// Lazy getter for accessibility service
const lazy = {};
ChromeUtils.defineLazyGetter(lazy, "a11yService", () => {
  try {
    return Cc["@mozilla.org/accessibilityService;1"].getService(
      Ci.nsIAccessibilityService
    );
  } catch (e) {
    return null;
  }
});

// InspectorUtils is globally available in privileged Firefox contexts
// Used to detect event listeners added via addEventListener

// Interactive roles for filtering
const INTERACTIVE_ROLES = new Set([
  "pushbutton", "button", "link", "entry", "password text",
  "text", "text container", "editable text", "searchbox",
  "checkbox", "radio button", "check menu item", "radio menu item",
  "toggle button", "combobox", "listbox", "option", "combobox option",
  "slider", "spinbutton", "menuitem", "menubar", "menu",
  "tab", "pagetab", "tablist", "tree item", "switch",
  // Also include these for form elements
  "autocomplete", "editbar", "password text", "dropdown list",
]);

// Role mapping from nsIAccessible role constants to readable strings
const ROLE_MAP = {
  1: "pushbutton",
  3: "check menu item",
  4: "dropdown list",
  5: "menu bar",
  6: "scroll bar",
  7: "grip",
  8: "sound",
  9: "cursor",
  10: "caret",
  11: "alert",
  12: "window",
  13: "internal frame",
  14: "menupopup",
  15: "menuitem",
  16: "tooltip",
  17: "application",
  18: "document",
  19: "pane",
  20: "chart",
  21: "dialog",
  22: "border",
  23: "grouping",
  24: "separator",
  25: "toolbar",
  26: "statusbar",
  27: "table",
  28: "columnheader",
  29: "rowheader",
  30: "column",
  31: "row",
  32: "cell",
  33: "link",
  34: "helpballoon",
  35: "character",
  36: "list",
  37: "listitem",
  38: "outline",
  39: "outlineitem",
  40: "pagetab",
  41: "propertypage",
  42: "indicator",
  43: "graphic",
  44: "statictext",
  45: "text leaf",
  46: "pushbutton",
  47: "checkbutton",
  48: "radiobutton",
  49: "combobox",
  50: "droplist",
  51: "progressbar",
  52: "dial",
  53: "hotkeyfield",
  54: "slider",
  55: "spinbutton",
  56: "diagram",
  57: "animation",
  58: "equation",
  59: "buttondropdown",
  60: "buttonmenu",
  61: "buttondropdowngrid",
  62: "whitespace",
  63: "pagetablist",
  64: "clock",
  65: "splitbutton",
  66: "ipaddress",
  67: "accel label",
  68: "arrow",
  69: "canvas",
  70: "check menu item",
  71: "color chooser",
  72: "date editor",
  73: "desktop icon",
  74: "desktop frame",
  75: "directory pane",
  76: "file chooser",
  77: "font chooser",
  78: "chrome window",
  79: "glass pane",
  80: "html container",
  81: "icon",
  82: "label",
  83: "layered pane",
  84: "option pane",
  85: "password text",
  86: "popup menu",
  87: "radio menu item",
  88: "root pane",
  89: "scroll pane",
  90: "split pane",
  91: "table column header",
  92: "table row header",
  93: "tear off menu item",
  94: "terminal",
  95: "text container",
  96: "toggle button",
  97: "tree table",
  98: "viewport",
  99: "header",
  100: "footer",
  101: "paragraph",
  102: "ruler",
  103: "autocomplete",
  104: "editbar",
  105: "entry",
  106: "caption",
  107: "document frame",
  108: "heading",
  109: "page",
  110: "section",
  111: "redundant object",
  112: "form",
  113: "ime",
  114: "app root",
  115: "parent menuitem",
  116: "calendar",
  117: "combobox list",
  118: "combobox option",
  119: "image map",
  120: "option",
  121: "listbox",
  122: "flat equation",
  123: "gridcell",
  124: "embedded object",
  125: "note",
  126: "figure",
  127: "check rich option",
  128: "rich option",
  129: "definition list",
  130: "term",
  131: "definition",
  132: "key",
  133: "switch",
  134: "mathml math",
  135: "mathml identifier",
  136: "mathml number",
  137: "mathml operator",
  138: "mathml text",
  139: "mathml string literal",
  140: "mathml glyph",
  141: "mathml row",
  142: "mathml fraction",
  143: "mathml sqrt",
  144: "mathml root",
  145: "mathml fenced",
  146: "mathml enclosed",
  147: "mathml style",
  148: "mathml sub",
  149: "mathml sup",
  150: "mathml subsup",
  151: "mathml under",
  152: "mathml over",
  153: "mathml underover",
  154: "mathml multiscripts",
  155: "mathml table",
  156: "mathml labeled row",
  157: "mathml table row",
  158: "mathml cell",
  159: "mathml action",
  160: "mathml error",
  161: "mathml stack",
  162: "mathml long division",
  163: "mathml stack group",
  164: "mathml stack row",
  165: "mathml stack carries",
  166: "mathml stack carry",
  167: "mathml stack line",
  168: "details",
  169: "summary",
  170: "meter",
  171: "navigation",
  172: "complementary",
  173: "contentinfo",
  174: "main",
  175: "search",
  176: "banner",
  177: "region",
  178: "article",
  179: "landmark",
  180: "blockquote",
  181: "mark",
  182: "suggestion",
  183: "comment",
};

export class NevofluxChild extends JSWindowActorChild {
  // Frame context: null = main document, string = iframe selector
  _currentFrameSelector = null;

  // Consistent document access - fallback chain for different Firefox contexts
  get doc() {
    return this.document || this.contentWindow?.document;
  }

  get win() {
    return this.contentWindow || globalThis;
  }

  // Get current document context (respects frame switching)
  get currentDoc() {
    if (!this._currentFrameSelector) {
      return this.doc;
    }
    const iframe = this.doc?.querySelector(this._currentFrameSelector);
    return iframe?.contentDocument || this.doc;
  }

  get currentWin() {
    if (!this._currentFrameSelector) {
      return this.win;
    }
    const iframe = this.doc?.querySelector(this._currentFrameSelector);
    return iframe?.contentWindow || this.win;
  }

  receiveMessage({ name, data }) {
    if (name === "execute") {
      return this.execute(data.action, data.params);
    }
    if (name === "startPicker") {
      return this.startPicker(data);
    }
    if (name === "stopPicker") {
      return this.stopPicker();
    }
    if (name === "getSelection") {
      return this.getCurrentSelection();
    }
    if (name === "lockPage") {
      return this.lockPage(data);
    }
    if (name === "unlockPage") {
      return this.unlockPage();
    }
    return null;
  }

  async execute(action, params) {
    console.log("[NevofluxChild.execute] action:", action, "params:", JSON.stringify(params));
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
      listFrames: () => this.listFrames(safeParams),
      switchFrame: () => this.switchFrame(safeParams),
      frameMain: () => this.frameMain(safeParams),
      getMarkdown: () => this.getMarkdown(safeParams),
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
    const el = this.currentDoc?.querySelector(selector);
    return el?.textContent || "";
  }

  getHtml({ selector }) {
    const el = this.currentDoc?.querySelector(selector);
    return el?.innerHTML || "";
  }

  getValue({ selector }) {
    const el = this.currentDoc?.querySelector(selector);
    return el?.value || "";
  }

  snapshot({ interactive = true, compact = false, depth, root, useA11y = true, domFallback = true, include_hidden = false }) {
    // Ensure root has a valid default
    const rootSelector = root || "body";
    const doc = this.currentDoc;
    const win = this.currentWin;

    console.log("[NevofluxChild.snapshot] Starting snapshot, params:", { interactive, compact, depth, root, rootSelector, useA11y, domFallback, include_hidden });

    if (!doc) {
      return { tree: "", refs: {}, error: "No document available" };
    }

    const rootEl = doc.querySelector(rootSelector);
    if (!rootEl) {
      return { tree: "", refs: {}, error: `Root element '${rootSelector}' not found` };
    }

    const refs = {};
    let refCounter = 1;
    const seenElements = new WeakSet();
    const self = this;

    // ========== A11y Tree Traversal ==========
    let a11yCount = 0;
    if (useA11y && lazy.a11yService) {
      try {
        const docAccessible = lazy.a11yService.getAccessibleFor(doc);
        if (docAccessible) {
          const traverseA11y = (accessible, currentDepth = 0) => {
            if (!accessible) return "";
            if (depth != null && currentDepth > depth) return "";

            const role = self._getA11yRole(accessible);
            const roleName = ROLE_MAP[role] || `unknown(${role})`;
            const name = accessible.name || "";
            const domNode = accessible.DOMNode;

            // Filter interactive elements
            const isInteractiveRole = INTERACTIVE_ROLES.has(roleName) ||
              roleName.includes("button") ||
              roleName.includes("link") ||
              roleName.includes("text") ||
              roleName.includes("entry") ||
              roleName.includes("checkbox") ||
              roleName.includes("radio") ||
              roleName.includes("combo") ||
              roleName.includes("list") ||
              roleName.includes("menu") ||
              roleName.includes("tab") ||
              roleName.includes("switch") ||
              roleName.includes("slider");

            if (interactive && !isInteractiveRole) {
              // Still traverse children
              let childContent = "";
              const childCount = accessible.childCount || 0;
              for (let i = 0; i < childCount; i++) {
                try {
                  const child = accessible.getChildAt(i);
                  childContent += traverseA11y(child, currentDepth);
                } catch (e) { /* ignore */ }
              }
              return childContent;
            }

            // Skip if no name and compact mode
            if (compact && !name && !isInteractiveRole) {
              let childContent = "";
              const childCount = accessible.childCount || 0;
              for (let i = 0; i < childCount; i++) {
                try {
                  const child = accessible.getChildAt(i);
                  childContent += traverseA11y(child, currentDepth);
                } catch (e) { /* ignore */ }
              }
              return childContent;
            }

            // Get bounding box early for zero-area filtering
            let rect = null;
            try {
              if (domNode && domNode.getBoundingClientRect) {
                const r = domNode.getBoundingClientRect();
                rect = {
                  x: Math.round(r.x),
                  y: Math.round(r.y),
                  width: Math.round(r.width),
                  height: Math.round(r.height),
                };
              }
            } catch (e) { /* ignore */ }

            // Skip zero-area elements in interactive mode (not visible, can't be clicked)
            if (interactive && rect && rect.width === 0 && rect.height === 0) {
              // Still traverse children - they might be visible
              let childContent = "";
              const childCount = accessible.childCount || 0;
              for (let i = 0; i < childCount; i++) {
                try {
                  const child = accessible.getChildAt(i);
                  childContent += traverseA11y(child, currentDepth);
                } catch (e) { /* ignore */ }
              }
              return childContent;
            }

            // Skip elements outside viewport in interactive mode
            if (interactive && rect && !self._isInViewport(rect, win)) {
              return "";  // Don't traverse children either - they're off-screen too
            }

            // Mark DOM node as seen
            if (domNode && domNode.nodeType === 1) {
              seenElements.add(domNode);
            }

            const refId = `e${refCounter++}`;
            a11yCount++;

            // Get states
            let states = {};
            try {
              const stateObj = {};
              const state = accessible.state;
              // Check common states (using bitmasks)
              stateObj.disabled = !!(state & 0x80); // STATE_UNAVAILABLE
              stateObj.focused = !!(state & 0x4);   // STATE_FOCUSED
              stateObj.checked = !!(state & 0x10);  // STATE_CHECKED
              stateObj.selected = !!(state & 0x2);  // STATE_SELECTED
              stateObj.expanded = !!(state & 0x200); // STATE_EXPANDED
              states = stateObj;
            } catch (e) { /* ignore */ }

            const selector = domNode ? self.generateSelector(domNode) : null;

            // Simplified refs structure: only essential fields for agent interaction
            refs[refId] = {
              selector,
              role: roleName,
              name: (name || "").slice(0, 50),
              ...(rect && { rect }),  // Include bounding box if available
            };

            const indent = "  ".repeat(currentDepth);
            const nameStr = name ? ` "${name}"` : "";
            let output = `${indent}- ${roleName}${nameStr} [ref=${refId}]\n`;

            // Traverse children
            const childCount = accessible.childCount || 0;
            for (let i = 0; i < childCount; i++) {
              try {
                const child = accessible.getChildAt(i);
                output += traverseA11y(child, currentDepth + 1);
              } catch (e) { /* ignore */ }
            }

            return output;
          };

          // Start from document accessible or root element accessible
          const rootAccessible = lazy.a11yService.getAccessibleFor(rootEl) || docAccessible;
          var tree = traverseA11y(rootAccessible);
        }
      } catch (e) {
        console.warn("[NevofluxChild.snapshot] A11y tree traversal failed:", e.message);
      }
    }

    // ========== DOM Fallback Traversal ==========
    let domCount = 0;
    if (domFallback) {
      // Find cursor:pointer elements not in A11y tree
      const scanDomFallback = () => {
        const pointerElements = [];

        // Scan for cursor:pointer elements
        const textTags = ["span", "div", "li", "p", "label", "td", "a", "button"];
        for (const tag of textTags) {
          try {
            const els = doc.querySelectorAll(tag);
            for (const el of els) {
              if (seenElements.has(el)) continue;
              // Skip hidden elements unless include_hidden is true
              if (!include_hidden && self._isHiddenElement(el, win)) continue;

              try {
                const style = win.getComputedStyle(el);
                const hasPointer = style.cursor === "pointer";
                const hasOnclick = el.hasAttribute("onclick");
                const hasRole = el.getAttribute("role");

                if ((hasPointer || hasOnclick) && !hasRole) {
                  const text = self.getAccessibleName(el);
                  if (text) {
                    pointerElements.push({ el, text, source: hasOnclick ? "dom-onclick" : "dom-pointer" });
                  }
                }
              } catch (e) { /* ignore */ }
            }
          } catch (e) { /* ignore */ }
        }

        // Also scan for interactive elements not in A11y tree
        const interactiveSelectors = [
          "button", "a[href]", "input:not([type='hidden'])", "textarea", "select",
          "[role='button']", "[role='link']", "[role='textbox']",
          "[tabindex]:not([tabindex='-1'])",
          "[title]"  // Elements with title attribute are often interactive
        ];

        for (const selector of interactiveSelectors) {
          try {
            const els = doc.querySelectorAll(selector);
            for (const el of els) {
              if (seenElements.has(el)) continue;
              // Skip hidden elements unless include_hidden is true
              if (!include_hidden && self._isHiddenElement(el, win)) continue;

              const text = self.getAccessibleName(el);
              pointerElements.push({ el, text: text || "", source: "dom-interactive" });
            }
          } catch (e) { /* ignore */ }
        }

        return pointerElements;
      };

      const domElements = scanDomFallback();
      for (const { el, text, source } of domElements) {
        // Get bounding box for DOM fallback elements
        let rect = null;
        try {
          if (el.getBoundingClientRect) {
            const r = el.getBoundingClientRect();
            rect = {
              x: Math.round(r.x),
              y: Math.round(r.y),
              width: Math.round(r.width),
              height: Math.round(r.height),
            };
          }
        } catch (e) { /* ignore */ }

        // Skip zero-area elements in interactive mode
        if (interactive && rect && rect.width === 0 && rect.height === 0) {
          continue;
        }

        // Skip elements outside viewport in interactive mode
        if (interactive && rect && !self._isInViewport(rect, win)) {
          continue;
        }

        seenElements.add(el);

        const refId = `e${refCounter++}`;
        domCount++;

        const role = self.inferRole(el);

        // Simplified refs structure: only essential fields for agent interaction
        refs[refId] = {
          selector: self.generateSelector(el),
          role,
          name: (text || "").slice(0, 50),
          ...(rect && { rect }),  // Include bounding box if available
        };

        // Append to tree output
        if (typeof tree === "string") {
          tree += `- ${role} "${text}" [ref=${refId}] (${source})\n`;
        }
      }
    }

    // ========== Legacy DOM-only fallback (if A11y failed) ==========
    if (!tree && !useA11y) {
      const buildTree = (el, currentDepth = 0) => {
        if (!el || el.nodeType !== 1) return "";
        if (depth != null && currentDepth > depth) return "";

        const role = self.inferRole(el);
        const name = self.getAccessibleName(el);

        if (interactive && !self.isInteractive(el)) {
          return Array.from(el.children || [])
            .map(c => buildTree(c, currentDepth))
            .filter(Boolean)
            .join("");
        }

        if (compact && !self.hasContent(el) && !self.isInteractive(el)) {
          return Array.from(el.children || [])
            .map(c => buildTree(c, currentDepth))
            .filter(Boolean)
            .join("");
        }

        // Get bounding box for legacy DOM fallback
        let rect = null;
        try {
          if (el.getBoundingClientRect) {
            const r = el.getBoundingClientRect();
            rect = {
              x: Math.round(r.x),
              y: Math.round(r.y),
              width: Math.round(r.width),
              height: Math.round(r.height),
            };
          }
        } catch (e) { /* ignore */ }

        // Skip zero-area elements in interactive mode - still traverse children
        if (interactive && rect && rect.width === 0 && rect.height === 0) {
          return Array.from(el.children || [])
            .map(c => buildTree(c, currentDepth))
            .filter(Boolean)
            .join("");
        }

        // Skip elements outside viewport in interactive mode
        if (interactive && rect && !self._isInViewport(rect, win)) {
          return "";  // Off-screen, skip entirely
        }

        const refId = `e${refCounter++}`;

        // Simplified refs structure: only essential fields for agent interaction
        refs[refId] = {
          selector: self.generateSelector(el),
          role,
          name: (name || "").slice(0, 50),
          ...(rect && { rect }),  // Include bounding box if available
        };

        const indent = "  ".repeat(currentDepth);
        const children = Array.from(el.children || [])
          .map(c => buildTree(c, currentDepth + 1))
          .filter(Boolean)
          .join("");
        const nameStr = name ? ` "${name}"` : "";
        return `${indent}- ${role}${nameStr} [ref=${refId}]\n${children}`;
      };

      tree = buildTree(rootEl);
    }

    const totalCount = Object.keys(refs).length;
    console.log("[NevofluxChild.snapshot] Done. a11yCount:", a11yCount, "domCount:", domCount, "total refs:", totalCount);

    return {
      tree: tree || "",
      refs,
      stats: {
        total: totalCount,
        fromA11y: a11yCount,
        fromDom: domCount,
      },
      url: doc.location?.href || "",
      title: doc.title || "",
    };
  }

  // Helper to get A11y role number
  _getA11yRole(accessible) {
    try {
      return accessible.role;
    } catch (e) {
      return 0;
    }
  }

  // Helper to check if element is hidden
  _isHiddenElement(el, win) {
    try {
      const style = win.getComputedStyle(el);
      if (style.display === "none") return true;
      if (style.visibility === "hidden") return true;
      if (style.opacity === "0") return true;
      if (el.getAttribute("aria-hidden") === "true") return true;
      return false;
    } catch (e) {
      return false; // On error, assume visible
    }
  }

  /**
   * Check if element is visible in the current viewport.
   * Three checks: geometric (in viewport + has area), style (not hidden), obstruction (center point hit test).
   * @param {Element} el - DOM element
   * @param {Window} win - Window object
   * @returns {boolean} true if element is visible in viewport
   */
  _isViewportVisible(el, win) {
    try {
      // 1. Geometric: element must be in viewport with non-zero dimensions
      const rect = el.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return false;
      const vw = win.innerWidth;
      const vh = win.innerHeight;
      // Element must overlap with viewport
      if (rect.bottom <= 0 || rect.top >= vh || rect.right <= 0 || rect.left >= vw) return false;

      // 2. Style: not explicitly hidden
      const style = win.getComputedStyle(el);
      if (style.display === "none") return false;
      if (style.visibility === "hidden") return false;
      if (parseFloat(style.opacity) === 0) return false;

      // 3. Obstruction: center point hit test
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      // Clamp to viewport bounds
      const testX = Math.max(0, Math.min(centerX, vw - 1));
      const testY = Math.max(0, Math.min(centerY, vh - 1));
      const topEl = this.currentDoc.elementFromPoint(testX, testY);
      if (!topEl) return false;
      // Element is visible if hit test returns itself or a descendant
      if (topEl !== el && !el.contains(topEl)) return false;

      return true;
    } catch (e) {
      return false;
    }
  }

  // Check if a bounding rect overlaps with the viewport
  _isInViewport(rect, win) {
    if (!rect || !win) return true; // If no rect, assume visible
    const vw = win.innerWidth;
    const vh = win.innerHeight;
    // Element is outside viewport if entirely above, below, left, or right
    if (rect.x + rect.width <= 0) return false;  // entirely left of viewport
    if (rect.y + rect.height <= 0) return false;  // entirely above viewport
    if (rect.x >= vw) return false;                // entirely right of viewport
    if (rect.y >= vh) return false;                // entirely below viewport
    return true;
  }

  async screenshot({ fullPage = false, type = "jpeg", quality = 60, maxWidth = 1280 }) {
    const win = this.contentWindow;
    const doc = this.document;

    try {
      // Calculate source dimensions
      const srcWidth = fullPage ? doc.documentElement.scrollWidth : win.innerWidth;
      const srcHeight = fullPage ? doc.documentElement.scrollHeight : win.innerHeight;

      // First capture at full resolution
      const srcCanvas = doc.createElement("canvas");
      srcCanvas.width = srcWidth;
      srcCanvas.height = srcHeight;
      const srcCtx = srcCanvas.getContext("2d");

      // Use drawWindow (privileged Firefox API available in JSWindowActors)
      if (fullPage) {
        srcCtx.drawWindow(win, 0, 0, srcWidth, srcHeight, "rgb(255,255,255)");
      } else {
        const scrollX = win.scrollX;
        const scrollY = win.scrollY;
        srcCtx.drawWindow(win, scrollX, scrollY, srcWidth, srcHeight, "rgb(255,255,255)");
      }

      // Downscale if needed to reduce size for LLM token efficiency
      let outputCanvas = srcCanvas;
      let outputWidth = srcWidth;
      let outputHeight = srcHeight;

      if (maxWidth > 0 && srcWidth > maxWidth) {
        const scale = maxWidth / srcWidth;
        outputWidth = Math.round(srcWidth * scale);
        outputHeight = Math.round(srcHeight * scale);

        outputCanvas = doc.createElement("canvas");
        outputCanvas.width = outputWidth;
        outputCanvas.height = outputHeight;
        const outCtx = outputCanvas.getContext("2d");
        // Use smooth scaling for better quality at smaller size
        outCtx.imageSmoothingEnabled = true;
        outCtx.imageSmoothingQuality = "high";
        outCtx.drawImage(srcCanvas, 0, 0, outputWidth, outputHeight);
      }

      // Convert to data URL - default JPEG for much smaller size
      const mimeType = type === "png" ? "image/png" : "image/jpeg";
      const qualityArg = type !== "png" ? quality / 100 : undefined;
      const dataUrl = outputCanvas.toDataURL(mimeType, qualityArg);

      // Extract base64 data (remove "data:image/...;base64," prefix)
      const base64Data = dataUrl.split(",")[1] || "";

      return {
        success: true,
        data: base64Data,
        mimeType,
        width: outputWidth,
        height: outputHeight,
        originalWidth: srcWidth,
        originalHeight: srcHeight,
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
    const doc = this.currentDoc;
    const win = this.currentWin;
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
    return this.currentDoc?.querySelector(selector) !== null;
  }

  // ========== Interaction ==========

  async click({ selector, button = "left", clickCount = 1, delay = 0, force = false }) {
    const doc = this.currentDoc;
    const win = this.currentWin;
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    // 1. Get the element from selector
    let el = doc.querySelector(selector);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true, suggestion: "Use waitForSelector first" } };
    }

    console.log("[NevofluxChild.click] selector:", selector);
    console.log("[NevofluxChild.click] element:", el.tagName, el.className);

    // Set up click effect detection (DOM changes + network requests)
    let domChanged = false;
    let networkRequestMade = false;
    let observer = null;
    let perfObserver = null;

    try {
      // 2. Set up MutationObserver to detect DOM changes
      observer = new win.MutationObserver((mutations) => {
        // Filter out trivial changes (e.g., style changes from hover)
        for (const mutation of mutations) {
          if (mutation.type === "childList" && (mutation.addedNodes.length > 0 || mutation.removedNodes.length > 0)) {
            domChanged = true;
            break;
          }
          if (mutation.type === "attributes") {
            // Ignore style-only and class-only changes that might be hover effects
            const attr = mutation.attributeName;
            if (attr !== "style" && attr !== "class") {
              domChanged = true;
              break;
            }
            // For class changes, check if it's significant (not just hover state)
            if (attr === "class") {
              const oldVal = mutation.oldValue || "";
              const newVal = mutation.target.className || "";
              // Consider significant if more than just adding/removing hover/active/focus classes
              const hoverClasses = /\b(hover|active|focus|focused|pressed)\b/gi;
              const oldClean = oldVal.replace(hoverClasses, "").trim();
              const newClean = newVal.replace(hoverClasses, "").trim();
              if (oldClean !== newClean) {
                domChanged = true;
                break;
              }
            }
          }
        }
      });

      observer.observe(doc.body || doc.documentElement, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeOldValue: true,
        attributeFilter: ["class", "style", "hidden", "disabled", "aria-hidden", "aria-expanded", "data-state"]
      });

      // 3. Set up PerformanceObserver to detect network requests
      try {
        perfObserver = new win.PerformanceObserver((list) => {
          const entries = list.getEntries();
          for (const entry of entries) {
            if (entry.initiatorType === "fetch" || entry.initiatorType === "xmlhttprequest") {
              networkRequestMade = true;
              break;
            }
          }
        });
        perfObserver.observe({ entryTypes: ["resource"] });
      } catch (e) {
        // PerformanceObserver might not be available in all contexts
        console.log("[NevofluxChild.click] PerformanceObserver not available:", e.message);
      }

      // 4. Ensure element is visible (scroll into view if needed)
      if (!force && !this.isVisible({ selector })) {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        await this.sleep(300);

        if (!this.isVisible({ selector })) {
          if (observer) observer.disconnect();
          if (perfObserver) perfObserver.disconnect();
          return { success: false, error: { code: 1002, message: "Element not visible", recoverable: true, suggestion: "Use force: true to click anyway" } };
        }
      }

      // 5. Click the element directly - background.js already handles child element finding
      let targetEl = el;

      // 4. Handle pointer-events: none - only case where we need to find alternative
      try {
        const style = win.getComputedStyle(targetEl);
        if (style.pointerEvents === "none") {
          // Try to find a clickable child that doesn't have pointer-events: none
          const clickableChild = this._findClickableDescendant(targetEl, win);
          if (clickableChild) {
            console.log("[NevofluxChild.click] Bypassing pointer-events:none, using child:", clickableChild.tagName);
            targetEl = clickableChild;
          }
        }
      } catch (e) { /* ignore style access errors */ }

      // 5. Calculate click coordinates using the actual target element
      const rect = targetEl.getBoundingClientRect();
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;

      // 6. Try to find an unobstructed click point (multi-point strategy)
      let clickPoint = this._findUnobstructedPoint(targetEl, doc);
      if (!clickPoint) {
        // Fall back to center point if all points are obstructed
        clickPoint = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
        console.log("[NevofluxChild.click] All points obstructed, using center");
      }

      console.log("[NevofluxChild.click] Target:", targetEl.tagName, "coords:", clickPoint, "rect:", { width: rect.width, height: rect.height });

      // 7. Check what element is actually at the click point
      const elementAtPoint = doc.elementFromPoint(clickPoint.x, clickPoint.y);
      const isTargetAtPoint = elementAtPoint === targetEl || targetEl.contains(elementAtPoint);
      console.log("[NevofluxChild.click] elementFromPoint:", elementAtPoint?.tagName, "isTargetAtPoint:", isTargetAtPoint);

      // 8. Try windowUtils for trusted mouse events (best method)
      const domUtils = this._getWindowUtils();
      console.log("[NevofluxChild.click] windowUtils available:", !!domUtils, "sendMouseEvent:", typeof domUtils?.sendMouseEvent);

      let windowUtilsUsed = false;

      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        console.log("[NevofluxChild.click] Using windowUtils.sendMouseEvent at", clickPoint.x, clickPoint.y);
        for (let i = 0; i < clickCount; i++) {
          // Move mouse to element first
          domUtils.sendMouseEvent("mousemove", clickPoint.x, clickPoint.y, buttonCode, 0, 0);
          await this.sleep(10);
          // Mouse down + up = click (synthesized by browser)
          domUtils.sendMouseEvent("mousedown", clickPoint.x, clickPoint.y, buttonCode, 1, 0);
          await this.sleep(50);
          domUtils.sendMouseEvent("mouseup", clickPoint.x, clickPoint.y, buttonCode, 1, 0);

          if (delay > 0 && i < clickCount - 1) {
            await this.sleep(delay);
          }
        }
        windowUtilsUsed = true;
      }

      // 9. Always try direct targetEl.click() as a reliable fallback
      //    This is the most reliable method for many sites (same as F12 console)
      console.log("[NevofluxChild.click] Executing targetEl.click()");
      targetEl.scrollIntoView({ behavior: "instant", block: "center" });
      await this.sleep(50);

      if (typeof targetEl.click === "function") {
        for (let i = 0; i < clickCount; i++) {
          targetEl.click();
          if (delay > 0 && i < clickCount - 1) {
            await this.sleep(delay);
          }
        }
        console.log("[NevofluxChild.click] targetEl.click() executed");
      }

      // 10. Dispatch synthetic events to the target element
      await this.sleep(50);
      this._dispatchMouseEvents(targetEl, clickPoint.x, clickPoint.y, buttonCode, win);

      // 11. If original element differs from target (pointer-events:none case), also click the original
      if (el !== targetEl) {
        console.log("[NevofluxChild.click] Also clicking original element (pointer-events workaround)");
        if (typeof el.click === "function") {
          el.click();
        }
        const elRect = el.getBoundingClientRect();
        const elX = elRect.left + elRect.width / 2;
        const elY = elRect.top + elRect.height / 2;
        this._dispatchMouseEvents(el, elX, elY, buttonCode, win);
      }

      // 12. If target wasn't at click point, also try clicking what's actually there
      if (!isTargetAtPoint && elementAtPoint && elementAtPoint !== targetEl) {
        console.log("[NevofluxChild.click] Clicking element at point:", elementAtPoint.tagName);
        if (typeof elementAtPoint.click === "function") {
          elementAtPoint.click();
        }
        this._dispatchMouseEvents(elementAtPoint, clickPoint.x, clickPoint.y, buttonCode, win);
      }

      // 13. Wait for effects to occur (DOM changes or network requests)
      await this.sleep(150);

      // 14. Check if element was removed from DOM (e.g., close button clicked)
      const elementStillExists = doc.body?.contains(el) ?? false;
      const elementRemoved = !elementStillExists;

    } catch (e) {
      console.error("[NevofluxChild.click] Error:", e.message, e.stack);
      if (observer) observer.disconnect();
      if (perfObserver) perfObserver.disconnect();
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    // Cleanup observers
    if (observer) observer.disconnect();
    if (perfObserver) perfObserver.disconnect();

    // Determine success based on detected effects
    const clickEffective = domChanged || networkRequestMade || !doc.body?.contains(el);
    console.log("[NevofluxChild.click] Complete - domChanged:", domChanged, "networkRequest:", networkRequestMade, "elementRemoved:", !doc.body?.contains(el), "effective:", clickEffective);

    return {
      success: true,  // Click action was performed
      effective: clickEffective,  // Whether click had detectable effect
      domChanged,
      networkRequestMade,
      elementRemoved: !doc.body?.contains(el)
    };
  }

  type({ selector, text }) {
    console.log("[NevofluxChild.type] Starting type, selector:", selector, "text length:", text?.length);
    const doc = this.currentDoc;
    const win = this.currentWin;
    if (!doc || !win) {
      console.log("[NevofluxChild.type] No doc/win available");
      return { success: false, error: { code: 5001, message: "No document/window available", recoverable: false } };
    }

    const el = doc.querySelector(selector);
    console.log("[NevofluxChild.type] Element found:", !!el, "tagName:", el?.tagName);
    if (!el) {
      return { success: false, error: { code: 1001, message: "Element not found", recoverable: true } };
    }

    try {
      // Focus the element first
      el.focus();
      console.log("[NevofluxChild.type] Focused element");

      // Try to use windowUtils for real keyboard simulation
      const domUtils = win.windowUtils;
      console.log("[NevofluxChild.type] domUtils available:", !!domUtils, "sendKeyEvent:", typeof domUtils?.sendKeyEvent);

      if (domUtils && typeof domUtils.sendKeyEvent === "function") {
        // Use Firefox's privileged API for real keyboard events
        console.log("[NevofluxChild.type] Using windowUtils.sendKeyEvent");
        for (const char of text) {
          const charCode = char.charCodeAt(0);
          // sendKeyEvent(type, keyCode, charCode, modifiers, aAdditionalFlags)
          // keyCode=0 means use charCode, modifiers=0 means no modifiers
          domUtils.sendKeyEvent("keydown", 0, charCode, 0);
          domUtils.sendKeyEvent("keypress", 0, charCode, 0);
          domUtils.sendKeyEvent("keyup", 0, charCode, 0);
        }
        console.log("[NevofluxChild.type] Finished typing via sendKeyEvent");
      } else {
        // Fallback: direct value manipulation
        console.log("[NevofluxChild.type] Using fallback value manipulation");
        for (const char of text) {
          el.value += char;
          el.dispatchEvent(new Event("input", { bubbles: true }));
        }
        console.log("[NevofluxChild.type] Finished typing via value manipulation");
      }
    } catch (e) {
      console.error("[NevofluxChild.type] Error:", e.message, e.stack);
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }

    return { success: true };
  }

  fill({ selector, text }) {
    const doc = this.currentDoc;
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
    if (!this.currentDoc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const el = this.currentDoc.querySelector(selector);

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

  // Helper to get windowUtils - try multiple access paths
  _getWindowUtils() {
    // Try browsingContext first (preferred in JSWindowActorChild)
    try {
      const utils = this.browsingContext?.window?.windowUtils;
      if (utils) {
        return utils;
      }
    } catch (e) { /* ignore */ }

    // Fallback to contentWindow
    try {
      const utils = this.contentWindow?.windowUtils;
      if (utils) {
        return utils;
      }
    } catch (e) { /* ignore */ }

    // Fallback to document.defaultView
    try {
      const utils = this.document?.defaultView?.windowUtils;
      if (utils) {
        return utils;
      }
    } catch (e) { /* ignore */ }

    return null;
  }

  // Helper to dispatch mouse events with proper bubbling for event delegation
  _dispatchMouseEvents(el, x, y, buttonCode, win) {
    const eventInit = {
      bubbles: true,
      cancelable: true,
      view: win,
      clientX: x,
      clientY: y,
      button: buttonCode,
    };

    // Trigger complete mouse event sequence for event delegation support
    el.dispatchEvent(new MouseEvent("mouseenter", { ...eventInit, bubbles: false }));
    el.dispatchEvent(new MouseEvent("mouseover", eventInit));
    el.dispatchEvent(new MouseEvent("mousemove", eventInit));
    el.dispatchEvent(new MouseEvent("mousedown", eventInit));
    el.dispatchEvent(new MouseEvent("mouseup", eventInit));
    el.dispatchEvent(new MouseEvent("click", eventInit));
  }

  /**
   * Check if an element is interactive - uses same logic as isInteractive for snapshot consistency
   * @param {Element} el - The element to check
   * @param {Window} win - The window object (unused, kept for API compatibility)
   * @param {boolean} checkDescendants - Whether to also check if any descendant is interactive
   */
  _isClickable(el, win, checkDescendants = false) {
    if (!el) return false;

    // Use same logic as isInteractive() for snapshot consistency
    const interactiveTags = ["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA"];
    const hasClickHandler = el.onclick !== null;
    const role = el.getAttribute("role");
    const hasRole = ["button", "link", "textbox", "checkbox", "radio", "combobox", "menuitem", "tab"].includes(role);
    const isTabFocusable = el.getAttribute("tabindex") !== null;

    if (interactiveTags.includes(el.tagName) || hasClickHandler || hasRole || isTabFocusable) {
      return true;
    }

    // Check if any descendant is interactive
    if (checkDescendants) {
      try {
        const descendants = el.querySelectorAll("*");
        for (const desc of descendants) {
          if (this.isInteractive(desc)) {
            return true;
          }
        }
      } catch (e) { /* ignore */ }
    }

    return false;
  }

  /**
   * Check if element is explicitly an interactive element (not just cursor:pointer)
   */
  _isExplicitlyInteractive(el) {
    if (!el) return false;

    const interactiveTags = ["BUTTON", "A", "INPUT", "SELECT", "TEXTAREA"];
    if (interactiveTags.includes(el.tagName)) return true;

    const role = el.getAttribute("role");
    if (["button", "link", "menuitem", "tab", "checkbox", "radio", "switch"].includes(role)) return true;

    if (el.onclick !== null || el.hasAttribute("onclick")) return true;

    // Also check for real event listeners using InspectorUtils
    if (this._hasClickEventListener(el)) return true;

    return false;
  }

  /**
   * Get InspectorUtils - try multiple access paths
   * InspectorUtils is a privileged Firefox API for inspecting DOM elements
   */
  _getInspectorUtils() {
    // Try 1: Global InspectorUtils (available in JSWindowActorChild)
    try {
      if (typeof InspectorUtils !== "undefined") {
        return InspectorUtils;
      }
    } catch (e) { /* ignore */ }

    // Try 2: From content window's defaultView
    try {
      const win = this.contentWindow || this.document?.defaultView;
      if (win?.InspectorUtils) {
        return win.InspectorUtils;
      }
    } catch (e) { /* ignore */ }

    // Try 3: From Cu (Components.utils)
    try {
      if (typeof Cu !== "undefined" && Cu.getGlobalForObject) {
        const global = Cu.getGlobalForObject(Cu);
        if (global?.InspectorUtils) {
          return global.InspectorUtils;
        }
      }
    } catch (e) { /* ignore */ }

    return null;
  }

  /**
   * Check if an element has click/mousedown/mouseup event listeners
   * Uses Firefox's InspectorUtils API to detect addEventListener-added listeners
   */
  _hasClickEventListener(el) {
    if (!el) return false;

    try {
      const inspectorUtils = this._getInspectorUtils();
      if (inspectorUtils && typeof inspectorUtils.getEventListenerInfoFor === "function") {
        // Use wrappedJSObject to get the actual content element for InspectorUtils
        const unwrappedEl = el.wrappedJSObject || el;
        const listeners = inspectorUtils.getEventListenerInfoFor(unwrappedEl);
        if (listeners && listeners.length > 0) {
          // Check for click-related events
          const clickEvents = ["click", "mousedown", "mouseup", "pointerdown", "pointerup", "touchstart", "touchend"];
          for (const listener of listeners) {
            if (clickEvents.includes(listener.type)) {
              console.log("[_hasClickEventListener] InspectorUtils found click listener:", el.tagName, el.className);
              return true;
            }
          }
        }
      }
    } catch (e) {
      // InspectorUtils not available or access error
    }

    // Fallback: Check for React/Vue event handlers
    // Need to use wrappedJSObject to access content object from privileged context
    try {
      // Get the unwrapped content object (crosses realm boundary)
      const unwrapped = el.wrappedJSObject || el;
      const keys = Object.keys(unwrapped);

      for (const key of keys) {
        // React 16+ internal properties - check for actual click handlers
        if (key.startsWith("__reactProps$")) {
          try {
            const props = unwrapped[key];
            if (props && (props.onClick || props.onMouseDown || props.onPointerDown)) {
              console.log("[_hasClickEventListener] Found React onClick:", el.tagName, el.className);
              return true;
            }
          } catch (e) { /* ignore cross-origin access errors */ }
        }
      }

      // Vue 3.x: Check for _vei (Vue Event Invokers) - this contains actual event handlers
      // Note: __vue__, __vueParentComponent, __vnode are just Vue internals, not event indicators
      if (unwrapped._vei) {
        // _vei is an object like { onClick: handler, onMousedown: handler, ... }
        const vei = unwrapped._vei;
        if (vei.onClick || vei.onMousedown || vei.onPointerdown ||
          vei.onclick || vei.onmousedown || vei.onpointerdown) {
          console.log("[_hasClickEventListener] Found Vue _vei click handler:", el.tagName, el.className);
          return true;
        }
      }
    } catch (e) {
      // Cross-origin or other access errors
    }

    return false;
  }

  /**
   * Find a clickable descendant within an element
   * This handles cases where selector points to a container but the actual
   * click listener is on a child element.
   * Uses same interactivity logic as isInteractive() for snapshot consistency.
   */
  _findClickableDescendant(el, win) {
    if (!el || !win) return null;

    // PRIORITY 1: Check direct children for interactive elements (same logic as isInteractive)
    try {
      const directChildren = el.children;
      console.log("[_findClickableDescendant] Checking", directChildren.length, "direct children first");

      for (const child of directChildren) {
        // Use isInteractive for snapshot consistency
        if (this.isInteractive(child)) {
          console.log("[_findClickableDescendant] Found interactive direct child:", child.tagName, child.className);
          return child;
        }
      }
    } catch (e) { /* ignore */ }

    // PRIORITY 2: Use CSS selectors matching isInteractive criteria
    // These selectors match the same elements isInteractive would return true for
    const interactiveSelectors = [
      "a",                    // interactiveTags: A
      "button",               // interactiveTags: BUTTON
      "input",                // interactiveTags: INPUT
      "select",               // interactiveTags: SELECT
      "textarea",             // interactiveTags: TEXTAREA
      "[onclick]",            // hasClickHandler
      "[role='button']",      // hasRole
      "[role='link']",        // hasRole
      "[role='textbox']",     // hasRole
      "[role='checkbox']",    // hasRole
      "[role='radio']",       // hasRole
      "[role='combobox']",    // hasRole
      "[role='menuitem']",    // hasRole
      "[role='tab']",         // hasRole
      "[tabindex]",           // isTabFocusable
    ];

    for (const selector of interactiveSelectors) {
      try {
        const child = el.querySelector(selector);
        if (child && this.isInteractive(child)) {
          console.log("[_findClickableDescendant] Found interactive element via selector:", selector, child.tagName, child.className);
          return child;
        }
      } catch (e) { /* ignore selector errors */ }
    }

    // PRIORITY 3: Search all descendants for any interactive element
    try {
      const allDescendants = el.querySelectorAll("*");
      console.log("[_findClickableDescendant] Checking", allDescendants.length, "descendants for interactivity");

      for (const child of allDescendants) {
        if (this.isInteractive(child)) {
          console.log("[_findClickableDescendant] Found interactive descendant:", child.tagName, child.className);
          return child;
        }
      }
      console.log("[_findClickableDescendant] No interactive elements found in descendants");
    } catch (e) { /* ignore */ }

    return null;
  }

  /**
   * Calculate multiple potential click points for an element
   * Used when center point might be obscured
   */
  _getClickPoints(rect) {
    const margin = 5; // Safety margin from edges
    return [
      { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 },   // Center
      { x: rect.left + margin, y: rect.top + margin },                      // Top-left
      { x: rect.right - margin, y: rect.top + margin },                     // Top-right
      { x: rect.left + margin, y: rect.bottom - margin },                   // Bottom-left
      { x: rect.right - margin, y: rect.bottom - margin },                  // Bottom-right
      { x: rect.left + rect.width / 2, y: rect.top + margin },             // Top-center
      { x: rect.left + rect.width / 2, y: rect.bottom - margin },          // Bottom-center
    ];
  }

  /**
   * Find the first unobstructed point that hits the target element
   */
  _findUnobstructedPoint(el, doc) {
    const rect = el.getBoundingClientRect();
    const points = this._getClickPoints(rect);

    for (const point of points) {
      try {
        const elementAtPoint = doc.elementFromPoint(point.x, point.y);
        // Check if the point hits the element or one of its descendants
        if (elementAtPoint === el || el.contains(elementAtPoint)) {
          return point;
        }
      } catch (e) { /* ignore */ }
    }

    return null; // All points are obstructed
  }

  /**
   * Find a clickable ancestor element for event delegation scenarios
   * Checks for real event listeners using InspectorUtils
   */
  _findClickableAncestor(el, win, maxDepth = 15) {
    let current = el?.parentElement;
    let depth = 0;

    while (current && current !== current.ownerDocument?.body && depth < maxDepth) {
      // Priority 1: Check for real click event listeners (most reliable)
      if (this._hasClickEventListener(current)) {
        console.log("[_findClickableAncestor] Found ancestor with click listener:", current.tagName, current.className);
        return current;
      }

      // Priority 2: Check for various click indicators (attributes/tags)
      if (
        current.onclick ||
        current.hasAttribute("onclick") ||
        current.hasAttribute("data-click") ||
        current.hasAttribute("data-action") ||
        current.tagName === "A" ||
        current.tagName === "BUTTON" ||
        current.getAttribute("role") === "button" ||
        current.getAttribute("role") === "link" ||
        current.getAttribute("role") === "menuitem"
      ) {
        return current;
      }

      // Priority 3: Check for cursor:pointer (common for event delegation)
      try {
        const style = win.getComputedStyle(current);
        if (style.cursor === "pointer") {
          return current;
        }
      } catch (e) { /* ignore */ }

      current = current.parentElement;
      depth++;
    }

    return null;
  }

  async keyPress({ key, modifiers = [], delay = 0 }) {
    console.log("[NevofluxChild.keyPress] key:", key, "modifiers:", modifiers);
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      const eventInit = {
        key: key,
        code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
        keyCode: keyCode,
        which: keyCode,
        bubbles: true,
        cancelable: true,
        ctrlKey: modifiers.includes("ctrl"),
        altKey: modifiers.includes("alt"),
        shiftKey: modifiers.includes("shift"),
        metaKey: modifiers.includes("meta"),
      };

      // For special keys like Enter, use simpler event sequence to avoid crashes
      // during form submission/navigation
      const isSpecialKey = ["Enter", "Tab", "Escape"].includes(key);

      if (isSpecialKey) {
        // Just dispatch keydown - most handlers respond to keydown for special keys
        console.log("[NevofluxChild.keyPress] Special key, dispatching single keydown event");
        target.dispatchEvent(new win.KeyboardEvent("keydown", eventInit));
        // Small delay to let event handlers run before we return
        // Use content window's setTimeout (available in content context)
        if (win.setTimeout) {
          await new Promise(r => win.setTimeout(r, 10));
        }
      } else {
        // Full key sequence for regular keys
        target.dispatchEvent(new win.KeyboardEvent("keydown", eventInit));
        if (delay > 0) await this.sleep(delay);
        target.dispatchEvent(new win.KeyboardEvent("keypress", eventInit));
        if (delay > 0) await this.sleep(delay);
        target.dispatchEvent(new win.KeyboardEvent("keyup", eventInit));
      }

      return { success: true };
    } catch (e) {
      console.error("[NevofluxChild.keyPress] Error:", e.message);
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  keyDown({ key, modifiers = [] }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = this._getWindowUtils();
      if (!domUtils || typeof domUtils.sendKeyEvent !== "function") {
        // Fallback to DOM events
        return this._keyDownFallback(key, modifiers);
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

  keyUp({ key, modifiers = [] }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const domUtils = this._getWindowUtils();
      if (!domUtils || typeof domUtils.sendKeyEvent !== "function") {
        // Fallback to DOM events
        return this._keyUpFallback(key, modifiers);
      }

      let modifierFlags = 0;
      if (modifiers.includes("ctrl")) modifierFlags |= 0x02;
      if (modifiers.includes("alt")) modifierFlags |= 0x01;
      if (modifiers.includes("shift")) modifierFlags |= 0x04;
      if (modifiers.includes("meta")) modifierFlags |= 0x08;

      const keyCode = this._getKeyCode(key);
      const charCode = key.length === 1 ? key.charCodeAt(0) : 0;

      domUtils.sendKeyEvent("keyup", keyCode, charCode, modifierFlags);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // Fallback keyboard implementation using DOM events (when sendKeyEvent is not available)
  async _keyPressFallback(key, modifiers = [], delay = 0) {
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return { success: false, error: { code: 5001, message: "No window/document available", recoverable: false } };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      const eventInit = {
        key: key,
        code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
        keyCode: keyCode,
        which: keyCode,
        bubbles: true,
        cancelable: true,
        ctrlKey: modifiers.includes("ctrl"),
        altKey: modifiers.includes("alt"),
        shiftKey: modifiers.includes("shift"),
        metaKey: modifiers.includes("meta"),
      };

      target.dispatchEvent(new win.KeyboardEvent("keydown", eventInit));
      if (delay > 0) await this.sleep(delay);
      target.dispatchEvent(new win.KeyboardEvent("keypress", eventInit));
      if (delay > 0) await this.sleep(delay);
      target.dispatchEvent(new win.KeyboardEvent("keyup", eventInit));

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // Fallback keyDown using DOM events
  _keyDownFallback(key, modifiers = []) {
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return { success: false, error: { code: 5001, message: "No window/document available", recoverable: false } };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      target.dispatchEvent(new win.KeyboardEvent("keydown", {
        key: key,
        code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
        keyCode: keyCode,
        which: keyCode,
        bubbles: true,
        cancelable: true,
        ctrlKey: modifiers.includes("ctrl"),
        altKey: modifiers.includes("alt"),
        shiftKey: modifiers.includes("shift"),
        metaKey: modifiers.includes("meta"),
      }));

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // Fallback keyUp using DOM events
  _keyUpFallback(key, modifiers = []) {
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return { success: false, error: { code: 5001, message: "No window/document available", recoverable: false } };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      target.dispatchEvent(new win.KeyboardEvent("keyup", {
        key: key,
        code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
        keyCode: keyCode,
        which: keyCode,
        bubbles: true,
        cancelable: true,
        ctrlKey: modifiers.includes("ctrl"),
        altKey: modifiers.includes("alt"),
        shiftKey: modifiers.includes("shift"),
        metaKey: modifiers.includes("meta"),
      }));

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
      const domUtils = this._getWindowUtils();
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        domUtils.sendMouseEvent("mousemove", x, y, 0, 0, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseDown({ button = "left", x, y }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = this._getWindowUtils();
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        const posX = x !== undefined ? x : win.innerWidth / 2;
        const posY = y !== undefined ? y : win.innerHeight / 2;
        domUtils.sendMouseEvent("mousedown", posX, posY, buttonCode, 1, 0);
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  mouseUp({ button = "left", x, y }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;
      const domUtils = this._getWindowUtils();
      if (domUtils && typeof domUtils.sendMouseEvent === "function") {
        const posX = x !== undefined ? x : win.innerWidth / 2;
        const posY = y !== undefined ? y : win.innerHeight / 2;
        domUtils.sendMouseEvent("mouseup", posX, posY, buttonCode, 1, 0);
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
      const domUtils = this._getWindowUtils();
      if (domUtils && typeof domUtils.sendWheelEvent === "function") {
        const x = win.innerWidth / 2;
        const y = win.innerHeight / 2;
        // sendWheelEvent(x, y, deltaX, deltaY, deltaZ, deltaMode, modifiers, lineOrPageDeltaX, lineOrPageDeltaY, options)
        domUtils.sendWheelEvent(x, y, deltaX, deltaY, 0, 0, 0, 0, 0, 0);
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
    const doc = this.currentDoc;
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
    const doc = this.currentDoc;
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
    // 1. Check explicit accessibility attributes first
    const ariaLabel = el.getAttribute("aria-label");
    if (ariaLabel) return ariaLabel;

    // 2. Check aria-labelledby
    const labelledBy = el.getAttribute("aria-labelledby");
    if (labelledBy) {
      const labelEl = el.ownerDocument?.getElementById(labelledBy);
      if (labelEl?.textContent?.trim()) {
        return labelEl.textContent.trim().slice(0, 50);
      }
    }

    // 3. Check title attribute (both getAttribute and property for Vue/React compatibility)
    const titleAttr = el.getAttribute("title");
    if (titleAttr) return titleAttr;
    if (el.title) return el.title;

    // 4. Check alt (for images)
    const alt = el.getAttribute("alt");
    if (alt) return alt;

    // 5. Check placeholder for inputs/textareas
    const tagName = el.tagName?.toUpperCase();
    if (tagName === "INPUT" || tagName === "TEXTAREA") {
      const placeholder = el.getAttribute("placeholder") || el.placeholder;
      if (placeholder) return placeholder;
    }

    // 6. Check value for buttons and inputs
    if (tagName === "INPUT") {
      const inputType = el.type?.toLowerCase();
      if (inputType === "submit" || inputType === "button" || inputType === "reset") {
        const value = el.value;
        if (value) return value;
      }
    }

    // 7. Fallback to direct text content (excluding nested element text)
    // This gets only immediate text children, not deeply nested text
    let directText = "";
    for (const node of el.childNodes) {
      if (node.nodeType === 3) { // Text node
        directText += node.textContent || "";
      }
    }
    directText = directText.trim();
    if (directText) return directText.slice(0, 50);

    // 8. Final fallback: full text content
    const textContent = el.textContent?.trim();
    if (textContent) return textContent.slice(0, 50);

    return null;
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

  // ========== Storage ==========

  getLocalStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
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
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const serialized = typeof value === "string" ? value : JSON.stringify(value);
      win.localStorage.setItem(key, serialized);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 7003, message: String(e), recoverable: false } };
    }
  }

  removeLocalStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    if (!key) {
      return { success: false, error: { code: 7002, message: "Missing required parameter: key", recoverable: false } };
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
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
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
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
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
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      const serialized = typeof value === "string" ? value : JSON.stringify(value);
      win.sessionStorage.setItem(key, serialized);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 7003, message: String(e), recoverable: false } };
    }
  }

  removeSessionStorage({ key }) {
    const win = this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    if (!key) {
      return { success: false, error: { code: 7002, message: "Missing required parameter: key", recoverable: false } };
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
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    try {
      win.sessionStorage.clear();
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== Frame Management ==========

  listFrames() {
    const doc = this.currentDoc;
    if (!doc) {
      return [];
    }

    const iframes = doc.querySelectorAll("iframe");
    const frames = [];

    for (const iframe of iframes) {
      const rect = iframe.getBoundingClientRect();
      const style = doc.defaultView?.getComputedStyle(iframe);
      const visible = rect.width > 0 && rect.height > 0 &&
        style?.visibility !== "hidden" &&
        style?.display !== "none";

      frames.push({
        selector: this.generateSelector(iframe),
        url: iframe.src || "",
        name: iframe.name || "",
        visible
      });
    }

    return frames;
  }

  switchFrame({ selector }) {
    const doc = this.doc; // Always search from main document
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    // If already in a frame, search within that frame
    const searchDoc = this._currentFrameSelector
      ? doc.querySelector(this._currentFrameSelector)?.contentDocument || doc
      : doc;

    const iframe = searchDoc.querySelector(selector);
    if (!iframe || iframe.tagName !== "IFRAME") {
      return { success: false, error: { code: 10001, message: `Frame not found: ${selector}`, recoverable: true } };
    }

    try {
      // Test if we can access the frame's content
      const frameDoc = iframe.contentDocument;
      if (!frameDoc) {
        return { success: false, error: { code: 10002, message: "Frame access denied (cross-origin)", recoverable: false } };
      }

      // Store the absolute selector path for nested frames
      if (this._currentFrameSelector) {
        // Build compound selector for nested frame
        this._currentFrameSelector = `${this._currentFrameSelector} ${selector}`;
      } else {
        this._currentFrameSelector = selector;
      }

      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 10002, message: `Frame access denied: ${e.message}`, recoverable: false } };
    }
  }

  frameMain() {
    this._currentFrameSelector = null;
    return { success: true };
  }

  // ========== JavaScript Execution ==========

  // Note: Timeout is not implemented for eval. Implementing true timeout for synchronous
  // eval is complex and would require running in a Worker or using async patterns.
  // The script executes synchronously in the page context.
  evalScript({ script, returnValue = true }) {
    const win = this.contentWindow;
    if (!win) {
      return { success: false, error: { code: 5001, message: "No window available", recoverable: false } };
    }

    if (!script || typeof script !== "string") {
      return { success: false, error: { code: 9002, message: "Missing or invalid required parameter: script", recoverable: false } };
    }

    try {
      // Execute in page context
      const result = win.eval(script);

      if (!returnValue) {
        return { success: true };
      }

      // Serialize result
      let serialized;
      let type = typeof result;

      try {
        if (result === undefined) {
          serialized = undefined;
          type = "undefined";
        } else if (result === null) {
          serialized = null;
          type = "null";
        } else {
          serialized = JSON.parse(JSON.stringify(result));
        }
      } catch {
        // Can't serialize, return string representation
        serialized = String(result);
        type = "string";
      }

      return {
        success: true,
        value: serialized,
        type
      };
    } catch (e) {
      return {
        success: false,
        error: {
          code: 9001,
          message: e.message,
          recoverable: false
        }
      };
    }
  }

  addScript({ script, runAt = "document_idle" }) {
    const doc = this.doc;
    const win = this.contentWindow;
    if (!doc || !win) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    if (!script || typeof script !== "string") {
      return { success: false, error: { code: 9002, message: "Missing or invalid required parameter: script", recoverable: false } };
    }

    try {
      const scriptEl = doc.createElement("script");
      scriptEl.textContent = script;
      // Use timestamp + random suffix to ensure unique handles
      scriptEl.id = `nevoflux_script_${Date.now()}_${Math.random().toString(36).slice(2)}`;

      if (runAt === "document_start") {
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
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    if (!handle) {
      return { success: false, error: { code: 9002, message: "Missing required parameter: handle", recoverable: false } };
    }

    try {
      const scriptEl = doc.getElementById(handle);
      if (scriptEl) {
        scriptEl.remove();
        return { success: true };
      }
      return { success: false, error: { code: 9003, message: "Script not found", recoverable: false } };
    } catch (e) {
      return { success: false, error: { code: 9001, message: String(e), recoverable: false } };
    }
  }

  // ========== Markdown Extraction ==========

  /**
   * Get markdown content from the page using Turndown directly.
   * Uses GFM tables plugin for proper table rendering.
   * Simple and predictable - no content filtering by Readability.
   *
   * @param {Object} options
   * @param {string} options.selector - Optional CSS selector to extract from (default: body)
   * @param {boolean} options.includeImages - Whether to include images (default: false)
   * @param {boolean} options.includeLinks - Whether to preserve links (default: true)
   * @param {boolean} options.removeNavigation - Whether to remove nav/header/footer (default: true)
   */
  getMarkdown({
    selector = null,
    includeImages = false,
    includeLinks = true,
    removeNavigation = true,
  } = {}) {
    const doc = this.currentDoc;
    if (!doc) {
      return { success: false, error: { code: 5001, message: "No document available", recoverable: false } };
    }

    try {
      // Clone the document to avoid modifying the original
      const docClone = doc.cloneNode(true);

      // Remove script/style/meta tags
      const junkTags = ['script', 'noscript', 'style', 'link', 'meta', 'template', 'svg'];
      for (const tagName of junkTags) {
        const elements = docClone.querySelectorAll(tagName);
        for (const el of elements) {
          el.remove();
        }
      }

      // Optionally remove navigation elements
      if (removeNavigation) {
        const navSelectors = [
          'nav', 'header', 'footer',
          '[role="navigation"]', '[role="banner"]', '[role="contentinfo"]',
          '[aria-hidden="true"]', '[hidden]',
        ];
        for (const sel of navSelectors) {
          try {
            const elements = docClone.querySelectorAll(sel);
            for (const el of elements) {
              el.remove();
            }
          } catch (e) {
            // Invalid selector
          }
        }
      }

      // Handle lazy-loaded images
      const lazyImages = docClone.querySelectorAll('img[data-src], img[data-original], img[data-lazy-src], img[data-actualsrc]');
      for (const img of lazyImages) {
        const lazySrc = img.getAttribute('data-src') ||
                        img.getAttribute('data-original') ||
                        img.getAttribute('data-lazy-src') ||
                        img.getAttribute('data-actualsrc');
        if (lazySrc && !img.getAttribute('src')) {
          img.setAttribute('src', lazySrc);
        }
      }

      // Find content element - use selector if provided, otherwise use body
      let contentEl;
      if (selector) {
        contentEl = docClone.querySelector(selector);
        if (!contentEl) {
          return { success: false, error: { code: 1001, message: `Selector "${selector}" not found`, recoverable: true } };
        }
      } else {
        // Use body directly for complete page content
        contentEl = docClone.body;
      }

      // Create Turndown service with GFM support
      const turndownService = new TurndownService({
        headingStyle: 'atx',
        hr: '---',
        bulletListMarker: '-',
        codeBlockStyle: 'fenced',
        fence: '```',
        emDelimiter: '*',
        strongDelimiter: '**',
        linkStyle: 'inlined',
      });

      // Set document context
      turndownService.setDocument(doc);

      // Enable GFM tables
      turndownService.use(gfm);

      // Handle images
      if (!includeImages) {
        turndownService.addRule('removeImages', {
          filter: 'img',
          replacement: function() {
            return '';
          }
        });
      }

      // Handle links
      if (!includeLinks) {
        turndownService.addRule('plainLinks', {
          filter: 'a',
          replacement: function(content) {
            return content;
          }
        });
      }

      // Convert to markdown
      const markdown = turndownService.turndown(contentEl);

      // Clean up excessive whitespace
      const cleanedMarkdown = markdown
        .replace(/\n{3,}/g, '\n\n')  // Max 2 consecutive newlines
        .replace(/[ \t]+$/gm, '')    // Trim trailing whitespace per line
        .trim();

      return {
        success: true,
        markdown: cleanedMarkdown,
        title: doc.title || "",
        url: doc.location?.href || "",
      };
    } catch (e) {
      console.error("[NevofluxChild.getMarkdown] Error:", e);
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }


  /**
   * Check if an element should be skipped during conversion
   */
  _shouldSkipElement(el, filterNonContent) {
    if (!el || el.nodeType !== 1) return true;

    const tagName = el.tagName.toUpperCase();

    // Always skip these elements
    const skipTags = ["SCRIPT", "STYLE", "NOSCRIPT", "SVG", "IFRAME", "TEMPLATE"];
    if (skipTags.includes(tagName)) return true;

    if (!filterNonContent) return false;

    // Skip navigation and structural elements
    const skipStructural = ["NAV", "HEADER", "FOOTER", "ASIDE"];
    if (skipStructural.includes(tagName)) return true;

    // Skip by role
    const role = el.getAttribute("role");
    const skipRoles = ["navigation", "banner", "contentinfo", "complementary", "search"];
    if (role && skipRoles.includes(role)) return true;

    // Skip by common ad/non-content class names
    const className = el.className?.toString().toLowerCase() || "";
    const id = el.id?.toLowerCase() || "";
    const skipPatterns = [
      "nav", "menu", "sidebar", "widget", "ad", "ads", "advertisement",
      "banner", "promo", "social", "share", "comment", "related",
      "footer", "header", "breadcrumb", "pagination", "toc",
    ];

    for (const pattern of skipPatterns) {
      if (className.includes(pattern) || id.includes(pattern)) {
        // Don't skip if it's a main content area that happens to have a matching class
        if (el.textContent?.trim().length > 500) {
          continue;
        }
        return true;
      }
    }

    return false;
  }

  /**
   * Convert an element tree to Markdown
   */
  _convertToMarkdown(rootEl, options) {
    const lines = [];
    this._processElement(rootEl, options, lines, 0);
    return this._cleanMarkdown(lines.join("\n"));
  }

  /**
   * Process a single element and its children
   */
  _processElement(el, options, lines, depth) {
    if (!el) return;

    // Handle text nodes
    if (el.nodeType === 3) {
      const text = el.textContent?.trim();
      if (text) {
        lines.push(text);
      }
      return;
    }

    // Skip non-element nodes and filtered elements
    if (el.nodeType !== 1) return;
    if (this._shouldSkipElement(el, options.filterNonContent)) return;

    const tagName = el.tagName.toUpperCase();

    // Handle specific elements
    switch (tagName) {
      case "H1":
      case "H2":
      case "H3":
      case "H4":
      case "H5":
      case "H6": {
        const level = parseInt(tagName.charAt(1), 10);
        const text = el.textContent?.trim();
        if (text) {
          lines.push("");
          lines.push("#".repeat(level) + " " + text);
          lines.push("");
        }
        return;
      }

      case "P": {
        const content = this._getInlineContent(el, options);
        if (content.trim()) {
          lines.push("");
          lines.push(content);
          lines.push("");
        }
        return;
      }

      case "A": {
        if (options.includeLinks) {
          const text = el.textContent?.trim();
          const href = el.href; // DOM automatically provides absolute URL
          if (text && href) {
            lines.push(`[${text}](${href})`);
          } else if (text) {
            lines.push(text);
          }
        } else {
          lines.push(el.textContent?.trim() || "");
        }
        return;
      }

      case "IMG": {
        if (options.includeImages) {
          const src = el.src; // DOM automatically provides absolute URL
          if (src) {
            // Skip base64 images unless explicitly included
            if (src.startsWith("data:") && !options.includeBase64Images) {
              return;
            }
            const alt = el.alt || el.title || "image";
            lines.push(`![${alt}](${src})`);
          }
        }
        return;
      }

      case "UL":
      case "OL": {
        lines.push("");
        this._processList(el, options, lines, tagName === "OL", depth);
        lines.push("");
        return;
      }

      case "LI": {
        const content = this._getInlineContent(el, options);
        if (content.trim()) {
          lines.push(content);
        }
        // Process nested lists
        for (const child of el.children) {
          if (child.tagName === "UL" || child.tagName === "OL") {
            this._processList(child, options, lines, child.tagName === "OL", depth + 1);
          }
        }
        return;
      }

      case "BLOCKQUOTE": {
        lines.push("");
        const quoteLines = [];
        for (const child of el.childNodes) {
          this._processElement(child, options, quoteLines, depth);
        }
        for (const line of quoteLines) {
          if (line.trim()) {
            lines.push("> " + line);
          }
        }
        lines.push("");
        return;
      }

      case "PRE": {
        const codeEl = el.querySelector("code");
        const code = codeEl ? codeEl.textContent : el.textContent;
        const lang = codeEl?.className.match(/language-(\w+)/)?.[1] || "";
        lines.push("");
        lines.push("```" + lang);
        lines.push(code?.trim() || "");
        lines.push("```");
        lines.push("");
        return;
      }

      case "CODE": {
        // Inline code (not inside PRE)
        if (el.parentElement?.tagName !== "PRE") {
          const text = el.textContent?.trim();
          if (text) {
            lines.push("`" + text + "`");
          }
        }
        return;
      }

      case "TABLE": {
        if (options.includeTables) {
          const tableMarkdown = this._convertTable(el);
          if (tableMarkdown) {
            lines.push("");
            lines.push(tableMarkdown);
            lines.push("");
          }
        }
        return;
      }

      case "HR": {
        lines.push("");
        lines.push("---");
        lines.push("");
        return;
      }

      case "BR": {
        lines.push("  "); // Two spaces for line break
        return;
      }

      case "STRONG":
      case "B": {
        const text = el.textContent?.trim();
        if (text) {
          lines.push("**" + text + "**");
        }
        return;
      }

      case "EM":
      case "I": {
        const text = el.textContent?.trim();
        if (text) {
          lines.push("*" + text + "*");
        }
        return;
      }

      case "DIV":
      case "SECTION":
      case "ARTICLE":
      case "MAIN":
      case "SPAN": {
        // Container elements - process children
        for (const child of el.childNodes) {
          this._processElement(child, options, lines, depth);
        }
        return;
      }

      default: {
        // For other elements, process children
        for (const child of el.childNodes) {
          this._processElement(child, options, lines, depth);
        }
      }
    }
  }

  /**
   * Process list elements
   */
  _processList(listEl, options, lines, isOrdered, depth) {
    const indent = "  ".repeat(depth);
    let counter = 1;

    for (const li of listEl.children) {
      if (li.tagName !== "LI") continue;

      const prefix = isOrdered ? `${counter}. ` : "- ";
      const content = this._getInlineContent(li, options);

      if (content.trim()) {
        lines.push(indent + prefix + content);
      }

      // Handle nested lists
      for (const child of li.children) {
        if (child.tagName === "UL" || child.tagName === "OL") {
          this._processList(child, options, lines, child.tagName === "OL", depth + 1);
        }
      }

      counter++;
    }
  }

  /**
   * Get inline content from an element (text with inline formatting)
   */
  _getInlineContent(el, options) {
    const parts = [];

    for (const node of el.childNodes) {
      if (node.nodeType === 3) {
        // Text node
        const text = node.textContent;
        if (text) parts.push(text);
      } else if (node.nodeType === 1) {
        // Element node
        const tag = node.tagName.toUpperCase();

        // Skip nested block elements and non-content elements
        if (["UL", "OL", "DIV", "P", "TABLE", "BLOCKQUOTE", "PRE", "SCRIPT", "STYLE", "NOSCRIPT", "SVG", "IFRAME", "TEMPLATE", "NAV", "HEADER", "FOOTER", "ASIDE"].includes(tag)) {
          continue;
        }

        switch (tag) {
          case "A":
            if (options.includeLinks && node.href) {
              const text = node.textContent?.trim();
              if (text) {
                parts.push(`[${text}](${node.href})`);
              }
            } else {
              parts.push(node.textContent || "");
            }
            break;

          case "STRONG":
          case "B":
            parts.push("**" + (node.textContent || "") + "**");
            break;

          case "EM":
          case "I":
            parts.push("*" + (node.textContent || "") + "*");
            break;

          case "CODE":
            parts.push("`" + (node.textContent || "") + "`");
            break;

          case "IMG":
            if (options.includeImages && node.src) {
              // Skip base64 images unless explicitly included
              if (node.src.startsWith("data:") && !options.includeBase64Images) {
                break;
              }
              const alt = node.alt || node.title || "image";
              parts.push(`![${alt}](${node.src})`);
            }
            break;

          case "BR":
            parts.push("  \n");
            break;

          default:
            parts.push(node.textContent || "");
        }
      }
    }

    return parts.join("").replace(/\s+/g, " ").trim();
  }

  /**
   * Convert a table element to Markdown
   */
  _convertTable(tableEl) {
    const rows = [];
    const headerRow = [];
    let hasHeader = false;

    // Process thead
    const thead = tableEl.querySelector("thead");
    if (thead) {
      const headerCells = thead.querySelectorAll("th, td");
      for (const cell of headerCells) {
        headerRow.push(cell.textContent?.trim() || "");
      }
      hasHeader = true;
    }

    // Process tbody
    const tbody = tableEl.querySelector("tbody") || tableEl;
    const dataRows = tbody.querySelectorAll("tr");

    for (let i = 0; i < dataRows.length; i++) {
      const tr = dataRows[i];
      const cells = tr.querySelectorAll("th, td");
      const rowData = [];

      for (const cell of cells) {
        rowData.push(cell.textContent?.trim() || "");
      }

      // If first row is all TH and we don't have header yet
      if (i === 0 && !hasHeader && tr.querySelector("th")) {
        headerRow.push(...rowData);
        hasHeader = true;
        continue;
      }

      rows.push(rowData);
    }

    // Generate markdown
    if (headerRow.length === 0 && rows.length > 0) {
      // Use first row as header if no header found
      headerRow.push(...rows.shift());
    }

    if (headerRow.length === 0) {
      return "";
    }

    const colCount = headerRow.length;
    const lines = [];

    // Header row
    lines.push("| " + headerRow.join(" | ") + " |");

    // Separator
    lines.push("| " + headerRow.map(() => "---").join(" | ") + " |");

    // Data rows
    for (const row of rows) {
      // Pad row if needed
      while (row.length < colCount) row.push("");
      lines.push("| " + row.slice(0, colCount).join(" | ") + " |");
    }

    return lines.join("\n");
  }

  /**
   * Clean up the generated Markdown
   */
  _cleanMarkdown(markdown) {
    return markdown
      // Remove any remaining <style>...</style> blocks (including content)
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, "")
      // Remove any remaining <script>...</script> blocks (including content)
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, "")
      // Remove any remaining <noscript>...</noscript> blocks
      .replace(/<noscript[^>]*>[\s\S]*?<\/noscript>/gi, "")
      // Remove inline style attributes from any remaining HTML tags
      .replace(/\s+style\s*=\s*["'][^"']*["']/gi, "")
      // Remove any remaining HTML tags (but keep content)
      .replace(/<[^>]+>/g, "")
      // Remove excessive blank lines (more than 2)
      .replace(/\n{3,}/g, "\n\n")
      // Clean up whitespace
      .replace(/[ \t]+$/gm, "")
      // Trim
      .trim();
  }

  // ========== Element Picker ==========

  startPicker({ filter = "any", highlightColor = "#6366f1" }) {
    if (this._pickerActive) {
      return { success: false, error: "Picker already active" };
    }

    this._pickerActive = true;
    this._pickerFilter = filter;
    this._highlightColor = highlightColor;
    this._pickerResolve = null;
    this._pickerReject = null;

    this._createPickerHighlight();

    this.doc.addEventListener("mousemove", this._onPickerMove, true);
    this.doc.addEventListener("click", this._onPickerClick, true);
    this.doc.addEventListener("keydown", this._onPickerKey, true);

    this._originalCursor = this.doc.body.style.cursor;
    this.doc.body.style.cursor = "crosshair";

    return new Promise((resolve, reject) => {
      this._pickerResolve = resolve;
      this._pickerReject = reject;
    });
  }

  stopPicker() {
    if (!this._pickerActive) return { success: true };

    this._pickerActive = false;

    this.doc.removeEventListener("mousemove", this._onPickerMove, true);
    this.doc.removeEventListener("click", this._onPickerClick, true);
    this.doc.removeEventListener("keydown", this._onPickerKey, true);

    this.doc.body.style.cursor = this._originalCursor || "";
    this._removePickerHighlight();

    if (this._pickerReject) {
      this._pickerReject({ success: false, error: "cancelled" });
    }

    return { success: true };
  }

  _createPickerHighlight() {
    if (this._highlightEl) return;

    this._highlightEl = this.doc.createElement("div");
    this._highlightEl.id = "nevoflux-picker-highlight";
    this._highlightEl.style.cssText = `
      position: fixed;
      pointer-events: none;
      z-index: 2147483647;
      border: 2px solid ${this._highlightColor};
      background: ${this._highlightColor}20;
      border-radius: 3px;
      transition: all 0.1s ease-out;
      display: none;
    `;

    this._labelEl = this.doc.createElement("div");
    this._labelEl.style.cssText = `
      position: absolute;
      bottom: 100%;
      left: 0;
      background: ${this._highlightColor};
      color: white;
      font-size: 11px;
      font-family: system-ui, sans-serif;
      padding: 2px 6px;
      border-radius: 3px 3px 0 0;
      white-space: nowrap;
    `;
    this._highlightEl.appendChild(this._labelEl);

    this.doc.body.appendChild(this._highlightEl);
  }

  _removePickerHighlight() {
    this._highlightEl?.remove();
    this._highlightEl = null;
    this._labelEl = null;
    this._hoveredEl = null;
  }

  _onPickerMove = (event) => {
    event.stopPropagation();

    let target = event.target;
    if (target === this._highlightEl || this._highlightEl?.contains(target)) {
      return;
    }

    this._hoveredEl = target;
    this._updatePickerHighlight(target);
  };

  _onPickerClick = (event) => {
    event.stopPropagation();
    event.preventDefault();

    const target = this._hoveredEl;
    if (!target) return;

    const result = {
      selector: this._generateStableSelector(target),
      xpath: this._generateXPath(target),
      tagName: target.tagName.toLowerCase(),
      id: target.id || null,
      className: typeof target.className === "string" ? target.className : null,
      text: target.textContent?.slice(0, 200)?.trim() || null,
      attributes: this._getPickerElementAttributes(target),
      rect: target.getBoundingClientRect().toJSON(),
    };

    this.stopPicker();

    if (this._pickerResolve) {
      this._pickerResolve({ success: true, data: result });
    }
  };

  _onPickerKey = (event) => {
    event.stopPropagation();

    if (event.key === "Escape") {
      event.preventDefault();
      this.stopPicker();
    }
  };

  _updatePickerHighlight(element) {
    if (!this._highlightEl || !element) return;

    const rect = element.getBoundingClientRect();

    this._highlightEl.style.display = "block";
    this._highlightEl.style.top = `${rect.top}px`;
    this._highlightEl.style.left = `${rect.left}px`;
    this._highlightEl.style.width = `${rect.width}px`;
    this._highlightEl.style.height = `${rect.height}px`;

    const tag = element.tagName.toLowerCase();
    const id = element.id ? `#${element.id}` : "";
    const cls = element.className && typeof element.className === "string"
      ? `.${element.className.split(" ")[0]}`
      : "";
    this._labelEl.textContent = `${tag}${id}${cls}`;
  }

  _generateStableSelector(element) {
    if (!element || element === this.doc.body) return "body";

    // Priority 1: Unique ID
    if (element.id) {
      const selector = `#${CSS.escape(element.id)}`;
      if (this.doc.querySelectorAll(selector).length === 1) {
        return selector;
      }
    }

    // Priority 2: data-testid or data-* attributes
    for (const attr of element.attributes) {
      if (attr.name === "data-testid" || attr.name.startsWith("data-")) {
        const selector = `[${attr.name}="${CSS.escape(attr.value)}"]`;
        if (this.doc.querySelectorAll(selector).length === 1) {
          return selector;
        }
      }
    }

    // Priority 3: Build path
    const path = [];
    let current = element;

    while (current && current !== this.doc.body) {
      let selector = current.tagName.toLowerCase();

      if (current.id) {
        path.unshift(`#${CSS.escape(current.id)}`);
        break;
      }

      const parent = current.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter(
          el => el.tagName === current.tagName
        );
        if (siblings.length > 1) {
          const index = siblings.indexOf(current) + 1;
          selector += `:nth-of-type(${index})`;
        }
      }

      path.unshift(selector);
      current = current.parentElement;
    }

    return path.join(" > ");
  }

  _generateXPath(element) {
    if (!element) return "";

    const parts = [];
    let current = element;

    while (current && current.nodeType === Node.ELEMENT_NODE) {
      let index = 1;
      let sibling = current.previousElementSibling;

      while (sibling) {
        if (sibling.tagName === current.tagName) index++;
        sibling = sibling.previousElementSibling;
      }

      const tagName = current.tagName.toLowerCase();
      parts.unshift(`${tagName}[${index}]`);
      current = current.parentElement;
    }

    return "/" + parts.join("/");
  }

  _getPickerElementAttributes(element) {
    const attrs = {};
    for (const attr of element.attributes) {
      if (attr.value.length < 200 && !attr.name.startsWith("on")) {
        attrs[attr.name] = attr.value;
      }
    }
    return attrs;
  }

  // ========== Selection ==========

  getCurrentSelection() {
    const selection = this.win.getSelection();

    if (!selection || selection.isCollapsed) {
      return { success: true, data: null };
    }

    const text = selection.toString().trim();
    if (!text) {
      return { success: true, data: null };
    }

    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    const container = this.doc.createElement("div");
    container.appendChild(range.cloneContents());

    return {
      success: true,
      data: {
        text,
        html: container.innerHTML,
        rect: rect.toJSON(),
        anchorNode: this._generateStableSelector(selection.anchorNode.parentElement),
        url: this.win.location.href,
        title: this.doc.title,
      },
    };
  }

  // ========== Page Lock ==========

  lockPage({ showOverlay = true, message = "" }) {
    if (this._pageLocked) return { success: true };

    this._pageLocked = true;

    // Event locking
    this._lockHandler = (event) => {
      event.stopImmediatePropagation();
      event.preventDefault();
    };

    const events = [
      "mousedown", "mouseup", "click", "dblclick", "contextmenu",
      "keydown", "keyup", "keypress",
      "touchstart", "touchend", "touchmove",
      "wheel", "scroll",
    ];

    events.forEach(type => {
      this.doc.addEventListener(type, this._lockHandler, { capture: true });
    });

    this._lockEvents = events;

    // Visual overlay
    if (showOverlay) {
      this._createLockOverlay(message);
    }

    return { success: true };
  }

  unlockPage() {
    if (!this._pageLocked) return { success: true };

    this._pageLocked = false;

    if (this._lockHandler && this._lockEvents) {
      this._lockEvents.forEach(type => {
        this.doc.removeEventListener(type, this._lockHandler, { capture: true });
      });
    }
    this._lockHandler = null;
    this._lockEvents = null;

    this._removeLockOverlay();

    return { success: true };
  }

  _createLockOverlay(message) {
    if (this._lockOverlay) return;

    // Create overlay container
    this._lockOverlay = this.doc.createElement("div");
    this._lockOverlay.id = "nevoflux-lock-overlay";
    this._lockOverlay.style.cssText = `
      position: fixed;
      inset: 0;
      z-index: 2147483646;
      background: rgba(0, 0, 0, 0.5);
      display: flex;
      align-items: center;
      justify-content: center;
    `;

    // Create inner container
    const container = this.doc.createElement("div");
    container.style.cssText = `
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 16px;
    `;

    // Create spinner
    const spinner = this.doc.createElement("div");
    spinner.style.cssText = `
      width: 48px;
      height: 48px;
      border: 3px solid rgba(255,255,255,0.3);
      border-top-color: white;
      border-radius: 50%;
      animation: nevoflux-spin 1s linear infinite;
    `;

    // Create message (using textContent to prevent XSS)
    const messageEl = this.doc.createElement("div");
    messageEl.style.cssText = `
      color: white;
      font-size: 14px;
      font-family: system-ui, sans-serif;
    `;
    messageEl.textContent = message || "Agent working...";

    // Create style for animation
    const style = this.doc.createElement("style");
    style.textContent = `
      @keyframes nevoflux-spin {
        to { transform: rotate(360deg); }
      }
    `;

    // Assemble
    container.appendChild(spinner);
    container.appendChild(messageEl);
    this._lockOverlay.appendChild(container);
    this._lockOverlay.appendChild(style);

    this.doc.body.appendChild(this._lockOverlay);
  }

  _removeLockOverlay() {
    this._lockOverlay?.remove();
    this._lockOverlay = null;
  }
}
