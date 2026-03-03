/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Import Turndown for HTML to Markdown conversion (with GFM tables support)
import { TurndownService, gfm } from 'resource:///actors/Turndown.sys.mjs';

// Lazy getter for accessibility service
const lazy = {};
ChromeUtils.defineLazyGetter(lazy, 'a11yService', () => {
  try {
    return Cc['@mozilla.org/accessibilityService;1'].getService(Ci.nsIAccessibilityService);
  } catch (e) {
    return null;
  }
});

// InspectorUtils is globally available in privileged Firefox contexts
// Used to detect event listeners added via addEventListener

// Interactive roles for filtering (A11y tree — Gecko nsIAccessible role names)
const INTERACTIVE_ROLES = new Set([
  'pushbutton',
  'button',
  'link',
  'entry',
  'password text',
  'text',
  'text container',
  'editable text',
  'searchbox',
  'checkbox',
  'radio button',
  'check menu item',
  'radio menu item',
  'toggle button',
  'combobox',
  'listbox',
  'option',
  'combobox option',
  'slider',
  'spinbutton',
  'menuitem',
  'menubar',
  'menu',
  'tab',
  'pagetab',
  'tablist',
  'tree item',
  'switch',
  'autocomplete',
  'editbar',
  'dropdown list',
]);

// Landmark roles for grouping in compact output
const LANDMARK_ROLES = new Set([
  'banner',
  'navigation',
  'main',
  'complementary',
  'contentinfo',
  'search',
  'form',
  'region',
]);

// Gecko role name → ARIA role (for a11y: locator protocol)
const ROLE_TO_ARIA = {
  pushbutton: 'button',
  entry: 'textbox',
  link: 'link',
  'password text': 'textbox',
  checkbox: 'checkbox',
  'radio button': 'radio',
  combobox: 'combobox',
  slider: 'slider',
  tab: 'tab',
  pagetab: 'tab',
  menuitem: 'menuitem',
  switch: 'switch',
  searchbox: 'searchbox',
  'toggle button': 'button',
  option: 'option',
  'combobox option': 'option',
  spinbutton: 'spinbutton',
  listbox: 'listbox',
  'tree item': 'treeitem',
  'editable text': 'textbox',
  'text container': 'textbox',
};

// Interactive events for InspectorUtils detection
const INTERACTIVE_EVENTS = new Set([
  'click',
  'mousedown',
  'mouseup',
  'pointerdown',
  'pointerup',
  'touchstart',
  'touchend',
  'keydown',
  'keyup',
  'keypress',
]);

// Role mapping from nsIAccessible role constants to readable strings
const ROLE_MAP = {
  1: 'pushbutton',
  3: 'check menu item',
  4: 'dropdown list',
  5: 'menu bar',
  6: 'scroll bar',
  7: 'grip',
  8: 'sound',
  9: 'cursor',
  10: 'caret',
  11: 'alert',
  12: 'window',
  13: 'internal frame',
  14: 'menupopup',
  15: 'menuitem',
  16: 'tooltip',
  17: 'application',
  18: 'document',
  19: 'pane',
  20: 'chart',
  21: 'dialog',
  22: 'border',
  23: 'grouping',
  24: 'separator',
  25: 'toolbar',
  26: 'statusbar',
  27: 'table',
  28: 'columnheader',
  29: 'rowheader',
  30: 'column',
  31: 'row',
  32: 'cell',
  33: 'link',
  34: 'helpballoon',
  35: 'character',
  36: 'list',
  37: 'listitem',
  38: 'outline',
  39: 'outlineitem',
  40: 'pagetab',
  41: 'propertypage',
  42: 'indicator',
  43: 'graphic',
  44: 'statictext',
  45: 'text leaf',
  46: 'pushbutton',
  47: 'checkbutton',
  48: 'radiobutton',
  49: 'combobox',
  50: 'droplist',
  51: 'progressbar',
  52: 'dial',
  53: 'hotkeyfield',
  54: 'slider',
  55: 'spinbutton',
  56: 'diagram',
  57: 'animation',
  58: 'equation',
  59: 'buttondropdown',
  60: 'buttonmenu',
  61: 'buttondropdowngrid',
  62: 'whitespace',
  63: 'pagetablist',
  64: 'clock',
  65: 'splitbutton',
  66: 'ipaddress',
  67: 'accel label',
  68: 'arrow',
  69: 'canvas',
  70: 'check menu item',
  71: 'color chooser',
  72: 'date editor',
  73: 'desktop icon',
  74: 'desktop frame',
  75: 'directory pane',
  76: 'file chooser',
  77: 'font chooser',
  78: 'chrome window',
  79: 'glass pane',
  80: 'html container',
  81: 'icon',
  82: 'label',
  83: 'layered pane',
  84: 'option pane',
  85: 'password text',
  86: 'popup menu',
  87: 'radio menu item',
  88: 'root pane',
  89: 'scroll pane',
  90: 'split pane',
  91: 'table column header',
  92: 'table row header',
  93: 'tear off menu item',
  94: 'terminal',
  95: 'text container',
  96: 'toggle button',
  97: 'tree table',
  98: 'viewport',
  99: 'header',
  100: 'footer',
  101: 'paragraph',
  102: 'ruler',
  103: 'autocomplete',
  104: 'editbar',
  105: 'entry',
  106: 'caption',
  107: 'document frame',
  108: 'heading',
  109: 'page',
  110: 'section',
  111: 'redundant object',
  112: 'form',
  113: 'ime',
  114: 'app root',
  115: 'parent menuitem',
  116: 'calendar',
  117: 'combobox list',
  118: 'combobox option',
  119: 'image map',
  120: 'option',
  121: 'listbox',
  122: 'flat equation',
  123: 'gridcell',
  124: 'embedded object',
  125: 'note',
  126: 'figure',
  127: 'check rich option',
  128: 'rich option',
  129: 'definition list',
  130: 'term',
  131: 'definition',
  132: 'key',
  133: 'switch',
  134: 'mathml math',
  135: 'mathml identifier',
  136: 'mathml number',
  137: 'mathml operator',
  138: 'mathml text',
  139: 'mathml string literal',
  140: 'mathml glyph',
  141: 'mathml row',
  142: 'mathml fraction',
  143: 'mathml sqrt',
  144: 'mathml root',
  145: 'mathml fenced',
  146: 'mathml enclosed',
  147: 'mathml style',
  148: 'mathml sub',
  149: 'mathml sup',
  150: 'mathml subsup',
  151: 'mathml under',
  152: 'mathml over',
  153: 'mathml underover',
  154: 'mathml multiscripts',
  155: 'mathml table',
  156: 'mathml labeled row',
  157: 'mathml table row',
  158: 'mathml cell',
  159: 'mathml action',
  160: 'mathml error',
  161: 'mathml stack',
  162: 'mathml long division',
  163: 'mathml stack group',
  164: 'mathml stack row',
  165: 'mathml stack carries',
  166: 'mathml stack carry',
  167: 'mathml stack line',
  168: 'details',
  169: 'summary',
  170: 'meter',
  171: 'navigation',
  172: 'complementary',
  173: 'contentinfo',
  174: 'main',
  175: 'search',
  176: 'banner',
  177: 'region',
  178: 'article',
  179: 'landmark',
  180: 'blockquote',
  181: 'mark',
  182: 'suggestion',
  183: 'comment',
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

  // ========== NevoFlux Page Detection ==========

  /**
   * Returns true when the current document is a nevoflux:// privileged page
   * (served via chrome://nevoflux/ protocol).
   */
  _isNevofluxPage() {
    const url = this.document?.URL;
    return !!url && (url.startsWith('chrome://nevoflux/') || url.startsWith('nevoflux://'));
  }

  receiveMessage({ name, data }) {
    if (name === 'execute') {
      return this.execute(data.action, data.params);
    }
    if (name === 'startPicker') {
      return this.startPicker(data);
    }
    if (name === 'stopPicker') {
      return this.stopPicker();
    }
    if (name === 'getSelection') {
      return this.getCurrentSelection();
    }
    if (name === 'lockPage') {
      return this.lockPage(data);
    }
    if (name === 'unlockPage') {
      return this.unlockPage();
    }

    // ---------- ContentStore messages (nevoflux:// pages only) ----------
    if (name === 'contentStore:update' && this._isNevofluxPage()) {
      const content = this.contentWindow;
      if (content) {
        const evt = new content.CustomEvent('NevofluxMessage', {
          detail: Cu.cloneInto(data, content),
        });
        content.dispatchEvent(evt);
      }
      return undefined;
    }

    // ---------- Agent push messages (canvas agent:chat sessions) ----------
    if (name === 'agent:push' && this._isNevofluxPage()) {
      const content = this.contentWindow;
      if (content) {
        const evt = new content.CustomEvent('NevofluxMessage', {
          detail: Cu.cloneInto(data, content),
        });
        content.dispatchEvent(evt);
      }
      return undefined;
    }

    return null;
  }

  async execute(action, params) {
    console.log('[NevofluxChild.execute] action:', action, 'params:', JSON.stringify(params));
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
      scroll: () => this.scroll(safeParams),
      waitForStable: () => this.waitForStable(safeParams),
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
    const el = this.currentDoc?.querySelector(selector);
    return el?.value || '';
  }

  // =====================================================================
  //  Snapshot Engine — 5-Phase Pipeline
  //  Phase 1: A11y Tree (primary semantic source)
  //  Phase 2: DOM Gap-Fill (catch what A11y misses)
  //  Phase 3: Merge + Occlusion + Dedup + Priority + Truncate
  //  Phase 4: Selector Generation (a11y: locator protocol)
  //  Phase 5: Serialization (compact for LLM, refs for program)
  // =====================================================================

  snapshot({ root, useA11y, domFallback, viewport_only, maxElements, keywords } = {}) {
    if (useA11y == null) useA11y = true;
    if (domFallback == null) domFallback = true;
    if (viewport_only == null) viewport_only = true;
    if (maxElements == null) maxElements = 200;

    const doc = this.currentDoc;
    const win = this.currentWin;
    if (!doc || !win) {
      return { type: 'full', tree: '', refs: {}, error: 'No document available' };
    }

    const rootEl = root ? doc.querySelector(root) : doc.body || doc.documentElement;
    if (!rootEl) {
      return { type: 'full', tree: '', refs: {}, error: 'Root element not found' };
    }

    const seenNodes = new Set(); // All A11y-traversed DOMNodes (not just interactive)
    let a11yResults = [];
    let domExtras = [];

    // === Phase 1: A11y Tree traversal ===
    if (useA11y && lazy.a11yService) {
      try {
        const accDoc = lazy.a11yService.getAccessibleFor(doc);
        if (accDoc) {
          this._walkA11yTree(accDoc, a11yResults, seenNodes, win, []);
        }
      } catch (e) {
        console.warn('[NevofluxChild.snapshot] A11y traversal failed:', e.message);
      }
    }

    // === Phase 2: DOM gap-fill ===
    if (domFallback) {
      domExtras = this._domPatchScan(rootEl, seenNodes, win);
    }

    // === Phase K: Keyword search (parallel to Phase 1+2) ===
    const keywordResults = this._keywordSearch(keywords, doc);

    // === Phase 3: Merge pipeline ===
    let elements = this._snapshotMerge(a11yResults, domExtras, keywordResults);
    const preOcclusionCount = elements.length;
    elements = this._filterOccluded(elements, doc);
    const occludedCount = preOcclusionCount - elements.length;
    elements = this._deduplicateNested(elements);
    elements = this._prioritize(elements);
    let truncatedCount = 0;
    if (elements.length > maxElements) {
      // Protect keyword-matched elements from truncation
      const kwElements = elements.filter(e => e.keywordMatch);
      const nonKwElements = elements.filter(e => !e.keywordMatch);
      const remainingSlots = Math.max(0, maxElements - kwElements.length);
      const keptNonKw = nonKwElements.slice(0, remainingSlots);
      truncatedCount = nonKwElements.length - keptNonKw.length;
      elements = [...kwElements, ...keptNonKw];
    }

    // === Phase 5 (before Phase 4): Clear old IDs and assign new ones ===
    this._clearPreviousAiIds(doc);
    let uid = 0;
    for (const el of elements) {
      el.id = `e${uid++}`;
      try {
        el.node.setAttribute('data-ai-id', el.id);
      } catch {}
    }

    // === Phase 4: Selector generation ===
    const docAcc = useA11y && lazy.a11yService ? lazy.a11yService.getAccessibleFor(doc) : null;
    for (const el of elements) {
      el.selectors = this._generateSelectors(el, doc, docAcc);
    }

    // === Phase 5: Serialization ===
    // Mark duplicate names for disambiguation
    this._markDuplicateNames(elements);
    // Compute landmarks for inferred elements
    for (const el of elements) {
      if (el.inferred && !el.landmark) {
        el.landmark = this._findLandmarkFromDOM(el.node);
      }
    }

    // Detect modal scroll state for viewportInfo and compact output
    let modalScrollInfo = null;
    try {
      const modalNodes = doc.querySelectorAll(
        '[role="dialog"], [role="alertdialog"], [aria-modal="true"]'
      );
      for (const modal of modalNodes) {
        const rect = modal.getBoundingClientRect();
        if (rect.width > 0 && rect.height > 0) {
          const scrollable = this._findScrollableChild(modal, win);
          if (scrollable && scrollable.scrollHeight > scrollable.clientHeight) {
            modalScrollInfo = {
              scrollTop: Math.round(scrollable.scrollTop),
              scrollHeight: Math.round(scrollable.scrollHeight),
              clientHeight: Math.round(scrollable.clientHeight),
              canScrollUp: scrollable.scrollTop > 0,
              canScrollDown:
                scrollable.scrollTop + scrollable.clientHeight < scrollable.scrollHeight - 1,
            };
            break;
          }
        }
      }
    } catch {}

    const compact = this._serializeCompact(elements, doc, win, truncatedCount, modalScrollInfo);
    const refs = this._buildRefs(elements);

    // Build viewportInfo
    const scrollTop = win.scrollY || 0;
    const scrollHeight = doc.documentElement.scrollHeight || 0;
    const viewportHeight = win.innerHeight || 0;
    const viewportWidth = win.innerWidth || 0;

    const viewportInfo = {
      scrollTop: Math.round(scrollTop),
      scrollHeight: Math.round(scrollHeight),
      viewportHeight,
      viewportWidth,
      canScrollUp: scrollTop > 0,
      canScrollDown: scrollTop + viewportHeight < scrollHeight - 1,
      pageTitle: doc.title || '',
      url: win.location?.href || '',
      modalScroll: modalScrollInfo,
    };

    return {
      type: 'full',
      tree: compact,
      refs,
      viewportInfo,
      stats: {
        total: elements.length,
        a11y: elements.filter((e) => !e.inferred).length,
        inferred: elements.filter((e) => e.inferred).length,
        occluded: occludedCount,
        truncated: truncatedCount,
      },
      url: viewportInfo.url,
      title: viewportInfo.pageTitle,
    };
  }

  // ── Phase 1: A11y Tree Traversal ──

  _walkA11yTree(acc, results, seenNodes, win, landmarkStack) {
    // Step 1: Viewport pruning (can skip entire subtree)
    const bx = {},
      by = {},
      bw = {},
      bh = {};
    try {
      acc.getBounds(bx, by, bw, bh);
    } catch {
      return;
    }
    const vr = {
      x: bx.value - win.mozInnerScreenX,
      y: by.value - win.mozInnerScreenY,
      width: bw.value,
      height: bh.value,
    };
    if (
      vr.y + vr.height < 0 ||
      vr.y > win.innerHeight ||
      vr.x + vr.width < 0 ||
      vr.x > win.innerWidth
    ) {
      return; // Subtree must also be outside viewport
    }

    // Step 2: Landmark stack maintenance
    const roleNum = acc.role;
    const roleName = ROLE_MAP[roleNum] || '';
    let currentLandmark = landmarkStack[landmarkStack.length - 1] || null;

    if (LANDMARK_ROLES.has(roleName)) {
      let accName = '';
      try {
        accName = acc.name || '';
      } catch {}
      currentLandmark = accName ? `${roleName} "${accName}"` : roleName;
      landmarkStack.push(currentLandmark);
    }

    // Step 3: Collect DOMNode (ALL nodes, not just interactive)
    let domNode = null;
    try {
      domNode = acc.DOMNode;
    } catch {}
    if (domNode?.nodeType === 1) {
      seenNodes.add(domNode); // Mark as A11y-traversed

      // Step 4: Interactive role filter (no subtree pruning)
      if (INTERACTIVE_ROLES.has(roleName)) {
        let accName = '';
        try {
          accName = acc.name || '';
        } catch {}

        results.push({
          node: domNode,
          role: roleName,
          name: accName,
          states: this._extractA11yStates(acc),
          viewportRect: vr,
          landmark: currentLandmark,
          inferred: false,
          signal: null,
        });
      }
    }

    // Always recurse children
    const count = acc.childCount || 0;
    for (let i = 0; i < count; i++) {
      try {
        const child = acc.getChildAt(i);
        if (child) this._walkA11yTree(child, results, seenNodes, win, landmarkStack);
      } catch {}
    }

    // Pop landmark scope
    if (LANDMARK_ROLES.has(roleName)) {
      landmarkStack.pop();
    }
  }

  _extractA11yStates(acc) {
    const states = {};
    try {
      const s1 = {},
        s2 = {};
      acc.getState(s1, s2);
      const state = s1.value || 0;
      const _extraState = s2.value || 0;
      // nsIAccessibleStates constants (from accessible/interfaces/nsIAccessibleStates.idl)
      if (state & 0x01) states.disabled = true; // STATE_UNAVAILABLE
      if (state & 0x02) states.selected = true; // STATE_SELECTED
      if (state & 0x04) states.focused = true; // STATE_FOCUSED
      if (state & 0x08) states.pressed = true; // STATE_PRESSED
      if (state & 0x10) states.checked = true; // STATE_CHECKED
      if (state & 0x40) states.readonly = true; // STATE_READONLY
      if (state & 0x200) states.expanded = true; // STATE_EXPANDED
      if (state & 0x400) states.expanded = false; // STATE_COLLAPSED (overrides)
    } catch {}
    return states;
  }

  // ── Phase 2: DOM Gap-Fill ──

  _domPatchScan(root, seenNodes, win) {
    const extras = [];
    const doc = root.ownerDocument || this.currentDoc;
    if (!doc) return extras;
    const promotedAncestors = new Set(); // Track ancestors already captured via child promotion

    const walk = (node) => {
      if (!node || node.nodeType !== 1) return;

      // Viewport pruning (first, all nodes — can skip subtree)
      let rect;
      try {
        rect = node.getBoundingClientRect();
      } catch {
        return;
      }
      const M = 50;
      if (
        rect.bottom < -M ||
        rect.top > win.innerHeight + M ||
        rect.right < -M ||
        rect.left > win.innerWidth + M
      ) {
        return; // Skip entire subtree
      }

      // Shadow DOM: targeted probe
      if (node.shadowRoot) {
        try {
          const probe = 'a,button,input,select,textarea,[role],[tabindex],[contenteditable]';
          const candidates = node.shadowRoot.querySelectorAll(probe);
          let hasUnknown = false;
          for (const el of candidates) {
            if (!seenNodes.has(el)) {
              hasUnknown = true;
              break;
            }
          }
          if (hasUnknown) {
            for (const child of node.shadowRoot.children) walk(child);
          }
        } catch {}
      }

      // Same-origin iframe
      if (node.tagName === 'IFRAME') {
        try {
          const body = node.contentDocument?.body;
          if (body) walk(body);
        } catch {} // Cross-origin: silent skip
      }

      // Gap-fill detection (only for A11y-uncovered nodes)
      if (!seenNodes.has(node)) {
        let signal = this._detectInteractiveSignal(node, win);
        if (signal && rect.width > 0 && rect.height > 0) {
          let captureNode = node;
          let captureRect = rect;
          let skip = false;

          // Promote cursor-detected children to their interactive ancestor
          // e.g. <a><span><strong>text</strong></span></a> → capture the <a>
          if (signal === 'cursor') {
            const ancestor = this._findInteractiveAncestor(node);
            if (ancestor) {
              if (promotedAncestors.has(ancestor)) {
                skip = true; // Already captured via another child
              } else {
                promotedAncestors.add(ancestor);
                captureNode = ancestor;
                signal = 'tag';
                try {
                  captureRect = ancestor.getBoundingClientRect();
                } catch {}
              }
            }
          }

          if (!skip) {
            extras.push({
              node: captureNode,
              tag: captureNode.tagName,
              text: this._getDirectText(captureNode).trim().slice(0, 60),
              signal,
              viewportRect: {
                x: captureRect.x,
                y: captureRect.y,
                width: captureRect.width,
                height: captureRect.height,
              },
              inferred: true,
            });
          }
        }
      }

      for (const child of node.children) walk(child);
    };

    walk(root);
    return extras;
  }

  _detectInteractiveSignal(el, win) {
    // Primary: InspectorUtils (catches all addEventListener events)
    try {
      if (typeof InspectorUtils !== 'undefined') {
        const unwrapped = el.wrappedJSObject || el;
        const listeners = InspectorUtils.getEventListenerInfoFor(unwrapped);
        if (listeners?.length > 0) {
          for (const l of listeners) {
            if (INTERACTIVE_EVENTS.has(l.type)) return 'listener';
          }
        }
      }
    } catch {}

    // Standard: native tag
    const tag = el.tagName;
    if (tag === 'A' && el.hasAttribute('href')) return 'tag';
    if (['BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'DETAILS', 'SUMMARY'].includes(tag)) return 'tag';

    // Standard: ARIA role
    const role = el.getAttribute('role');
    if (
      role &&
      [
        'button',
        'link',
        'textbox',
        'checkbox',
        'radio',
        'combobox',
        'menuitem',
        'tab',
        'switch',
        'option',
        'slider',
        'spinbutton',
        'searchbox',
        'listbox',
      ].includes(role)
    )
      return 'role';

    // Standard: tabindex >= 0
    if (el.hasAttribute('tabindex')) {
      const ti = parseInt(el.getAttribute('tabindex'), 10);
      if (ti >= 0) return 'tabindex';
    }

    // Standard: contenteditable
    if (el.isContentEditable && el.getAttribute('contenteditable') === 'true') return 'editable';

    // Heuristic: cursor:pointer with text and no child interactive elements
    try {
      const cs = win.getComputedStyle(el);
      if (cs.cursor === 'pointer') {
        const text = this._getDirectText(el).trim();
        if (
          text.length > 0 &&
          !el.querySelector("a,button,input,select,textarea,[role='button'],[role='link']")
        ) {
          return 'cursor';
        }
      }
    } catch {}

    // Heuristic: React/Vue framework handlers
    try {
      const unwrapped = el.wrappedJSObject || el;
      const keys = Object.keys(unwrapped);
      for (const key of keys) {
        if (key.startsWith('__reactProps$')) {
          try {
            const props = unwrapped[key];
            if (props?.onClick || props?.onMouseDown || props?.onPointerDown) return 'handler';
          } catch {}
        }
      }
      if (unwrapped._vei) {
        const vei = unwrapped._vei;
        if (
          vei.onClick ||
          vei.onMousedown ||
          vei.onPointerdown ||
          vei.onclick ||
          vei.onmousedown ||
          vei.onpointerdown
        )
          return 'handler';
      }
    } catch {}

    // Heuristic: SPA class/data patterns
    try {
      if (
        el.dataset?.controlName ||
        el.dataset?.action ||
        el.dataset?.click ||
        el.dataset?.toggle ||
        el.dataset?.entityUrn
      )
        return 'control';
      if (/^(btn|button|clickable|interactive|action)/i.test(el.className)) return 'control';
    } catch {}

    return null;
  }

  // Find nearest interactive ancestor for cursor-promoted elements (max 5 levels up)
  _findInteractiveAncestor(el) {
    const INTERACTIVE_TAGS = new Set(['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA']);
    const INTERACTIVE_ARIA = new Set([
      'button',
      'link',
      'textbox',
      'checkbox',
      'radio',
      'combobox',
      'menuitem',
      'tab',
      'switch',
      'option',
    ]);
    let p = el.parentElement;
    for (let i = 0; i < 5 && p; i++, p = p.parentElement) {
      if (INTERACTIVE_TAGS.has(p.tagName)) return p;
      const role = p.getAttribute('role');
      if (role && INTERACTIVE_ARIA.has(role)) return p;
    }
    return null;
  }

  _getDirectText(el) {
    let t = '';
    for (const n of el.childNodes) {
      if (n.nodeType === 3) {
        // Text node
        t += n.textContent;
      } else if (n.nodeType === 1) {
        // Exclude child interactive elements' text
        const tag = n.tagName;
        if (
          !['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(tag) &&
          !n.getAttribute('role')
        ) {
          t += this._getDirectText(n);
        }
      }
    }
    return t.replace(/\s+/g, ' ');
  }

  /**
   * Walk up DOM tree to find nearest interactive ancestor of a text node.
   * Mirrors the interactivity detection in _detectInteractiveSignal but
   * operates upward from a known text match.
   */
  _findClosestInteractable(el) {
    let node = el;
    while (node && node !== node.ownerDocument.body) {
      const tag = node.tagName?.toLowerCase();
      const role = node.getAttribute?.('role');

      // Native interactive elements
      if (['a', 'button', 'input', 'select', 'textarea'].includes(tag)) return node;

      // ARIA interactive roles
      if (['button', 'link', 'textbox', 'checkbox', 'menuitem', 'tab', 'switch', 'combobox'].includes(role)) return node;

      // Explicit tabindex
      if (node.getAttribute?.('tabindex') !== null) return node;

      // Visual cursor hint
      try {
        if (node.ownerGlobal?.getComputedStyle(node).cursor === 'pointer') return node;
      } catch (e) { /* getComputedStyle can throw for detached nodes */ }

      node = node.parentElement;
    }
    return el; // Fallback: return text element itself
  }

  /**
   * Fallback keyword search for icon-only elements (no rendered text).
   * Checks aria-label and title attributes.
   */
  _fallbackAttrSearch(keywords, doc) {
    const results = [];
    const els = doc.querySelectorAll('[aria-label], [title]');
    for (const el of els) {
      const label = el.getAttribute('aria-label') || el.getAttribute('title');
      if (!label) continue;
      const lowerLabel = label.toLowerCase();
      for (const kw of keywords) {
        if (lowerLabel.includes(kw.toLowerCase())) {
          const rect = el.getBoundingClientRect();
          if (rect.width === 0 || rect.height === 0) continue;
          results.push({
            keyword: kw,
            element: el,
            rect,
            source: 'aria-fallback',
          });
        }
      }
    }
    return results;
  }

  /**
   * Phase K: nsIFind-based keyword search.
   * Uses Gecko's native text search (same engine as Ctrl+F) to find
   * elements by visible text, then resolves to interactable ancestors.
   *
   * @param {string[]} keywords - Keywords to search for
   * @param {Document} doc - The document to search in
   * @returns {Array} Deduplicated keyword match results
   */
  _keywordSearch(keywords, doc) {
    if (!keywords || keywords.length === 0) return [];
    if (!doc.body) return [];

    const finder = Cc["@mozilla.org/embedcomp/rangefind;1"]
      .createInstance()
      .QueryInterface(Ci.nsIFind);
    finder.caseSensitive = false;
    finder.entireWord = false;

    const rawResults = [];

    for (const keyword of keywords) {
      const searchRange = doc.createRange();
      searchRange.selectNodeContents(doc.body);

      let startPoint = searchRange.cloneRange();
      startPoint.collapse(true);
      let endPoint = searchRange.cloneRange();
      endPoint.collapse(false);

      let found;
      while ((found = finder.Find(keyword, searchRange, startPoint, endPoint))) {
        const textNode = found.startContainer;
        const element = textNode.nodeType === 3 ? textNode.parentElement : textNode;
        if (!element) {
          startPoint = found.cloneRange();
          startPoint.collapse(false);
          continue;
        }

        const rect = found.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) {
          startPoint = found.cloneRange();
          startPoint.collapse(false);
          continue;
        }

        const interactable = this._findClosestInteractable(element);

        rawResults.push({
          keyword,
          exactText: found.toString(),
          element: interactable || element,
          rect: interactable ? interactable.getBoundingClientRect() : rect,
          source: 'nsifind',
        });

        startPoint = found.cloneRange();
        startPoint.collapse(false);
      }
    }

    // Fallback: aria-label/title for icon-only elements
    const attrResults = this._fallbackAttrSearch(keywords, doc);
    const allResults = rawResults.concat(attrResults);

    // Dedup: merge keywords per unique element
    return this._dedupKeywordResults(allResults);
  }

  /**
   * Deduplicate keyword results by element identity.
   * When multiple keywords match the same interactable element,
   * merge into a single entry with combined keyword list.
   */
  _dedupKeywordResults(results) {
    const byElement = new Map();
    for (const r of results) {
      const existing = byElement.get(r.element);
      if (existing) {
        if (!existing.keywords.includes(r.keyword)) {
          existing.keywords.push(r.keyword);
        }
      } else {
        byElement.set(r.element, {
          node: r.element,
          keywords: [r.keyword],
          rect: r.rect,
          source: r.source,
        });
      }
    }
    return [...byElement.values()];
  }

  // ── Phase 3: Merge Pipeline ──

  _snapshotMerge(a11yResults, domExtras, keywordResults = []) {
    const merged = [];

    for (const el of a11yResults) {
      merged.push({ node: el.node, role: el.role, name: el.name,
                    states: el.states, viewportRect: el.viewportRect,
                    landmark: el.landmark, inferred: false, signal: null,
                    keywordMatch: null });
    }
    for (const el of domExtras) {
      merged.push({ node: el.node, role: this._tagToRole(el.tag),
                    name: el.text, states: {}, viewportRect: el.viewportRect,
                    landmark: null, inferred: true, signal: el.signal,
                    keywordMatch: null });
    }

    // Merge keyword results: enrich existing or add new
    for (const kwResult of keywordResults) {
      const existingIdx = merged.findIndex(m => m.node === kwResult.node);
      if (existingIdx >= 0) {
        // Element already found by Phase 1 or 2 — enrich with keyword metadata
        merged[existingIdx].keywordMatch = kwResult.keywords;
      } else {
        // New element recovered by keyword search — add with inferred role
        const tag = kwResult.node.tagName?.toLowerCase() || 'unknown';
        const role = this._tagToRole(tag);
        const name = this._getDirectText(kwResult.node).trim().slice(0, 60);
        const rect = kwResult.rect;
        merged.push({
          node: kwResult.node,
          role: role || '?keyword',
          name: name || kwResult.keywords[0],
          states: {},
          viewportRect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          landmark: null,
          inferred: true,
          signal: 'keyword',
          keywordMatch: kwResult.keywords,
        });
      }
    }

    return merged;
  }

  _tagToRole(tag) {
    switch (tag) {
      case 'A':
        return 'link';
      case 'BUTTON':
        return 'button';
      case 'INPUT':
        return 'textbox';
      case 'SELECT':
        return 'listbox';
      case 'TEXTAREA':
        return 'textbox';
      case 'DETAILS':
        return 'details';
      case 'SUMMARY':
        return 'summary';
      case 'IMG':
        return 'img';
      default:
        return tag.toLowerCase();
    }
  }

  _filterOccluded(elements, doc) {
    // ── Modal-aware occlusion: detect active dialog overlays ──
    // When a modal dialog is open, its backdrop causes elementFromPoint to
    // return the overlay for ALL page elements, resulting in 0 hits.
    // Fix: skip elementFromPoint for modal children; filter out non-modal elements.
    const activeModals = [];
    try {
      const modalNodes = doc.querySelectorAll(
        '[role="dialog"], [role="alertdialog"], [aria-modal="true"]'
      );
      for (const modal of modalNodes) {
        try {
          const rect = modal.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            activeModals.push(modal);
          }
        } catch {}
      }
    } catch {}

    const hasActiveModal = activeModals.length > 0;
    const vw = doc.defaultView?.innerWidth || 0;
    const vh = doc.defaultView?.innerHeight || 0;

    return elements.filter((el) => {
      const rect = el.viewportRect;

      // ── Modal shortcut ──
      if (hasActiveModal) {
        const isInModal = activeModals.some((m) => m.contains(el.node));
        if (isInModal) {
          // Modal children: skip elementFromPoint (backdrop interferes),
          // just verify the element is within the viewport and has size.
          return (
            rect.width > 0 &&
            rect.height > 0 &&
            rect.y + rect.height > 0 &&
            rect.y < vh &&
            rect.x + rect.width > 0 &&
            rect.x < vw
          );
        }
        // Non-modal elements: behind the backdrop, not interactable
        return false;
      }

      // ── Standard 5-point occlusion sampling (no modal) ──
      const points = [
        [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
        [rect.x + rect.width * 0.25, rect.y + rect.height * 0.25],
        [rect.x + rect.width * 0.75, rect.y + rect.height * 0.75],
        [rect.x + rect.width * 0.75, rect.y + rect.height * 0.25],
        [rect.x + rect.width * 0.25, rect.y + rect.height * 0.75],
      ];

      let hits = 0;
      for (const [x, y] of points) {
        if (x < 0 || y < 0 || x >= vw || y >= vh) continue;
        try {
          const topEl = doc.elementFromPoint(x, y);
          if (topEl && (el.node === topEl || el.node.contains(topEl) || topEl.contains(el.node))) {
            hits++;
          }
        } catch {}
      }

      return hits > 0;
    });
  }

  _deduplicateNested(elements) {
    const nodeSet = new Set(elements.map((e) => e.node));
    const toRemove = new Set();

    for (const el of elements) {
      if (toRemove.has(el.node)) continue;

      // Find interactive ancestor (up to 6 levels)
      let p = el.node.parentElement;
      let parent = null;
      for (let i = 0; i < 6 && p; i++, p = p.parentElement) {
        if (nodeSet.has(p)) {
          parent = p;
          break;
        }
      }
      if (!parent) continue;

      const parentEl = elements.find((e) => e.node === parent);
      if (!parentEl || toRemove.has(parent)) continue;

      if (!el.inferred && !parentEl.inferred) {
        // R1/R2: Both A11y
        const childName = (el.name || '').trim();
        const parentName = (parentEl.name || '').trim();
        if (childName === parentName || childName.length === 0 || parentName.includes(childName)) {
          toRemove.add(el.node); // R1
        }
        // R2: keep both (different text = independent functions)
      } else if (el.inferred && !parentEl.inferred) {
        toRemove.add(el.node); // R3: A11y parent covers inferred child
      } else if (!el.inferred && parentEl.inferred) {
        // R4: Keep A11y child, remove inferred parent
        // (May trigger multiple times for same parent — safe)
        toRemove.add(parent);
      } else {
        // R5: Both inferred — keep the one with longer text
        if ((el.name || '').length >= (parentEl.name || '').length) {
          toRemove.add(parent);
        } else {
          toRemove.add(el.node);
        }
      }
    }

    return elements.filter((e) => !toRemove.has(e.node));
  }

  _prioritize(elements) {
    return elements.sort((a, b) => {
      const pa = this._elementPriority(a);
      const pb = this._elementPriority(b);
      if (pa !== pb) return pb - pa;
      // Same priority: top-to-bottom, left-to-right
      return a.viewportRect.y - b.viewportRect.y || a.viewportRect.x - b.viewportRect.x;
    });
  }

  _elementPriority(el) {
    let p = 0;

    // Source weight
    if (!el.inferred) p += 10;

    // Functional weight (A11y: role-based)
    if (!el.inferred) {
      const role = el.role;
      if (
        [
          'entry',
          'searchbox',
          'textbox',
          'password text',
          'combobox',
          'spinbutton',
          'editable text',
        ].includes(role)
      )
        p += 15;
      if (['pushbutton', 'button', 'toggle button', 'switch'].includes(role)) p += 12;
      if (role === 'link') p += 8;
      if (['checkbox', 'radio button', 'option', 'menuitem', 'tab', 'pagetab'].includes(role))
        p += 6;
    }

    // Functional weight (Inferred: tag + signal based)
    if (el.inferred) {
      const tag = el.role.toUpperCase();
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(tag)) p += 15;
      if (el.signal === 'editable') p += 14;
      const ariaRole = el.node?.getAttribute?.('role') || '';
      if (['button', 'link', 'textbox', 'searchbox', 'combobox', 'tab'].includes(ariaRole)) p += 12;
      if (el.signal === 'listener') p += 8;
      if (el.signal === 'tabindex') p += 6;
      if (el.signal === 'cursor') p += 3;
      if (el.signal === 'handler' || el.signal === 'control') p += 2;
    }

    // Name bonus
    if (el.name && el.name.trim().length > 0) p += 5;

    return p;
  }

  // ── Phase 4: Selector Generation ──

  _generateSelectors(el, doc, docAcc) {
    const node = el.node;
    const selectors = [];

    // A11y elements: try a11y: locator protocol first
    if (!el.inferred && el.role && el.name) {
      const ariaRole = ROLE_TO_ARIA[el.role];
      if (ariaRole) {
        // Try CSS form first (only when name = aria-label)
        const ariaLabel = node.getAttribute('aria-label');
        if (ariaLabel === el.name) {
          const explicitRole = node.getAttribute('role');
          const css = explicitRole
            ? `[role="${ariaRole}"][aria-label="${this._cssEscape(el.name)}"]`
            : `${node.tagName.toLowerCase()}[aria-label="${this._cssEscape(el.name)}"]`;
          if (this._isUniqueSelector(css, node, doc)) {
            selectors.push({ type: 'css', strategy: 'role', value: css });
          }
        }

        // a11y: locator (works for name from any source)
        if (selectors.length === 0 || selectors[0].strategy !== 'role') {
          const a11yLoc = `a11y:${ariaRole}/${el.name}`;
          if (!docAcc || this._isUniqueA11y(docAcc, ariaRole, el.name)) {
            selectors.push({ type: 'a11y', strategy: 'role', value: a11yLoc });
          }
        }
      }
    }

    // Shared path: aria-label
    const ariaLabel = node.getAttribute('aria-label');
    if (ariaLabel && !selectors.some((s) => s.strategy === 'role' && s.type === 'css')) {
      const s = `[aria-label="${this._cssEscape(ariaLabel)}"]`;
      if (this._isUniqueSelector(s, node, doc)) {
        selectors.push({ type: 'css', strategy: 'aria', value: s });
      }
    }

    // Shared: placeholder
    const ph = node.getAttribute('placeholder');
    if (ph) {
      const s = `[placeholder="${this._cssEscape(ph)}"]`;
      if (this._isUniqueSelector(s, node, doc)) {
        selectors.push({ type: 'css', strategy: 'ph', value: s });
      }
    }

    // Shared: label[for] → use the input's own #id as selector (label confirms identity)
    if (node.id && !this._isDynamicId(node.id)) {
      try {
        const label = doc.querySelector(`label[for="${this._cssEscape(node.id)}"]`);
        if (label) {
          const s = `#${CSS.escape(node.id)}`;
          if (this._isUniqueSelector(s, node, doc)) {
            selectors.push({ type: 'css', strategy: 'label', value: s });
          }
        }
      } catch {}
    }

    // Shared: data-testid etc
    for (const attr of ['data-testid', 'data-test-id', 'data-control-name']) {
      const val = node.getAttribute(attr);
      if (val) {
        selectors.push({
          type: 'css',
          strategy: 'testid',
          value: `[${attr}="${this._cssEscape(val)}"]`,
        });
        break;
      }
    }

    // Shared: stable id
    if (node.id && !this._isDynamicId(node.id)) {
      selectors.push({ type: 'css', strategy: 'id', value: `#${node.id}` });
    }

    // CSS path fallback (only if unique)
    const cssPath = this._buildCssPath(node, doc);
    if (cssPath && this._isUniqueSelector(cssPath, node, doc)) {
      selectors.push({ type: 'css', strategy: 'css', value: cssPath });
    }

    return selectors;
  }

  _isUniqueSelector(selector, expectedNode, doc) {
    try {
      const matches = doc.querySelectorAll(selector);
      return matches.length === 1 && matches[0] === expectedNode;
    } catch {
      return false;
    }
  }

  _isUniqueA11y(docAcc, targetRole, targetName) {
    let count = 0;
    const walk = (acc) => {
      if (count > 1) return;
      const roleName = ROLE_MAP[acc.role] || '';
      const ariaRole = ROLE_TO_ARIA[roleName] || '';
      let name = '';
      try {
        name = acc.name || '';
      } catch {}
      if (ariaRole === targetRole && name === targetName) {
        count++;
        if (count > 1) return;
      }
      for (let i = 0; i < (acc.childCount || 0); i++) {
        try {
          const child = acc.getChildAt(i);
          if (child) walk(child);
          if (count > 1) return;
        } catch {}
      }
    };
    walk(docAcc);
    return count === 1;
  }

  _isDynamicId(id) {
    return /[0-9a-f]{8,}|ember\d+|react|:r[0-9a-z]+:|\d{6,}/.test(id);
  }

  _buildCssPath(el, doc) {
    const parts = [];
    let cur = el;
    for (let i = 0; i < 4 && cur && cur !== doc.body; i++, cur = cur.parentElement) {
      let seg = cur.tagName.toLowerCase();
      if (cur.id && !this._isDynamicId(cur.id)) {
        parts.unshift(`#${cur.id}`);
        break;
      }
      const parent = cur.parentElement;
      if (parent) {
        const sibs = [...parent.children].filter((c) => c.tagName === cur.tagName);
        if (sibs.length > 1) seg += `:nth-of-type(${sibs.indexOf(cur) + 1})`;
      }
      parts.unshift(seg);
    }
    return parts.join('>');
  }

  _cssEscape(s) {
    return (s || '').replace(/["\\[\]]/g, '\\$&');
  }

  // ── Phase 5: Serialization ──

  _clearPreviousAiIds(doc) {
    try {
      const old = doc.querySelectorAll('[data-ai-id]');
      for (const el of old) el.removeAttribute('data-ai-id');
    } catch {}
  }

  _markDuplicateNames(elements) {
    const groups = new Map();
    for (const el of elements) {
      if (!el.name) continue;
      const key = `${el.inferred ? '?' : ''}${el.role}:${el.name}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(el);
    }
    for (const [, group] of groups) {
      if (group.length > 1) {
        for (const el of group) el._hasDuplicateName = true;
      }
    }
  }

  _findNearestContext(el) {
    const node = el.node;
    // Strategy 1: nearest heading or aria-label container
    let p = node.parentElement;
    for (let i = 0; i < 5 && p; i++, p = p.parentElement) {
      try {
        const heading = p.querySelector("h1,h2,h3,h4,h5,h6,[role='heading']");
        if (heading && heading !== node) {
          return heading.textContent.trim().slice(0, 30);
        }
        const label = p.getAttribute('aria-label');
        if (label) return label.slice(0, 30);
      } catch {}
    }
    // Strategy 2: nearest image alt in card container
    try {
      const container = node.closest("[class*='card'],[class*='item'],[role='article']");
      if (container) {
        const img = container.querySelector('img[alt]');
        if (img?.alt) return img.alt.slice(0, 30);
      }
    } catch {}
    return null;
  }

  _findLandmarkFromDOM(node) {
    let p = node.parentElement;
    for (let i = 0; i < 10 && p; i++, p = p.parentElement) {
      const role = p.getAttribute('role');
      if (role && LANDMARK_ROLES.has(role)) {
        const label = p.getAttribute('aria-label');
        return label ? `${role} "${label}"` : role;
      }
      const tag = p.tagName;
      if (tag === 'NAV') {
        const label = p.getAttribute('aria-label');
        return label ? `navigation "${label}"` : 'navigation';
      }
      if (tag === 'MAIN') return 'main';
      if (tag === 'HEADER') {
        const label = p.getAttribute('aria-label');
        return label ? `banner "${label}"` : 'banner';
      }
      if (tag === 'FOOTER') {
        const label = p.getAttribute('aria-label');
        return label ? `contentinfo "${label}"` : 'contentinfo';
      }
      if (tag === 'ASIDE') {
        const label = p.getAttribute('aria-label');
        return label ? `complementary "${label}"` : 'complementary';
      }
    }
    return null;
  }

  _serializeCompact(elements, doc, win, truncatedCount, modalScrollInfo = null) {
    const lines = [];

    // Page header
    lines.push(`# ${doc.title || ''}`);
    const loc = win.location || {};
    lines.push(`@ ${loc.pathname || ''}${loc.search || ''}`);

    const scrollTop = Math.round(win.scrollY || 0);
    const scrollHeight = doc.documentElement.scrollHeight || 0;
    const vh = win.innerHeight || 0;
    const vw = win.innerWidth || 0;
    const pct = scrollHeight > vh ? Math.round((scrollTop / (scrollHeight - vh)) * 100) : 0;
    const pos = scrollTop === 0 ? 'top' : scrollTop + vh >= scrollHeight - 1 ? 'bottom' : `${pct}%`;
    lines.push(`viewport: ${vw}x${vh} scroll: ${scrollTop}/${scrollHeight} (${pos})`);

    // Modal scroll state: tells LLM the modal is scrollable
    if (modalScrollInfo) {
      const mPct =
        modalScrollInfo.scrollHeight > modalScrollInfo.clientHeight
          ? Math.round(
              (modalScrollInfo.scrollTop /
                (modalScrollInfo.scrollHeight - modalScrollInfo.clientHeight)) *
                100
            )
          : 0;
      const mPos =
        modalScrollInfo.scrollTop === 0
          ? 'top'
          : !modalScrollInfo.canScrollDown
            ? 'bottom'
            : `${mPct}%`;
      let mLine = `modal-scroll: ${modalScrollInfo.scrollTop}/${modalScrollInfo.scrollHeight} (${mPos})`;
      if (modalScrollInfo.canScrollDown) mLine += ' ▼ has more content below';
      if (modalScrollInfo.canScrollUp) mLine += ' ▲ has content above';
      lines.push(mLine);
    }
    lines.push('');

    // Separate keyword-matched elements for priority output
    const kwElements = elements.filter(e => e.keywordMatch);
    const otherElements = elements.filter(e => !e.keywordMatch);

    if (kwElements.length > 0) {
      lines.push('');
      lines.push('=== KEYWORD MATCHES ===');
      for (const el of kwElements) {
        lines.push(this._elementToCompactLine(el, win));
      }
      lines.push('');
      lines.push('=== OTHER INTERACTABLES ===');
    }

    // Group by landmark
    const groups = new Map();
    for (const el of otherElements) {
      const lm = el.landmark || null;
      if (!groups.has(lm)) groups.set(lm, []);
      groups.get(lm).push(el);
    }
    // Named landmarks first, null last
    const sorted = new Map();
    for (const [lm, items] of groups) {
      if (lm) sorted.set(lm, items);
    }
    const noLandmark = groups.get(null);
    if (noLandmark?.length) sorted.set(null, noLandmark);

    for (const [landmark, items] of sorted) {
      if (landmark) lines.push(`## ${landmark}`);
      for (const el of items) {
        lines.push(this._elementToCompactLine(el, win));
      }
      lines.push('');
    }

    if (truncatedCount > 0) {
      lines.push(`(+${truncatedCount} more elements truncated)`);
    }

    return lines.join('\n');
  }

  _elementToCompactLine(el, win) {
    let line = `[${el.id}]`;

    // Prefix: A11y role or ?signal for inferred
    if (el.inferred) {
      line += ` ?${el.signal || 'unknown'}`;
    } else {
      line += ` ${el.role}`;
    }

    // Name
    if (el.name) {
      line += ` "${el.name}"`;
    }
    if (el.keywordMatch) { line += ` (keywords: ${el.keywordMatch.map(k => `"${k}"`).join(', ')})`; }

    // Disambiguation context (only for duplicate names)
    if (el._hasDuplicateName) {
      const ctx = this._findNearestContext(el);
      if (ctx) line += ` (near "${ctx}")`;
    }

    // Optional attributes
    const node = el.node;
    try {
      const ph = node.getAttribute('placeholder');
      if (ph) line += ` ph="${ph}"`;
    } catch {}

    // States (symbols)
    if (el.states?.checked) line += ' \u2713';
    if (el.states?.expanded === true) line += ' \u25BC';
    if (el.states?.expanded === false) line += ' \u25B6';
    if (el.states?.selected) line += ' [sel]';
    if (el.states?.disabled) line += ' [dis]';
    if (el.states?.focused) line += ' [foc]';

    // Value for inputs
    try {
      if (node.value) {
        const v = node.value.length > 30 ? node.value.slice(0, 27) + '...' : node.value;
        line += ` val="${v}"`;
      }
    } catch {}

    // Link target (shortened for same-origin)
    try {
      if (node.tagName === 'A' && node.href) {
        const url = new URL(node.href);
        if (url.origin === win.location.origin) {
          line += ` \u2192 ${url.pathname}`;
        } else {
          line += ` \u2192 ${url.host}${url.pathname}`;
        }
      }
    } catch {}

    return line;
  }

  _buildRefs(elements) {
    const refs = {};
    for (const el of elements) {
      refs[el.id] = {
        selectors: el.selectors || [],
        role: el.role,
        name: el.name || '',
        tagName: el.node.tagName?.toLowerCase() || '',
        rect: el.viewportRect
          ? {
              x: Math.round(el.viewportRect.x),
              y: Math.round(el.viewportRect.y),
              width: Math.round(el.viewportRect.width),
              height: Math.round(el.viewportRect.height),
            }
          : null,
        inferred: el.inferred,
        ...(el.signal ? { signal: el.signal } : {}),
      };
    }
    return refs;
  }

  // ── a11y: Locator Resolver (for act() operations) ──

  resolveLocator(selector, doc) {
    if (!selector) return null;

    // Type: CSS
    if (selector.type === 'css') {
      try {
        return doc.querySelector(selector.value);
      } catch {
        return null;
      }
    }

    // Type: a11y: protocol
    if (selector.type === 'a11y') {
      const match = selector.value.match(/^a11y:([^/]+)\/(.+)$/);
      if (!match) return null;
      const [, targetRole, targetName] = match;

      if (!lazy.a11yService) return null;
      const docAcc = lazy.a11yService.getAccessibleFor(doc);
      if (!docAcc) return null;

      return this._findByRoleName(docAcc, targetRole, targetName);
    }

    return null;
  }

  _findByRoleName(acc, targetRole, targetName) {
    const roleName = ROLE_MAP[acc.role] || '';
    const ariaRole = ROLE_TO_ARIA[roleName] || '';
    let name = '';
    try {
      name = acc.name || '';
    } catch {}

    if (ariaRole === targetRole && name === targetName) {
      try {
        return acc.DOMNode;
      } catch {
        return null;
      }
    }

    for (let i = 0; i < (acc.childCount || 0); i++) {
      try {
        const child = acc.getChildAt(i);
        if (!child) continue;
        const found = this._findByRoleName(child, targetRole, targetName);
        if (found) return found;
      } catch {}
    }
    return null;
  }

  // ── Resolve element by snapshot ID (used by act operations) ──

  resolveSnapshotElement(id, doc) {
    // 1. Direct data-ai-id lookup (O(1))
    const direct = doc.querySelector(`[data-ai-id="${id}"]`);
    if (direct) return direct;
    return null;
  }

  async screenshot({ fullPage = false, type = 'jpeg', quality = 60, maxWidth = 1280 }) {
    const win = this.contentWindow;
    const doc = this.document;

    try {
      // Calculate source dimensions
      const srcWidth = fullPage ? doc.documentElement.scrollWidth : win.innerWidth;
      const srcHeight = fullPage ? doc.documentElement.scrollHeight : win.innerHeight;

      // First capture at full resolution
      const srcCanvas = doc.createElement('canvas');
      srcCanvas.width = srcWidth;
      srcCanvas.height = srcHeight;
      const srcCtx = srcCanvas.getContext('2d');

      // Use drawWindow (privileged Firefox API available in JSWindowActors)
      if (fullPage) {
        srcCtx.drawWindow(win, 0, 0, srcWidth, srcHeight, 'rgb(255,255,255)');
      } else {
        const scrollX = win.scrollX;
        const scrollY = win.scrollY;
        srcCtx.drawWindow(win, scrollX, scrollY, srcWidth, srcHeight, 'rgb(255,255,255)');
      }

      // Downscale if needed to reduce size for LLM token efficiency
      let outputCanvas = srcCanvas;
      let outputWidth = srcWidth;
      let outputHeight = srcHeight;

      if (maxWidth > 0 && srcWidth > maxWidth) {
        const scale = maxWidth / srcWidth;
        outputWidth = Math.round(srcWidth * scale);
        outputHeight = Math.round(srcHeight * scale);

        outputCanvas = doc.createElement('canvas');
        outputCanvas.width = outputWidth;
        outputCanvas.height = outputHeight;
        const outCtx = outputCanvas.getContext('2d');
        // Use smooth scaling for better quality at smaller size
        outCtx.imageSmoothingEnabled = true;
        outCtx.imageSmoothingQuality = 'high';
        outCtx.drawImage(srcCanvas, 0, 0, outputWidth, outputHeight);
      }

      // Convert to data URL - default JPEG for much smaller size
      const mimeType = type === 'png' ? 'image/png' : 'image/jpeg';
      const qualityArg = type !== 'png' ? quality / 100 : undefined;
      const dataUrl = outputCanvas.toDataURL(mimeType, qualityArg);

      // Extract base64 data (remove "data:image/...;base64," prefix)
      const base64Data = dataUrl.split(',')[1] || '';

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
      style.visibility !== 'hidden' &&
      style.display !== 'none' &&
      style.opacity !== '0'
    );
  }

  exists({ selector }) {
    return this.currentDoc?.querySelector(selector) !== null;
  }

  // ========== Interaction ==========

  async click({ selector, button = 'left', clickCount = 1, delay = 0, force = false }) {
    const doc = this.currentDoc;
    const win = this.currentWin;
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    // 1. Get the element from selector
    let el = doc.querySelector(selector);
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

    console.log('[NevofluxChild.click] selector:', selector);
    console.log('[NevofluxChild.click] element:', el.tagName, el.className);

    // 2. Ensure element is visible (scroll into view if needed)
    if (!force && !this.isVisible({ selector })) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
      await this.sleep(300);
      if (!this.isVisible({ selector })) {
        return {
          success: false,
          error: {
            code: 1002,
            message: 'Element not visible',
            recoverable: true,
            suggestion: 'Use force: true to click anyway',
          },
        };
      }
    }

    // 3. Resolve actual click target (handle pointer-events: none)
    let targetEl = el;
    try {
      const style = win.getComputedStyle(targetEl);
      if (style.pointerEvents === 'none') {
        const clickableChild = this._findClickableDescendant(targetEl, win);
        if (clickableChild) {
          console.log(
            '[NevofluxChild.click] Bypassing pointer-events:none, using child:',
            clickableChild.tagName
          );
          targetEl = clickableChild;
        }
      }
    } catch (e) {
      /* ignore style access errors */
    }

    // 4. Calculate click coordinates
    const rect = targetEl.getBoundingClientRect();
    const buttonCode = { left: 0, middle: 1, right: 2 }[button] || 0;

    // 5. Find unobstructed click point (multi-point strategy)
    let clickPoint = this._findUnobstructedPoint(targetEl, doc);
    if (!clickPoint) {
      clickPoint = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      console.log('[NevofluxChild.click] All points obstructed, using center');
    }

    console.log('[NevofluxChild.click] Target:', targetEl.tagName, 'coords:', clickPoint, 'rect:', {
      width: rect.width,
      height: rect.height,
    });

    // 6. Check what element is at the click point
    const elementAtPoint = doc.elementFromPoint(clickPoint.x, clickPoint.y);
    const isTargetAtPoint = elementAtPoint === targetEl || targetEl.contains(elementAtPoint);
    console.log(
      '[NevofluxChild.click] elementFromPoint:',
      elementAtPoint?.tagName,
      'isTargetAtPoint:',
      isTargetAtPoint
    );

    // 7. Get windowUtils for trusted events
    const domUtils = this._getWindowUtils();

    // 8. Tiered click with DOM change detection between tiers
    //    Each tier fires a click method, then waits up to 500ms for DOM/network effect.
    //    If effect detected, skip remaining tiers to avoid double/triple firing.
    let clickMethod = 'none';
    let domChanged = false;
    let networkRequestMade = false;

    try {
      // Set up effect watcher BEFORE any click (captures changes during click)
      const watcher = this._setupClickEffectWatcher(doc, win);

      // --- Tier 1: windowUtils.sendMouseEvent (trusted events through browser input pipeline) ---
      if (domUtils && typeof domUtils.sendMouseEvent === 'function') {
        console.log(
          '[NevofluxChild.click] Tier 1: windowUtils.sendMouseEvent at',
          clickPoint.x,
          clickPoint.y
        );
        for (let i = 0; i < clickCount; i++) {
          domUtils.sendMouseEvent('mousemove', clickPoint.x, clickPoint.y, buttonCode, 0, 0);
          await this.sleep(10);
          domUtils.sendMouseEvent('mousedown', clickPoint.x, clickPoint.y, buttonCode, 1, 0);
          await this.sleep(50);
          domUtils.sendMouseEvent('mouseup', clickPoint.x, clickPoint.y, buttonCode, 1, 0);
          if (delay > 0 && i < clickCount - 1) await this.sleep(delay);
        }

        const tier1 = await watcher.waitForEffect(500);
        if (tier1.changed) {
          clickMethod = 'trusted_event';
          domChanged = tier1.domChanged;
          networkRequestMade = tier1.networkRequest;
          console.log(
            '[NevofluxChild.click] Tier 1 effective - domChanged:',
            tier1.domChanged,
            'network:',
            tier1.networkRequest
          );
        }
      }

      // --- Tier 2: element.click() (only if tier 1 didn't detect effect) ---
      if (clickMethod === 'none') {
        // Check if tier 1's effect arrived just after timeout (late detection)
        if (watcher.changed) {
          clickMethod = 'trusted_event_delayed';
          domChanged = watcher.domChanged;
          networkRequestMade = watcher.networkRequest;
          console.log('[NevofluxChild.click] Tier 1 late effect detected, skipping tier 2');
        } else {
          console.log('[NevofluxChild.click] Tier 2: targetEl.click()');
          targetEl.scrollIntoView({ behavior: 'instant', block: 'center' });
          await this.sleep(50);
          if (typeof targetEl.click === 'function') {
            for (let i = 0; i < clickCount; i++) {
              targetEl.click();
              if (delay > 0 && i < clickCount - 1) await this.sleep(delay);
            }
          }

          const tier2 = await watcher.waitForEffect(500);
          if (tier2.changed) {
            clickMethod = 'native_click';
            domChanged = tier2.domChanged;
            networkRequestMade = tier2.networkRequest;
            console.log(
              '[NevofluxChild.click] Tier 2 effective - domChanged:',
              tier2.domChanged,
              'network:',
              tier2.networkRequest
            );
          }
        }
      }

      // --- Tier 3: synthetic dispatchEvent (last resort) ---
      if (clickMethod === 'none') {
        // Check for late detection again
        if (watcher.changed) {
          clickMethod = 'native_click_delayed';
          domChanged = watcher.domChanged;
          networkRequestMade = watcher.networkRequest;
          console.log('[NevofluxChild.click] Tier 2 late effect detected, skipping tier 3');
        } else {
          console.log('[NevofluxChild.click] Tier 3: synthetic dispatchEvent');
          this._dispatchMouseEvents(targetEl, clickPoint.x, clickPoint.y, buttonCode, win);

          // Also try pointer-events:none and element-at-point fallbacks in tier 3
          if (el !== targetEl) {
            console.log(
              '[NevofluxChild.click] Also clicking original element (pointer-events workaround)'
            );
            if (typeof el.click === 'function') el.click();
            const elRect = el.getBoundingClientRect();
            this._dispatchMouseEvents(
              el,
              elRect.left + elRect.width / 2,
              elRect.top + elRect.height / 2,
              buttonCode,
              win
            );
          }
          if (!isTargetAtPoint && elementAtPoint && elementAtPoint !== targetEl) {
            console.log('[NevofluxChild.click] Clicking element at point:', elementAtPoint.tagName);
            if (typeof elementAtPoint.click === 'function') elementAtPoint.click();
            this._dispatchMouseEvents(elementAtPoint, clickPoint.x, clickPoint.y, buttonCode, win);
          }

          const tier3 = await watcher.waitForEffect(300);
          clickMethod = tier3.changed ? 'synthetic' : 'all_tiers_exhausted';
          domChanged = tier3.domChanged;
          networkRequestMade = tier3.networkRequest;
        }
      }

      watcher.disconnect();
    } catch (e) {
      console.error('[NevofluxChild.click] Error:', e.message, e.stack);
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    // 9. Determine results
    const elementRemoved = !(doc.body?.contains(el) ?? false);
    const clickEffective = domChanged || networkRequestMade || elementRemoved;
    console.log(
      '[NevofluxChild.click] Complete - method:',
      clickMethod,
      'domChanged:',
      domChanged,
      'network:',
      networkRequestMade,
      'removed:',
      elementRemoved,
      'effective:',
      clickEffective
    );

    return {
      success: true,
      effective: clickEffective,
      clickMethod,
      domChanged,
      networkRequestMade,
      elementRemoved,
    };
  }

  type({ selector, text }) {
    console.log(
      '[NevofluxChild.type] Starting type, selector:',
      selector,
      'text length:',
      text?.length
    );
    const doc = this.currentDoc;
    const win = this.currentWin;
    if (!doc || !win) {
      console.log('[NevofluxChild.type] No doc/win available');
      return {
        success: false,
        error: { code: 5001, message: 'No document/window available', recoverable: false },
      };
    }

    const el = doc.querySelector(selector);
    console.log('[NevofluxChild.type] Element found:', !!el, 'tagName:', el?.tagName);
    if (!el) {
      return {
        success: false,
        error: { code: 1001, message: 'Element not found', recoverable: true },
      };
    }

    try {
      // Focus the element first
      el.focus();
      console.log('[NevofluxChild.type] Focused element');

      // Try to use windowUtils for real keyboard simulation
      const domUtils = win.windowUtils;
      console.log(
        '[NevofluxChild.type] domUtils available:',
        !!domUtils,
        'sendKeyEvent:',
        typeof domUtils?.sendKeyEvent
      );

      if (domUtils && typeof domUtils.sendKeyEvent === 'function') {
        // Use Firefox's privileged API for real keyboard events
        console.log('[NevofluxChild.type] Using windowUtils.sendKeyEvent');
        for (const char of text) {
          const charCode = char.charCodeAt(0);
          // sendKeyEvent(type, keyCode, charCode, modifiers, aAdditionalFlags)
          // keyCode=0 means use charCode, modifiers=0 means no modifiers
          domUtils.sendKeyEvent('keydown', 0, charCode, 0);
          domUtils.sendKeyEvent('keypress', 0, charCode, 0);
          domUtils.sendKeyEvent('keyup', 0, charCode, 0);
        }
        console.log('[NevofluxChild.type] Finished typing via sendKeyEvent');
      } else {
        // Fallback: direct value manipulation
        console.log('[NevofluxChild.type] Using fallback value manipulation');
        for (const char of text) {
          el.value += char;
          el.dispatchEvent(new Event('input', { bubbles: true }));
        }
        console.log('[NevofluxChild.type] Finished typing via value manipulation');
      }
    } catch (e) {
      console.error('[NevofluxChild.type] Error:', e.message, e.stack);
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }

    return { success: true };
  }

  fill({ selector, text }) {
    const doc = this.currentDoc;
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
      el.value = text;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    } catch (e) {
      return { success: false, error: { code: 5001, message: e.message, recoverable: false } };
    }

    return { success: true };
  }

  // ========== Wait ==========

  async waitForSelector({ selector, timeout = 30000, state = 'visible' }) {
    if (!this.currentDoc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
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

    return {
      success: false,
      error: { code: 4001, message: `Timeout waiting for ${selector}`, recoverable: true },
    };
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
    } catch (e) {
      /* ignore */
    }

    // Fallback to contentWindow
    try {
      const utils = this.contentWindow?.windowUtils;
      if (utils) {
        return utils;
      }
    } catch (e) {
      /* ignore */
    }

    // Fallback to document.defaultView
    try {
      const utils = this.document?.defaultView?.windowUtils;
      if (utils) {
        return utils;
      }
    } catch (e) {
      /* ignore */
    }

    return null;
  }

  /**
   * Set up observers to detect click effects (DOM changes + network requests).
   * Must be called BEFORE the click action so changes during click are captured.
   * Returns a watcher with waitForEffect(timeout) and disconnect().
   */
  _setupClickEffectWatcher(doc, win) {
    let domChanged = false;
    let networkRequest = false;
    let resolveWait = null;

    const notifyChange = () => {
      if (resolveWait) {
        const fn = resolveWait;
        resolveWait = null;
        fn();
      }
    };

    // MutationObserver with noise filtering
    const observer = new win.MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (
          mutation.type === 'childList' &&
          (mutation.addedNodes.length > 0 || mutation.removedNodes.length > 0)
        ) {
          // Filter trivial injections (analytics scripts, tracking pixels)
          let meaningful = false;
          for (const node of mutation.addedNodes) {
            if (node.nodeType === 1) {
              const tag = node.tagName?.toLowerCase();
              if (tag === 'script' || tag === 'link' || tag === 'style') continue;
              if (tag === 'img' && node.width <= 1 && node.height <= 1) continue;
              meaningful = true;
              break;
            }
            if (node.nodeType === 3 && node.textContent?.trim()) {
              meaningful = true;
              break;
            }
          }
          if (!meaningful) {
            for (const node of mutation.removedNodes) {
              if (node.nodeType === 1) {
                meaningful = true;
                break;
              }
            }
          }
          if (meaningful) {
            domChanged = true;
            notifyChange();
            return;
          }
        }
        if (mutation.type === 'attributes') {
          const attr = mutation.attributeName;
          if (attr === 'style') continue;
          if (attr === 'class') {
            const oldVal = mutation.oldValue || '';
            const newVal = mutation.target.className || '';
            const hoverClasses = /\b(hover|active|focus|focused|pressed|highlighted)\b/gi;
            const oldClean = oldVal.replace(hoverClasses, '').trim();
            const newClean = newVal.replace(hoverClasses, '').trim();
            if (oldClean !== newClean) {
              domChanged = true;
              notifyChange();
              return;
            }
          } else {
            // Meaningful attribute: hidden, disabled, aria-*, data-state
            domChanged = true;
            notifyChange();
            return;
          }
        }
      }
    });

    observer.observe(doc.body || doc.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeOldValue: true,
      attributeFilter: [
        'class',
        'style',
        'hidden',
        'disabled',
        'aria-hidden',
        'aria-expanded',
        'aria-selected',
        'data-state',
        'data-active',
      ],
    });

    // PerformanceObserver for network requests triggered by click
    let perfObserver = null;
    try {
      perfObserver = new win.PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (entry.initiatorType === 'fetch' || entry.initiatorType === 'xmlhttprequest') {
            networkRequest = true;
            notifyChange();
            return;
          }
        }
      });
      perfObserver.observe({ entryTypes: ['resource'] });
    } catch (e) {
      // PerformanceObserver not available in all contexts
    }

    return {
      get changed() {
        return domChanged || networkRequest;
      },
      get domChanged() {
        return domChanged;
      },
      get networkRequest() {
        return networkRequest;
      },

      waitForEffect(timeout = 500) {
        if (domChanged || networkRequest) {
          return Promise.resolve({ changed: true, domChanged, networkRequest });
        }
        return new Promise((resolve) => {
          const done = () => {
            resolve({ changed: domChanged || networkRequest, domChanged, networkRequest });
          };
          resolveWait = done;
          win.setTimeout(() => {
            if (resolveWait === done) {
              resolveWait = null;
              done();
            }
          }, timeout);
        });
      },

      disconnect() {
        observer.disconnect();
        if (perfObserver) perfObserver.disconnect();
        resolveWait = null;
      },
    };
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
    el.dispatchEvent(new MouseEvent('mouseenter', { ...eventInit, bubbles: false }));
    el.dispatchEvent(new MouseEvent('mouseover', eventInit));
    el.dispatchEvent(new MouseEvent('mousemove', eventInit));
    el.dispatchEvent(new MouseEvent('mousedown', eventInit));
    el.dispatchEvent(new MouseEvent('mouseup', eventInit));
    el.dispatchEvent(new MouseEvent('click', eventInit));
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
    const interactiveTags = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'];
    const hasClickHandler = el.onclick !== null;
    const role = el.getAttribute('role');
    const hasRole = [
      'button',
      'link',
      'textbox',
      'checkbox',
      'radio',
      'combobox',
      'menuitem',
      'tab',
    ].includes(role);
    const isTabFocusable = el.getAttribute('tabindex') !== null;

    if (interactiveTags.includes(el.tagName) || hasClickHandler || hasRole || isTabFocusable) {
      return true;
    }

    // Check if any descendant is interactive
    if (checkDescendants) {
      try {
        const descendants = el.querySelectorAll('*');
        for (const desc of descendants) {
          if (this.isInteractive(desc)) {
            return true;
          }
        }
      } catch (e) {
        /* ignore */
      }
    }

    return false;
  }

  /**
   * Check if element is explicitly an interactive element (not just cursor:pointer)
   */
  _isExplicitlyInteractive(el) {
    if (!el) return false;

    const interactiveTags = ['BUTTON', 'A', 'INPUT', 'SELECT', 'TEXTAREA'];
    if (interactiveTags.includes(el.tagName)) return true;

    const role = el.getAttribute('role');
    if (['button', 'link', 'menuitem', 'tab', 'checkbox', 'radio', 'switch'].includes(role))
      return true;

    if (el.onclick !== null || el.hasAttribute('onclick')) return true;

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
      if (typeof InspectorUtils !== 'undefined') {
        return InspectorUtils;
      }
    } catch (e) {
      /* ignore */
    }

    // Try 2: From content window's defaultView
    try {
      const win = this.contentWindow || this.document?.defaultView;
      if (win?.InspectorUtils) {
        return win.InspectorUtils;
      }
    } catch (e) {
      /* ignore */
    }

    // Try 3: From Cu (Components.utils)
    try {
      if (typeof Cu !== 'undefined' && Cu.getGlobalForObject) {
        const global = Cu.getGlobalForObject(Cu);
        if (global?.InspectorUtils) {
          return global.InspectorUtils;
        }
      }
    } catch (e) {
      /* ignore */
    }

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
      if (inspectorUtils && typeof inspectorUtils.getEventListenerInfoFor === 'function') {
        // Use wrappedJSObject to get the actual content element for InspectorUtils
        const unwrappedEl = el.wrappedJSObject || el;
        const listeners = inspectorUtils.getEventListenerInfoFor(unwrappedEl);
        if (listeners && listeners.length > 0) {
          // Check for click-related events
          const clickEvents = [
            'click',
            'mousedown',
            'mouseup',
            'pointerdown',
            'pointerup',
            'touchstart',
            'touchend',
          ];
          for (const listener of listeners) {
            if (clickEvents.includes(listener.type)) {
              console.log(
                '[_hasClickEventListener] InspectorUtils found click listener:',
                el.tagName,
                el.className
              );
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
        if (key.startsWith('__reactProps$')) {
          try {
            const props = unwrapped[key];
            if (props && (props.onClick || props.onMouseDown || props.onPointerDown)) {
              console.log(
                '[_hasClickEventListener] Found React onClick:',
                el.tagName,
                el.className
              );
              return true;
            }
          } catch (e) {
            /* ignore cross-origin access errors */
          }
        }
      }

      // Vue 3.x: Check for _vei (Vue Event Invokers) - this contains actual event handlers
      // Note: __vue__, __vueParentComponent, __vnode are just Vue internals, not event indicators
      if (unwrapped._vei) {
        // _vei is an object like { onClick: handler, onMousedown: handler, ... }
        const vei = unwrapped._vei;
        if (
          vei.onClick ||
          vei.onMousedown ||
          vei.onPointerdown ||
          vei.onclick ||
          vei.onmousedown ||
          vei.onpointerdown
        ) {
          console.log(
            '[_hasClickEventListener] Found Vue _vei click handler:',
            el.tagName,
            el.className
          );
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
      console.log(
        '[_findClickableDescendant] Checking',
        directChildren.length,
        'direct children first'
      );

      for (const child of directChildren) {
        // Use isInteractive for snapshot consistency
        if (this.isInteractive(child)) {
          console.log(
            '[_findClickableDescendant] Found interactive direct child:',
            child.tagName,
            child.className
          );
          return child;
        }
      }
    } catch (e) {
      /* ignore */
    }

    // PRIORITY 2: Use CSS selectors matching isInteractive criteria
    // These selectors match the same elements isInteractive would return true for
    const interactiveSelectors = [
      'a', // interactiveTags: A
      'button', // interactiveTags: BUTTON
      'input', // interactiveTags: INPUT
      'select', // interactiveTags: SELECT
      'textarea', // interactiveTags: TEXTAREA
      '[onclick]', // hasClickHandler
      "[role='button']", // hasRole
      "[role='link']", // hasRole
      "[role='textbox']", // hasRole
      "[role='checkbox']", // hasRole
      "[role='radio']", // hasRole
      "[role='combobox']", // hasRole
      "[role='menuitem']", // hasRole
      "[role='tab']", // hasRole
      '[tabindex]', // isTabFocusable
    ];

    for (const selector of interactiveSelectors) {
      try {
        const child = el.querySelector(selector);
        if (child && this.isInteractive(child)) {
          console.log(
            '[_findClickableDescendant] Found interactive element via selector:',
            selector,
            child.tagName,
            child.className
          );
          return child;
        }
      } catch (e) {
        /* ignore selector errors */
      }
    }

    // PRIORITY 3: Search all descendants for any interactive element
    try {
      const allDescendants = el.querySelectorAll('*');
      console.log(
        '[_findClickableDescendant] Checking',
        allDescendants.length,
        'descendants for interactivity'
      );

      for (const child of allDescendants) {
        if (this.isInteractive(child)) {
          console.log(
            '[_findClickableDescendant] Found interactive descendant:',
            child.tagName,
            child.className
          );
          return child;
        }
      }
      console.log('[_findClickableDescendant] No interactive elements found in descendants');
    } catch (e) {
      /* ignore */
    }

    return null;
  }

  /**
   * Calculate multiple potential click points for an element
   * Used when center point might be obscured
   */
  _getClickPoints(rect) {
    const margin = 5; // Safety margin from edges
    return [
      { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }, // Center
      { x: rect.left + margin, y: rect.top + margin }, // Top-left
      { x: rect.right - margin, y: rect.top + margin }, // Top-right
      { x: rect.left + margin, y: rect.bottom - margin }, // Bottom-left
      { x: rect.right - margin, y: rect.bottom - margin }, // Bottom-right
      { x: rect.left + rect.width / 2, y: rect.top + margin }, // Top-center
      { x: rect.left + rect.width / 2, y: rect.bottom - margin }, // Bottom-center
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
      } catch (e) {
        /* ignore */
      }
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
        console.log(
          '[_findClickableAncestor] Found ancestor with click listener:',
          current.tagName,
          current.className
        );
        return current;
      }

      // Priority 2: Check for various click indicators (attributes/tags)
      if (
        current.onclick ||
        current.hasAttribute('onclick') ||
        current.hasAttribute('data-click') ||
        current.hasAttribute('data-action') ||
        current.tagName === 'A' ||
        current.tagName === 'BUTTON' ||
        current.getAttribute('role') === 'button' ||
        current.getAttribute('role') === 'link' ||
        current.getAttribute('role') === 'menuitem'
      ) {
        return current;
      }

      // Priority 3: Check for cursor:pointer (common for event delegation)
      try {
        const style = win.getComputedStyle(current);
        if (style.cursor === 'pointer') {
          return current;
        }
      } catch (e) {
        /* ignore */
      }

      current = current.parentElement;
      depth++;
    }

    return null;
  }

  async keyPress({ key, modifiers = [], delay = 0 }) {
    console.log('[NevofluxChild.keyPress] key:', key, 'modifiers:', modifiers);
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
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
        ctrlKey: modifiers.includes('ctrl'),
        altKey: modifiers.includes('alt'),
        shiftKey: modifiers.includes('shift'),
        metaKey: modifiers.includes('meta'),
      };

      // For special keys like Enter, use simpler event sequence to avoid crashes
      // during form submission/navigation
      const isSpecialKey = ['Enter', 'Tab', 'Escape'].includes(key);

      if (isSpecialKey) {
        // Just dispatch keydown - most handlers respond to keydown for special keys
        console.log('[NevofluxChild.keyPress] Special key, dispatching single keydown event');
        target.dispatchEvent(new win.KeyboardEvent('keydown', eventInit));
        // Small delay to let event handlers run before we return
        // Use content window's setTimeout (available in content context)
        if (win.setTimeout) {
          await new Promise((r) => win.setTimeout(r, 10));
        }
      } else {
        // Full key sequence for regular keys
        target.dispatchEvent(new win.KeyboardEvent('keydown', eventInit));
        if (delay > 0) await this.sleep(delay);
        target.dispatchEvent(new win.KeyboardEvent('keypress', eventInit));
        if (delay > 0) await this.sleep(delay);
        target.dispatchEvent(new win.KeyboardEvent('keyup', eventInit));
      }

      return { success: true };
    } catch (e) {
      console.error('[NevofluxChild.keyPress] Error:', e.message);
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
      const domUtils = this._getWindowUtils();
      if (!domUtils || typeof domUtils.sendKeyEvent !== 'function') {
        // Fallback to DOM events
        return this._keyDownFallback(key, modifiers);
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
      const domUtils = this._getWindowUtils();
      if (!domUtils || typeof domUtils.sendKeyEvent !== 'function') {
        // Fallback to DOM events
        return this._keyUpFallback(key, modifiers);
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

  // Fallback keyboard implementation using DOM events (when sendKeyEvent is not available)
  async _keyPressFallback(key, modifiers = [], delay = 0) {
    const win = this.document?.defaultView || this.contentWindow;
    const doc = this.doc;
    if (!win || !doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No window/document available', recoverable: false },
      };
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
        ctrlKey: modifiers.includes('ctrl'),
        altKey: modifiers.includes('alt'),
        shiftKey: modifiers.includes('shift'),
        metaKey: modifiers.includes('meta'),
      };

      target.dispatchEvent(new win.KeyboardEvent('keydown', eventInit));
      if (delay > 0) await this.sleep(delay);
      target.dispatchEvent(new win.KeyboardEvent('keypress', eventInit));
      if (delay > 0) await this.sleep(delay);
      target.dispatchEvent(new win.KeyboardEvent('keyup', eventInit));

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
      return {
        success: false,
        error: { code: 5001, message: 'No window/document available', recoverable: false },
      };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      target.dispatchEvent(
        new win.KeyboardEvent('keydown', {
          key: key,
          code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
          keyCode: keyCode,
          which: keyCode,
          bubbles: true,
          cancelable: true,
          ctrlKey: modifiers.includes('ctrl'),
          altKey: modifiers.includes('alt'),
          shiftKey: modifiers.includes('shift'),
          metaKey: modifiers.includes('meta'),
        })
      );

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
      return {
        success: false,
        error: { code: 5001, message: 'No window/document available', recoverable: false },
      };
    }

    try {
      const target = doc.activeElement || doc.body;
      const keyCode = this._getKeyCode(key);

      target.dispatchEvent(
        new win.KeyboardEvent('keyup', {
          key: key,
          code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
          keyCode: keyCode,
          which: keyCode,
          bubbles: true,
          cancelable: true,
          ctrlKey: modifiers.includes('ctrl'),
          altKey: modifiers.includes('alt'),
          shiftKey: modifiers.includes('shift'),
          metaKey: modifiers.includes('meta'),
        })
      );

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

  // ========== Mouse Control ==========

  mouseMove({ x, y, steps: _steps = 1 }) {
    const win = this.document?.defaultView || this.contentWindow;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const domUtils = this._getWindowUtils();
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
      const domUtils = this._getWindowUtils();
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
      const domUtils = this._getWindowUtils();
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
      const domUtils = this._getWindowUtils();
      if (domUtils && typeof domUtils.sendWheelEvent === 'function') {
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

  scroll({ direction = 'down', amount = 'page' }) {
    const win = this.currentWin;
    if (!win) {
      return {
        success: false,
        error: { code: 5001, message: 'No window available', recoverable: false },
      };
    }

    try {
      const doc = this.currentDoc;

      // ── Modal-aware scroll: find the scrollable modal container ──
      let scrollTarget = null;
      try {
        const modalNodes = doc.querySelectorAll(
          '[role="dialog"], [role="alertdialog"], [aria-modal="true"]'
        );
        for (const modal of modalNodes) {
          const rect = modal.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            // Find the scrollable element within the modal
            scrollTarget = this._findScrollableChild(modal, win);
            if (scrollTarget) break;
          }
        }
      } catch {}

      let scrollPx;
      if (amount === 'page') {
        scrollPx = (scrollTarget ? scrollTarget.clientHeight : win.innerHeight) * 0.85;
      } else if (amount === 'half') {
        scrollPx = (scrollTarget ? scrollTarget.clientHeight : win.innerHeight) * 0.5;
      } else {
        const parsed = parseInt(amount, 10);
        scrollPx = isNaN(parsed) ? win.innerHeight : parsed;
      }

      if (direction === 'up') scrollPx = -scrollPx;

      if (scrollTarget) {
        scrollTarget.scrollBy({ top: scrollPx, behavior: 'instant' });
        return {
          success: true,
          scrollTarget: 'modal',
          scrollTop: Math.round(scrollTarget.scrollTop),
          scrollHeight: Math.round(scrollTarget.scrollHeight),
          viewportHeight: scrollTarget.clientHeight,
        };
      }

      win.scrollBy({ top: scrollPx, behavior: 'instant' });

      return {
        success: true,
        scrollTop: Math.round(win.scrollY || 0),
        scrollHeight: Math.round(doc?.documentElement?.scrollHeight || 0),
        viewportHeight: win.innerHeight,
      };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  /**
   * Find the scrollable element within a modal dialog.
   * Checks the modal itself and its descendants for overflow scroll/auto.
   */
  _findScrollableChild(modal, win) {
    // Check the modal itself first
    try {
      const cs = win.getComputedStyle(modal);
      if (
        (cs.overflowY === 'auto' || cs.overflowY === 'scroll') &&
        modal.scrollHeight > modal.clientHeight
      ) {
        return modal;
      }
    } catch {}

    // BFS through children to find the scrollable container
    const queue = [...modal.children];
    while (queue.length > 0) {
      const node = queue.shift();
      if (!node || node.nodeType !== 1) continue;
      try {
        const cs = win.getComputedStyle(node);
        if (
          (cs.overflowY === 'auto' || cs.overflowY === 'scroll') &&
          node.scrollHeight > node.clientHeight
        ) {
          return node;
        }
      } catch {}
      for (const child of node.children) {
        queue.push(child);
      }
    }

    return null;
  }

  async waitForStable({ strategy = 'interaction', maxWait = 3000 }) {
    const startTime = Date.now();

    if (strategy === 'scroll') {
      // Scroll: short fixed wait
      await this.sleep(200);
      return { stable: true, strategy, duration_ms: Date.now() - startTime };
    }

    if (strategy === 'navigation') {
      // Navigation: wait for load event, up to maxWait
      const doc = this.currentDoc;
      if (doc && doc.readyState !== 'complete') {
        await new Promise((resolve) => {
          const win = this.currentWin;
          const timeout = win?.setTimeout?.(() => resolve(), maxWait) || null;
          const onLoad = () => {
            if (timeout && win?.clearTimeout) win.clearTimeout(timeout);
            resolve();
          };
          if (win) {
            win.addEventListener('load', onLoad, { once: true });
          } else {
            resolve();
          }
        });
      }
      // Extra 300ms for post-load scripts
      await this.sleep(300);
      return { stable: true, strategy, duration_ms: Date.now() - startTime };
    }

    // Default: interaction strategy
    // 300ms baseline
    await this.sleep(300);

    // Then monitor for DOM stability using MutationObserver
    const doc = this.currentDoc;
    if (doc?.body) {
      const remaining = maxWait - (Date.now() - startTime);
      if (remaining > 0) {
        await new Promise((resolve) => {
          let lastMutationTime = Date.now();
          const quiescenceMs = 200; // Consider stable after 200ms of no mutations

          const observer = new doc.defaultView.MutationObserver(() => {
            lastMutationTime = Date.now();
          });

          observer.observe(doc.body, {
            childList: true,
            subtree: true,
            attributes: true,
            attributeFilter: [
              'class',
              'style',
              'hidden',
              'disabled',
              'aria-expanded',
              'aria-hidden',
            ],
          });

          const checkStable = () => {
            const elapsed = Date.now() - startTime;
            if (elapsed >= maxWait) {
              observer.disconnect();
              resolve();
              return;
            }
            if (Date.now() - lastMutationTime >= quiescenceMs) {
              observer.disconnect();
              resolve();
              return;
            }
            const win = this.currentWin || doc.defaultView;
            if (win?.setTimeout) {
              win.setTimeout(checkStable, 50);
            } else {
              resolve();
            }
          };

          const win = this.currentWin || doc.defaultView;
          if (win?.setTimeout) {
            win.setTimeout(checkStable, quiescenceMs);
          } else {
            observer.disconnect();
            resolve();
          }
        });
      }
    }

    return { stable: true, strategy, duration_ms: Date.now() - startTime };
  }

  async dblclick({ selector, button = 'left', delay = 0 }) {
    return this.click({ selector, button, clickCount: 2, delay });
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
        new MouseEvent('mousedown', {
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
          new MouseEvent('mousemove', {
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
        new MouseEvent('mouseup', {
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
    const doc = this.currentDoc;
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
    const doc = this.currentDoc;
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
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 5001, message: String(e), recoverable: false } };
    }
  }

  // ========== Helpers ==========

  sleep(ms) {
    // In JSWindowActorChild, use content window's setTimeout via document.defaultView
    // or fall back to a busy-wait as last resort
    return new Promise((resolve) => {
      const win = this.document?.defaultView || this.contentWindow;
      if (win && typeof win.setTimeout === 'function') {
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
      A: 'link',
      BUTTON: 'button',
      INPUT: 'textbox',
      SELECT: 'combobox',
      TEXTAREA: 'textbox',
      IMG: 'image',
      H1: 'heading',
      H2: 'heading',
      H3: 'heading',
      H4: 'heading',
      H5: 'heading',
      H6: 'heading',
      NAV: 'navigation',
      MAIN: 'main',
      ASIDE: 'complementary',
      FOOTER: 'contentinfo',
      HEADER: 'banner',
      FORM: 'form',
      TABLE: 'table',
      UL: 'list',
      OL: 'list',
      LI: 'listitem',
    };
    return el.getAttribute('role') || roleMap[el.tagName] || 'generic';
  }

  getAccessibleName(el) {
    // 1. Check explicit accessibility attributes first
    const ariaLabel = el.getAttribute('aria-label');
    if (ariaLabel) return ariaLabel;

    // 2. Check aria-labelledby
    const labelledBy = el.getAttribute('aria-labelledby');
    if (labelledBy) {
      const labelEl = el.ownerDocument?.getElementById(labelledBy);
      if (labelEl?.textContent?.trim()) {
        return labelEl.textContent.trim().slice(0, 50);
      }
    }

    // 3. Check title attribute (both getAttribute and property for Vue/React compatibility)
    const titleAttr = el.getAttribute('title');
    if (titleAttr) return titleAttr;
    if (el.title) return el.title;

    // 4. Check alt (for images)
    const alt = el.getAttribute('alt');
    if (alt) return alt;

    // 5. Check placeholder for inputs/textareas
    const tagName = el.tagName?.toUpperCase();
    if (tagName === 'INPUT' || tagName === 'TEXTAREA') {
      const placeholder = el.getAttribute('placeholder') || el.placeholder;
      if (placeholder) return placeholder;
    }

    // 6. Check value for buttons and inputs
    if (tagName === 'INPUT') {
      const inputType = el.type?.toLowerCase();
      if (inputType === 'submit' || inputType === 'button' || inputType === 'reset') {
        const value = el.value;
        if (value) return value;
      }
    }

    // 7. Fallback to direct text content (excluding nested element text)
    // This gets only immediate text children, not deeply nested text
    let directText = '';
    for (const node of el.childNodes) {
      if (node.nodeType === 3) {
        // Text node
        directText += node.textContent || '';
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
    const interactiveTags = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'];
    const hasClickHandler = el.onclick !== null;
    const hasRole = ['button', 'link', 'textbox', 'checkbox', 'radio', 'combobox'].includes(
      el.getAttribute('role')
    );
    const isTabFocusable = el.getAttribute('tabindex') !== null;

    return interactiveTags.includes(el.tagName) || hasClickHandler || hasRole || isTabFocusable;
  }

  hasContent(el) {
    return el.textContent?.trim().length > 0 || el.querySelector('img, video, canvas, svg');
  }

  generateSelector(el) {
    if (el.id) {
      return `#${CSS.escape(el.id)}`;
    }

    const path = [];
    let current = el;

    while (current && current !== this.document.body) {
      let selector = current.tagName.toLowerCase();

      if (current.className && typeof current.className === 'string') {
        const classes = current.className.trim().split(/\s+/).slice(0, 2);
        if (classes.length > 0 && classes[0]) {
          selector += `.${classes.map((c) => CSS.escape(c)).join('.')}`;
        }
      }

      const siblings = current.parentElement?.children || [];
      const sameTagSiblings = Array.from(siblings).filter((s) => s.tagName === current.tagName);
      if (sameTagSiblings.length > 1) {
        const index = sameTagSiblings.indexOf(current) + 1;
        selector += `:nth-of-type(${index})`;
      }

      path.unshift(selector);
      current = current.parentElement;
    }

    return path.join(' > ');
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

  // ========== Frame Management ==========

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
    const doc = this.doc; // Always search from main document
    if (!doc) {
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
    }

    // If already in a frame, search within that frame
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
      // Test if we can access the frame's content
      const frameDoc = iframe.contentDocument;
      if (!frameDoc) {
        return {
          success: false,
          error: { code: 10002, message: 'Frame access denied (cross-origin)', recoverable: false },
        };
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

  // ========== JavaScript Execution ==========

  // Note: Timeout is not implemented for eval. Implementing true timeout for synchronous
  // eval is complex and would require running in a Worker or using async patterns.
  // The script executes synchronously in the page context.
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
          type = 'undefined';
        } else if (result === null) {
          serialized = null;
          type = 'null';
        } else {
          serialized = JSON.parse(JSON.stringify(result));
        }
      } catch {
        // Can't serialize, return string representation
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
      // Use timestamp + random suffix to ensure unique handles
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
      return {
        success: false,
        error: { code: 5001, message: 'No document available', recoverable: false },
      };
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
          'nav',
          'header',
          'footer',
          '[role="navigation"]',
          '[role="banner"]',
          '[role="contentinfo"]',
          '[aria-hidden="true"]',
          '[hidden]',
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
      const lazyImages = docClone.querySelectorAll(
        'img[data-src], img[data-original], img[data-lazy-src], img[data-actualsrc]'
      );
      for (const img of lazyImages) {
        const lazySrc =
          img.getAttribute('data-src') ||
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
          return {
            success: false,
            error: { code: 1001, message: `Selector "${selector}" not found`, recoverable: true },
          };
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
          replacement: function () {
            return '';
          },
        });
      }

      // Handle links
      if (!includeLinks) {
        turndownService.addRule('plainLinks', {
          filter: 'a',
          replacement: function (content) {
            return content;
          },
        });
      }

      // Convert to markdown
      const markdown = turndownService.turndown(contentEl);

      // Clean up excessive whitespace
      const cleanedMarkdown = markdown
        .replace(/\n{3,}/g, '\n\n') // Max 2 consecutive newlines
        .replace(/[ \t]+$/gm, '') // Trim trailing whitespace per line
        .trim();

      return {
        success: true,
        markdown: cleanedMarkdown,
        title: doc.title || '',
        url: doc.location?.href || '',
      };
    } catch (e) {
      console.error('[NevofluxChild.getMarkdown] Error:', e);
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
    const skipTags = ['SCRIPT', 'STYLE', 'NOSCRIPT', 'SVG', 'IFRAME', 'TEMPLATE'];
    if (skipTags.includes(tagName)) return true;

    if (!filterNonContent) return false;

    // Skip navigation and structural elements
    const skipStructural = ['NAV', 'HEADER', 'FOOTER', 'ASIDE'];
    if (skipStructural.includes(tagName)) return true;

    // Skip by role
    const role = el.getAttribute('role');
    const skipRoles = ['navigation', 'banner', 'contentinfo', 'complementary', 'search'];
    if (role && skipRoles.includes(role)) return true;

    // Skip by common ad/non-content class names
    const className = el.className?.toString().toLowerCase() || '';
    const id = el.id?.toLowerCase() || '';
    const skipPatterns = [
      'nav',
      'menu',
      'sidebar',
      'widget',
      'ad',
      'ads',
      'advertisement',
      'banner',
      'promo',
      'social',
      'share',
      'comment',
      'related',
      'footer',
      'header',
      'breadcrumb',
      'pagination',
      'toc',
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
    return this._cleanMarkdown(lines.join('\n'));
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
      case 'H1':
      case 'H2':
      case 'H3':
      case 'H4':
      case 'H5':
      case 'H6': {
        const level = parseInt(tagName.charAt(1), 10);
        const text = el.textContent?.trim();
        if (text) {
          lines.push('');
          lines.push('#'.repeat(level) + ' ' + text);
          lines.push('');
        }
        return;
      }

      case 'P': {
        const content = this._getInlineContent(el, options);
        if (content.trim()) {
          lines.push('');
          lines.push(content);
          lines.push('');
        }
        return;
      }

      case 'A': {
        if (options.includeLinks) {
          const text = el.textContent?.trim();
          const href = el.href; // DOM automatically provides absolute URL
          if (text && href) {
            lines.push(`[${text}](${href})`);
          } else if (text) {
            lines.push(text);
          }
        } else {
          lines.push(el.textContent?.trim() || '');
        }
        return;
      }

      case 'IMG': {
        if (options.includeImages) {
          const src = el.src; // DOM automatically provides absolute URL
          if (src) {
            // Skip base64 images unless explicitly included
            if (src.startsWith('data:') && !options.includeBase64Images) {
              return;
            }
            const alt = el.alt || el.title || 'image';
            lines.push(`![${alt}](${src})`);
          }
        }
        return;
      }

      case 'UL':
      case 'OL': {
        lines.push('');
        this._processList(el, options, lines, tagName === 'OL', depth);
        lines.push('');
        return;
      }

      case 'LI': {
        const content = this._getInlineContent(el, options);
        if (content.trim()) {
          lines.push(content);
        }
        // Process nested lists
        for (const child of el.children) {
          if (child.tagName === 'UL' || child.tagName === 'OL') {
            this._processList(child, options, lines, child.tagName === 'OL', depth + 1);
          }
        }
        return;
      }

      case 'BLOCKQUOTE': {
        lines.push('');
        const quoteLines = [];
        for (const child of el.childNodes) {
          this._processElement(child, options, quoteLines, depth);
        }
        for (const line of quoteLines) {
          if (line.trim()) {
            lines.push('> ' + line);
          }
        }
        lines.push('');
        return;
      }

      case 'PRE': {
        const codeEl = el.querySelector('code');
        const code = codeEl ? codeEl.textContent : el.textContent;
        const lang = codeEl?.className.match(/language-(\w+)/)?.[1] || '';
        lines.push('');
        lines.push('```' + lang);
        lines.push(code?.trim() || '');
        lines.push('```');
        lines.push('');
        return;
      }

      case 'CODE': {
        // Inline code (not inside PRE)
        if (el.parentElement?.tagName !== 'PRE') {
          const text = el.textContent?.trim();
          if (text) {
            lines.push('`' + text + '`');
          }
        }
        return;
      }

      case 'TABLE': {
        if (options.includeTables) {
          const tableMarkdown = this._convertTable(el);
          if (tableMarkdown) {
            lines.push('');
            lines.push(tableMarkdown);
            lines.push('');
          }
        }
        return;
      }

      case 'HR': {
        lines.push('');
        lines.push('---');
        lines.push('');
        return;
      }

      case 'BR': {
        lines.push('  '); // Two spaces for line break
        return;
      }

      case 'STRONG':
      case 'B': {
        const text = el.textContent?.trim();
        if (text) {
          lines.push('**' + text + '**');
        }
        return;
      }

      case 'EM':
      case 'I': {
        const text = el.textContent?.trim();
        if (text) {
          lines.push('*' + text + '*');
        }
        return;
      }

      case 'DIV':
      case 'SECTION':
      case 'ARTICLE':
      case 'MAIN':
      case 'SPAN': {
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
    const indent = '  '.repeat(depth);
    let counter = 1;

    for (const li of listEl.children) {
      if (li.tagName !== 'LI') continue;

      const prefix = isOrdered ? `${counter}. ` : '- ';
      const content = this._getInlineContent(li, options);

      if (content.trim()) {
        lines.push(indent + prefix + content);
      }

      // Handle nested lists
      for (const child of li.children) {
        if (child.tagName === 'UL' || child.tagName === 'OL') {
          this._processList(child, options, lines, child.tagName === 'OL', depth + 1);
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
        if (
          [
            'UL',
            'OL',
            'DIV',
            'P',
            'TABLE',
            'BLOCKQUOTE',
            'PRE',
            'SCRIPT',
            'STYLE',
            'NOSCRIPT',
            'SVG',
            'IFRAME',
            'TEMPLATE',
            'NAV',
            'HEADER',
            'FOOTER',
            'ASIDE',
          ].includes(tag)
        ) {
          continue;
        }

        switch (tag) {
          case 'A':
            if (options.includeLinks && node.href) {
              const text = node.textContent?.trim();
              if (text) {
                parts.push(`[${text}](${node.href})`);
              }
            } else {
              parts.push(node.textContent || '');
            }
            break;

          case 'STRONG':
          case 'B':
            parts.push('**' + (node.textContent || '') + '**');
            break;

          case 'EM':
          case 'I':
            parts.push('*' + (node.textContent || '') + '*');
            break;

          case 'CODE':
            parts.push('`' + (node.textContent || '') + '`');
            break;

          case 'IMG':
            if (options.includeImages && node.src) {
              // Skip base64 images unless explicitly included
              if (node.src.startsWith('data:') && !options.includeBase64Images) {
                break;
              }
              const alt = node.alt || node.title || 'image';
              parts.push(`![${alt}](${node.src})`);
            }
            break;

          case 'BR':
            parts.push('  \n');
            break;

          default:
            parts.push(node.textContent || '');
        }
      }
    }

    return parts.join('').replace(/\s+/g, ' ').trim();
  }

  /**
   * Convert a table element to Markdown
   */
  _convertTable(tableEl) {
    const rows = [];
    const headerRow = [];
    let hasHeader = false;

    // Process thead
    const thead = tableEl.querySelector('thead');
    if (thead) {
      const headerCells = thead.querySelectorAll('th, td');
      for (const cell of headerCells) {
        headerRow.push(cell.textContent?.trim() || '');
      }
      hasHeader = true;
    }

    // Process tbody
    const tbody = tableEl.querySelector('tbody') || tableEl;
    const dataRows = tbody.querySelectorAll('tr');

    for (let i = 0; i < dataRows.length; i++) {
      const tr = dataRows[i];
      const cells = tr.querySelectorAll('th, td');
      const rowData = [];

      for (const cell of cells) {
        rowData.push(cell.textContent?.trim() || '');
      }

      // If first row is all TH and we don't have header yet
      if (i === 0 && !hasHeader && tr.querySelector('th')) {
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
      return '';
    }

    const colCount = headerRow.length;
    const lines = [];

    // Header row
    lines.push('| ' + headerRow.join(' | ') + ' |');

    // Separator
    lines.push('| ' + headerRow.map(() => '---').join(' | ') + ' |');

    // Data rows
    for (const row of rows) {
      // Pad row if needed
      while (row.length < colCount) row.push('');
      lines.push('| ' + row.slice(0, colCount).join(' | ') + ' |');
    }

    return lines.join('\n');
  }

  /**
   * Clean up the generated Markdown
   */
  _cleanMarkdown(markdown) {
    return (
      markdown
        // Remove any remaining <style>...</style> blocks (including content)
        .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
        // Remove any remaining <script>...</script> blocks (including content)
        .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
        // Remove any remaining <noscript>...</noscript> blocks
        .replace(/<noscript[^>]*>[\s\S]*?<\/noscript>/gi, '')
        // Remove inline style attributes from any remaining HTML tags
        .replace(/\s+style\s*=\s*["'][^"']*["']/gi, '')
        // Remove any remaining HTML tags (but keep content)
        .replace(/<[^>]+>/g, '')
        // Remove excessive blank lines (more than 2)
        .replace(/\n{3,}/g, '\n\n')
        // Clean up whitespace
        .replace(/[ \t]+$/gm, '')
        // Trim
        .trim()
    );
  }

  // ========== Element Picker ==========

  startPicker({ filter = 'any', highlightColor = '#6366f1' }) {
    if (this._pickerActive) {
      return { success: false, error: 'Picker already active' };
    }

    this._pickerActive = true;
    this._pickerFilter = filter;
    this._highlightColor = highlightColor;
    this._pickerResolve = null;
    this._pickerReject = null;

    this._createPickerHighlight();

    this.doc.addEventListener('mousemove', this._onPickerMove, true);
    this.doc.addEventListener('click', this._onPickerClick, true);
    this.doc.addEventListener('keydown', this._onPickerKey, true);

    this._originalCursor = this.doc.body.style.cursor;
    this.doc.body.style.cursor = 'crosshair';

    return new Promise((resolve, reject) => {
      this._pickerResolve = resolve;
      this._pickerReject = reject;
    });
  }

  stopPicker() {
    if (!this._pickerActive) return { success: true };

    this._pickerActive = false;

    this.doc.removeEventListener('mousemove', this._onPickerMove, true);
    this.doc.removeEventListener('click', this._onPickerClick, true);
    this.doc.removeEventListener('keydown', this._onPickerKey, true);

    this.doc.body.style.cursor = this._originalCursor || '';
    this._removePickerHighlight();

    if (this._pickerReject) {
      this._pickerReject({ success: false, error: 'cancelled' });
    }

    return { success: true };
  }

  _createPickerHighlight() {
    if (this._highlightEl) return;

    this._highlightEl = this.doc.createElement('div');
    this._highlightEl.id = 'nevoflux-picker-highlight';
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

    this._labelEl = this.doc.createElement('div');
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
      className: typeof target.className === 'string' ? target.className : null,
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

    if (event.key === 'Escape') {
      event.preventDefault();
      this.stopPicker();
    }
  };

  _updatePickerHighlight(element) {
    if (!this._highlightEl || !element) return;

    const rect = element.getBoundingClientRect();

    this._highlightEl.style.display = 'block';
    this._highlightEl.style.top = `${rect.top}px`;
    this._highlightEl.style.left = `${rect.left}px`;
    this._highlightEl.style.width = `${rect.width}px`;
    this._highlightEl.style.height = `${rect.height}px`;

    const tag = element.tagName.toLowerCase();
    const id = element.id ? `#${element.id}` : '';
    const cls =
      element.className && typeof element.className === 'string'
        ? `.${element.className.split(' ')[0]}`
        : '';
    this._labelEl.textContent = `${tag}${id}${cls}`;
  }

  _generateStableSelector(element) {
    if (!element || element === this.doc.body) return 'body';

    // Priority 1: Unique ID
    if (element.id) {
      const selector = `#${CSS.escape(element.id)}`;
      if (this.doc.querySelectorAll(selector).length === 1) {
        return selector;
      }
    }

    // Priority 2: data-testid or data-* attributes
    for (const attr of element.attributes) {
      if (attr.name === 'data-testid' || attr.name.startsWith('data-')) {
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
        const siblings = Array.from(parent.children).filter((el) => el.tagName === current.tagName);
        if (siblings.length > 1) {
          const index = siblings.indexOf(current) + 1;
          selector += `:nth-of-type(${index})`;
        }
      }

      path.unshift(selector);
      current = current.parentElement;
    }

    return path.join(' > ');
  }

  _generateXPath(element) {
    if (!element) return '';

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

    return '/' + parts.join('/');
  }

  _getPickerElementAttributes(element) {
    const attrs = {};
    for (const attr of element.attributes) {
      if (attr.value.length < 200 && !attr.name.startsWith('on')) {
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
    const container = this.doc.createElement('div');
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

  lockPage({ showOverlay = true, message = '' }) {
    if (this._pageLocked) return { success: true };

    this._pageLocked = true;

    // Event locking
    this._lockHandler = (event) => {
      event.stopImmediatePropagation();
      event.preventDefault();
    };

    const events = [
      'mousedown',
      'mouseup',
      'click',
      'dblclick',
      'contextmenu',
      'keydown',
      'keyup',
      'keypress',
      'touchstart',
      'touchend',
      'touchmove',
      'wheel',
      'scroll',
    ];

    events.forEach((type) => {
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
      this._lockEvents.forEach((type) => {
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
    this._lockOverlay = this.doc.createElement('div');
    this._lockOverlay.id = 'nevoflux-lock-overlay';
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
    const container = this.doc.createElement('div');
    container.style.cssText = `
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 16px;
    `;

    // Create spinner
    const spinner = this.doc.createElement('div');
    spinner.style.cssText = `
      width: 48px;
      height: 48px;
      border: 3px solid rgba(255,255,255,0.3);
      border-top-color: white;
      border-radius: 50%;
      animation: nevoflux-spin 1s linear infinite;
    `;

    // Create message (using textContent to prevent XSS)
    const messageEl = this.doc.createElement('div');
    messageEl.style.cssText = `
      color: white;
      font-size: 14px;
      font-family: system-ui, sans-serif;
    `;
    messageEl.textContent = message || 'Agent working...';

    // Create style for animation
    const style = this.doc.createElement('style');
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

  // ========== NevoFlux Bridge (nevoflux:// pages) ==========

  /**
   * Called by the actor framework when a registered DOM event fires.
   * We use DOMDocElementInserted to inject the NevofluxBridge early,
   * before any page scripts run.
   */
  handleEvent(event) {
    if (event.type === 'DOMDocElementInserted' && this._isNevofluxPage()) {
      this._initBridge();
    }
  }

  /**
   * Inject `window.NevofluxBridge` into the content window of a
   * nevoflux:// page.  The bridge exposes storage and system helpers
   * that communicate with the parent actor via sendQuery / sendAsyncMessage.
   *
   * Security: Cu.exportFunction wraps each callable so the content
   * principal cannot reach chrome internals.  Cu.cloneInto is used for
   * any structured data crossing the boundary.
   */
  _initBridge() {
    const content = this.contentWindow;
    if (!content) {
      return;
    }

    const actor = this; // capture for closures

    // -- storage namespace --------------------------------------------------
    const storage = Cu.cloneInto({}, content);

    Cu.exportFunction(
      function get(key) {
        return actor.sendQuery('contentStore:get', { key });
      },
      storage,
      { defineAs: 'get' }
    );

    Cu.exportFunction(
      function set(key, value) {
        return actor.sendQuery('contentStore:set', { key, value });
      },
      storage,
      { defineAs: 'set' }
    );

    Cu.exportFunction(
      function del(key) {
        return actor.sendQuery('contentStore:delete', { key });
      },
      storage,
      { defineAs: 'delete' }
    );

    Cu.exportFunction(
      function query(prefix) {
        return actor.sendQuery('contentStore:query', { prefix });
      },
      storage,
      { defineAs: 'query' }
    );

    Cu.exportFunction(
      function subscribe(key, callback) {
        // Register with parent so it starts pushing updates for this key
        actor.sendQuery('contentStore:subscribe', { key });

        // Listen for pushed updates and call the content-world callback
        const handler = (evt) => {
          const detail = evt.detail;
          if (detail && detail.key === key) {
            try {
              callback(Cu.cloneInto(detail.value, content));
            } catch (e) {
              // Content callback error — ignore
            }
          }
        };
        content.addEventListener('NevofluxMessage', handler);

        // Return an unsubscribe function
        const unsub = Cu.exportFunction(function unsubscribe() {
          content.removeEventListener('NevofluxMessage', handler);
        }, content);
        return unsub;
      },
      storage,
      { defineAs: 'subscribe' }
    );

    // -- system namespace ---------------------------------------------------
    const system = Cu.cloneInto({}, content);

    Cu.exportFunction(
      function getInfo() {
        const info = {
          platform: Services.appinfo.OS,
          version: Services.appinfo.version,
        };
        return Cu.cloneInto(info, content);
      },
      system,
      { defineAs: 'getInfo' }
    );

    Cu.exportFunction(
      function getConfig(key) {
        return actor
          .sendQuery('contentStore:get', { key: `config:${key}` })
          .then((res) => Cu.cloneInto(res.value, content));
      },
      system,
      { defineAs: 'getConfig' }
    );

    // -- callTool (unified tool execution) -----------------------------------
    const callToolFn = Cu.exportFunction(function callTool(action, params) {
      return actor
        .sendQuery('bridge:request', {
          type: 'exec_tool',
          payload: { action, params: params || {} },
        })
        .then((res) => {
          const val = res.success ? res.data || res : res;
          return Cu.cloneInto(val, content);
        });
    }, content);

    // -- agent namespace ------------------------------------------------------
    const agentNs = Cu.cloneInto({}, content);

    Cu.exportFunction(
      function chat(message, options) {
        return actor
          .sendQuery('bridge:request', {
            type: 'send_to_agent',
            payload: {
              type: 'chat_message',
              payload: { content: message, ...(options || {}) },
            },
          })
          .then((res) => {
            const val = res.success ? res.data || res : res;
            return Cu.cloneInto(val, content);
          });
      },
      agentNs,
      { defineAs: 'chat' }
    );

    Cu.exportFunction(
      function sendCommand(command, params) {
        return actor
          .sendQuery('bridge:request', {
            type: 'send_to_agent',
            payload: {
              type: 'system_command',
              payload: { command, params: params || {} },
            },
          })
          .then((res) => {
            const val = res.success ? res.data || res : res;
            return Cu.cloneInto(val, content);
          });
      },
      agentNs,
      { defineAs: 'sendCommand' }
    );

    // -- sidebar namespace ----------------------------------------------------
    const sidebarNs = Cu.cloneInto({}, content);

    Cu.exportFunction(
      function send(message) {
        return actor
          .sendQuery('bridge:request', {
            type: 'sidebar_send',
            payload: message,
          })
          .then((res) => {
            const val = res.success ? res.data || res : res;
            return Cu.cloneInto(val, content);
          });
      },
      sidebarNs,
      { defineAs: 'send' }
    );

    Cu.exportFunction(
      function notify(notificationType, data) {
        return actor
          .sendQuery('bridge:request', {
            type: 'sidebar_send',
            payload: { type: notificationType, payload: data },
          })
          .then((res) => {
            const val = res.success ? res.data || res : res;
            return Cu.cloneInto(val, content);
          });
      },
      sidebarNs,
      { defineAs: 'notify' }
    );

    // -- assemble bridge object and expose on window ------------------------
    const bridge = Cu.cloneInto({}, content);
    Object.defineProperty(bridge, 'callTool', {
      value: callToolFn,
      writable: false,
      enumerable: true,
      configurable: false,
    });
    Object.defineProperty(bridge, 'storage', {
      value: storage,
      writable: false,
      enumerable: true,
      configurable: false,
    });
    Object.defineProperty(bridge, 'system', {
      value: system,
      writable: false,
      enumerable: true,
      configurable: false,
    });
    Object.defineProperty(bridge, 'agent', {
      value: agentNs,
      writable: false,
      enumerable: true,
      configurable: false,
    });
    Object.defineProperty(bridge, 'sidebar', {
      value: sidebarNs,
      writable: false,
      enumerable: true,
      configurable: false,
    });

    Cu.exportFunction(() => bridge, content, { defineAs: '__getNevofluxBridge' });
    Object.defineProperty(Cu.waiveXrays(content), 'NevofluxBridge', {
      value: bridge,
      writable: false,
      enumerable: true,
      configurable: false,
    });
  }
}
