/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * NevoFlux Agent Content Script
 * Injected into web pages to extract content and perform browser actions
 */

// =============================================================================
// Element Selection Helpers
// =============================================================================

/**
 * Find element by selector
 * @param {string} selector - CSS selector
 * @returns {Element|null}
 */
function findElement(selector) {
  return document.querySelector(selector);
}

/**
 * Find all elements matching selector
 * @param {string} selector - CSS selector
 * @param {number} limit - Maximum number of elements
 * @returns {Element[]}
 */
function findAllElements(selector, limit = 100) {
  const elements = document.querySelectorAll(selector);
  return Array.from(elements).slice(0, limit);
}

/**
 * Wait for an element to appear
 * @param {string} selector - CSS selector
 * @param {number} timeout - Timeout in ms
 * @returns {Promise<Element>}
 */
function waitForElement(selector, timeout = 10000) {
  return new Promise((resolve, reject) => {
    const element = findElement(selector);
    if (element) {
      resolve(element);
      return;
    }

    const observer = new MutationObserver(() => {
      const el = findElement(selector);
      if (el) {
        observer.disconnect();
        resolve(el);
      }
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });

    setTimeout(() => {
      observer.disconnect();
      reject(new Error(`Element not found within ${timeout}ms: ${selector}`));
    }, timeout);
  });
}

/**
 * Get element info
 * @param {Element} element
 * @returns {Object}
 */
function getElementInfo(element) {
  const rect = element.getBoundingClientRect();
  return {
    tag: element.tagName.toLowerCase(),
    id: element.id || null,
    classes: Array.from(element.classList),
    text: element.textContent?.substring(0, 200) || '',
    value: element.value || null,
    href: element.href || null,
    src: element.src || null,
    rect: {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    },
    visible: rect.width > 0 && rect.height > 0,
    attributes: Object.fromEntries(Array.from(element.attributes).map((a) => [a.name, a.value])),
  };
}

// =============================================================================
// Browser Tool Actions
// =============================================================================

/**
 * Click an element with full mouse event simulation
 * @param {Object} params - {selector: string}
 */
function actionClick(params) {
  const { selector } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'Selector required', recoverable: false },
    };
  }

  const element = findElement(selector);
  if (!element) {
    return {
      success: false,
      error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
    };
  }

  // Scroll into view if needed
  element.scrollIntoView({ behavior: 'smooth', block: 'center' });

  // Get element center for realistic click coordinates
  const rect = element.getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;

  const eventOptions = {
    bubbles: true,
    cancelable: true,
    view: window,
    clientX: centerX,
    clientY: centerY,
    screenX: centerX,
    screenY: centerY,
    button: 0,
    buttons: 1,
  };

  // Simulate complete mouse event sequence for better compatibility
  element.dispatchEvent(new MouseEvent('mouseenter', { ...eventOptions, bubbles: false }));
  element.dispatchEvent(new MouseEvent('mouseover', eventOptions));
  element.dispatchEvent(new MouseEvent('mousemove', eventOptions));
  element.dispatchEvent(new MouseEvent('mousedown', eventOptions));
  element.focus?.();
  element.dispatchEvent(new MouseEvent('mouseup', eventOptions));
  element.dispatchEvent(new MouseEvent('click', eventOptions));

  // Also try native click as fallback
  element.click();

  return { success: true, result: { selector, clicked: true } };
}

/**
 * Type text into an element (simulates keystrokes)
 * @param {Object} params - {selector: string, text: string}
 */
function actionType(params) {
  const { selector, text } = params;
  if (!selector || text === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'Selector and text required', recoverable: false },
    };
  }

  const element = findElement(selector);
  if (!element) {
    return {
      success: false,
      error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
    };
  }

  // Scroll into view and focus
  element.scrollIntoView({ behavior: 'instant', block: 'center' });
  element.focus();
  element.dispatchEvent(new FocusEvent('focus', { bubbles: true }));

  // Get native value setter
  const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    'value'
  )?.set;
  const nativeTextAreaValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    'value'
  )?.set;
  const nativeSetter =
    element.tagName === 'TEXTAREA' ? nativeTextAreaValueSetter : nativeInputValueSetter;

  // Type each character with events
  for (const char of text) {
    // Simulate keydown
    element.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: char,
        code: `Key${char.toUpperCase()}`,
        bubbles: true,
      })
    );

    // Update value
    const newValue = element.value + char;
    if (nativeSetter) {
      nativeSetter.call(element, newValue);
    } else {
      element.value = newValue;
    }

    // Dispatch input event
    element.dispatchEvent(
      new InputEvent('input', {
        bubbles: true,
        inputType: 'insertText',
        data: char,
      })
    );

    // Simulate keyup
    element.dispatchEvent(
      new KeyboardEvent('keyup', {
        key: char,
        code: `Key${char.toUpperCase()}`,
        bubbles: true,
      })
    );
  }

  element.dispatchEvent(new Event('change', { bubbles: true }));

  return { success: true, result: { selector, typed: text } };
}

/**
 * Fill a form field (clears existing value first)
 * Uses native events to ensure compatibility with React/Vue/Angular
 * @param {Object} params - {selector: string, value: string}
 */
function actionFill(params) {
  const { selector, value } = params;
  if (!selector || value === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'Selector and value required', recoverable: false },
    };
  }

  const element = findElement(selector);
  if (!element) {
    return {
      success: false,
      error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
    };
  }

  // Scroll into view
  element.scrollIntoView({ behavior: 'instant', block: 'center' });

  // Focus the element
  element.focus();
  element.dispatchEvent(new FocusEvent('focus', { bubbles: true }));

  // Clear existing value
  element.value = '';
  element.dispatchEvent(
    new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward' })
  );

  // Set value using native setter to bypass React/Vue's property override
  const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    'value'
  )?.set;
  const nativeTextAreaValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    'value'
  )?.set;

  if (element.tagName === 'TEXTAREA' && nativeTextAreaValueSetter) {
    nativeTextAreaValueSetter.call(element, value);
  } else if (nativeInputValueSetter) {
    nativeInputValueSetter.call(element, value);
  } else {
    element.value = value;
  }

  // Dispatch comprehensive events for framework compatibility
  element.dispatchEvent(
    new InputEvent('input', {
      bubbles: true,
      cancelable: true,
      inputType: 'insertText',
      data: value,
    })
  );

  element.dispatchEvent(new Event('change', { bubbles: true }));

  // Trigger compositionend for some frameworks
  element.dispatchEvent(
    new CompositionEvent('compositionend', {
      bubbles: true,
      data: value,
    })
  );

  return { success: true, result: { selector, filled: value } };
}

/**
 * Get page or element content
 * @param {Object} params - {selector?: string}
 */
function actionGetContent(params) {
  const { selector } = params;

  if (selector) {
    const element = findElement(selector);
    if (!element) {
      return {
        success: false,
        error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
      };
    }
    return {
      success: true,
      result: {
        selector,
        text: element.textContent,
        html: element.innerHTML,
      },
    };
  }

  // Return page content
  return {
    success: true,
    result: {
      title: document.title,
      url: window.location.href,
      text: document.body.innerText,
      // Limit HTML size
      html: document.documentElement.outerHTML.substring(0, 100000),
    },
  };
}

/**
 * Wait for an element to appear
 * @param {Object} params - {selector: string, timeout_ms?: number}
 */
async function actionWaitFor(params) {
  const { selector, timeout_ms = 10000 } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'Selector required', recoverable: false },
    };
  }

  try {
    const element = await waitForElement(selector, timeout_ms);
    return { success: true, result: { selector, found: true, info: getElementInfo(element) } };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Scroll the page
 * @param {Object} params - {direction: string, amount?: number, selector?: string}
 */
function actionScroll(params) {
  const { direction, amount = 300, selector } = params;

  const target = selector ? findElement(selector) : window;
  if (selector && !target) {
    return {
      success: false,
      error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
    };
  }

  const scrollOptions = { behavior: 'smooth' };

  switch (direction) {
    case 'up':
      if (target === window) {
        window.scrollBy({ top: -amount, ...scrollOptions });
      } else {
        target.scrollTop -= amount;
      }
      break;
    case 'down':
      if (target === window) {
        window.scrollBy({ top: amount, ...scrollOptions });
      } else {
        target.scrollTop += amount;
      }
      break;
    case 'left':
      if (target === window) {
        window.scrollBy({ left: -amount, ...scrollOptions });
      } else {
        target.scrollLeft -= amount;
      }
      break;
    case 'right':
      if (target === window) {
        window.scrollBy({ left: amount, ...scrollOptions });
      } else {
        target.scrollLeft += amount;
      }
      break;
    default:
      return {
        success: false,
        error: { code: -1, message: `Unknown direction: ${direction}`, recoverable: false },
      };
  }

  return { success: true, result: { direction, amount } };
}

/**
 * Get element info
 * @param {Object} params - {selector: string}
 */
function actionGetElement(params) {
  const { selector } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'Selector required', recoverable: false },
    };
  }

  const element = findElement(selector);
  if (!element) {
    return {
      success: false,
      error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
    };
  }

  return { success: true, result: getElementInfo(element) };
}

/**
 * Query all matching elements
 * @param {Object} params - {selector: string, limit?: number}
 */
function actionQueryAll(params) {
  const { selector, limit = 50 } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'Selector required', recoverable: false },
    };
  }

  const elements = findAllElements(selector, limit);
  const results = elements.map((el) => getElementInfo(el));

  return { success: true, result: { count: results.length, elements: results } };
}

// =============================================================================
// Keyboard Helpers
// =============================================================================

/**
 * Press Enter key on an element with full event simulation
 * @param {Element} element - Target element (should already have focus)
 */
function pressEnterKey(element) {
  const enterEventOptions = {
    key: 'Enter',
    code: 'Enter',
    keyCode: 13,
    which: 13,
    charCode: 13,
    bubbles: true,
    cancelable: true,
    view: window,
    composed: true, // Allows event to cross shadow DOM boundary
  };

  // Dispatch full keyboard event sequence on the element
  element.dispatchEvent(new KeyboardEvent('keydown', enterEventOptions));
  element.dispatchEvent(new KeyboardEvent('keypress', enterEventOptions));
  element.dispatchEvent(new KeyboardEvent('keyup', enterEventOptions));

  // For Vue/React UI frameworks: also dispatch on parent wrappers
  // These frameworks often have event listeners on wrapper components
  let parent = element.parentElement;
  let depth = 0;
  while (parent && depth < 5) {
    const classList = parent.classList;
    const className = parent.className || '';

    // Check for common UI framework component wrapper classes
    const isFrameworkWrapper =
      // Vue - Element UI
      classList.contains('el-input') ||
      classList.contains('el-select') ||
      classList.contains('el-autocomplete') ||
      classList.contains('el-input-group') ||
      // Vue - Vuetify
      classList.contains('v-input') ||
      classList.contains('v-text-field') ||
      // Vue data attribute
      Array.from(parent.attributes).some((attr) => attr.name.startsWith('data-v-')) ||
      // React - Material-UI (MUI)
      className.includes('MuiInput') ||
      className.includes('MuiTextField') ||
      className.includes('MuiAutocomplete') ||
      className.includes('MuiOutlinedInput') ||
      // React - Ant Design
      classList.contains('ant-input') ||
      classList.contains('ant-input-affix-wrapper') ||
      classList.contains('ant-select') ||
      classList.contains('ant-input-search') ||
      classList.contains('ant-input-group') ||
      // React - Chakra UI
      parent.hasAttribute('data-chakra-component') ||
      // React - Semantic UI
      (classList.contains('ui') && classList.contains('input')) ||
      (classList.contains('ui') && classList.contains('search')) ||
      // React - React Bootstrap / general Bootstrap
      classList.contains('form-group') ||
      classList.contains('input-group') ||
      // Generic patterns
      className.includes('input-wrapper') ||
      className.includes('search-box') ||
      className.includes('search-input') ||
      className.includes('InputBase') ||
      className.includes('TextInput');

    if (isFrameworkWrapper) {
      console.log('[NevoFlux] Also dispatching Enter on parent:', className);
      parent.dispatchEvent(new KeyboardEvent('keydown', enterEventOptions));
      parent.dispatchEvent(new KeyboardEvent('keypress', enterEventOptions));
      parent.dispatchEvent(new KeyboardEvent('keyup', enterEventOptions));
    }
    parent = parent.parentElement;
    depth++;
  }

  // Try to find and click search/submit button near the input
  // UI frameworks often have suffix icons or adjacent buttons
  const inputWrapper = element.closest(
    // Vue - Element UI
    '.el-input, .el-select, .el-input-group, ' +
      // Vue - Vuetify
      '.v-input, .v-text-field, ' +
      // React - Material-UI
      "[class*='MuiInput'], [class*='MuiTextField'], [class*='MuiAutocomplete'], " +
      // React - Ant Design
      '.ant-input-affix-wrapper, .ant-input-search, .ant-input-group, .ant-select, ' +
      // Generic
      ".search-box, .search-input, [class*='search-'], .input-group, .form-group"
  );

  if (inputWrapper) {
    // Look for search icon/button within the wrapper or next to it
    const searchBtn =
      inputWrapper.querySelector(
        // Vue - Element UI
        '.el-input__suffix, .el-icon-search, .el-button, ' +
          // React - Ant Design
          '.ant-input-search-button, .ant-btn, .anticon-search, ' +
          // React - Material-UI
          "[class*='MuiIconButton'], [class*='MuiButton'], " +
          // Generic
          "button, [class*='search-btn'], [class*='search-icon'], [class*='SearchIcon'], " +
          "[role='button'], .btn"
      ) ||
      inputWrapper.nextElementSibling?.querySelector(
        "button, .el-button, .ant-btn, [class*='MuiButton'], [class*='search']"
      ) ||
      inputWrapper.parentElement?.querySelector(
        "button:not(:disabled), .el-button, .ant-btn, [class*='search']:not(input)"
      );

    if (searchBtn && !searchBtn.disabled) {
      console.log('[NevoFlux] Found search button, clicking:', searchBtn.className);
      searchBtn.click();
    }
  }

  // For form elements, also try submitting the form
  if (element.form) {
    // Check if form has submit button or should auto-submit
    const submitBtn = element.form.querySelector(
      'input[type="submit"], button[type="submit"], button:not([type])'
    );
    if (submitBtn) {
      submitBtn.click();
    } else {
      // Try form submit event
      element.form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    }
  }
}

// =============================================================================
// Snapshot - Extract Interactive Elements
// =============================================================================

// Store for element references (cleared on each snapshot)
// Exposed on window for access from browser.scripting.executeScript
let snapshotElements = new Map();
let snapshotCounter = 0;

// Expose to window for debugger-based operations
window.snapshotElements = snapshotElements;

/**
 * Check if element is visible and interactable
 * @param {Element} element
 * @returns {boolean}
 */
function isInteractable(element) {
  const rect = element.getBoundingClientRect();
  const style = window.getComputedStyle(element);

  // Must have size
  if (rect.width === 0 || rect.height === 0) return false;

  // Must be visible
  if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0')
    return false;

  // Must be in viewport (with some margin)
  const inViewport =
    rect.top < window.innerHeight + 100 &&
    rect.bottom > -100 &&
    rect.left < window.innerWidth + 100 &&
    rect.right > -100;

  return inViewport;
}

/**
 * Get element's accessible text
 * @param {Element} element
 * @returns {string}
 */
function getAccessibleText(element) {
  // Priority: aria-label > aria-labelledby > title > alt > placeholder > text content
  if (element.getAttribute('aria-label')) {
    return element.getAttribute('aria-label');
  }

  const labelledBy = element.getAttribute('aria-labelledby');
  if (labelledBy) {
    const labelEl = document.getElementById(labelledBy);
    if (labelEl) return labelEl.textContent?.trim() || '';
  }

  if (element.title) return element.title;
  if (element.alt) return element.alt;
  if (element.placeholder) return element.placeholder;

  // Get direct text content (not nested)
  const text = element.textContent?.trim() || '';
  return text.substring(0, 100); // Limit length
}

/**
 * Generate unique selector for element
 * @param {Element} element
 * @returns {string}
 */
function generateSelector(element) {
  // Try ID first
  if (element.id) {
    return `#${CSS.escape(element.id)}`;
  }

  // Try unique class combination
  if (element.className && typeof element.className === 'string') {
    const classes = element.className.split(/\s+/).filter((c) => c.length > 0);
    if (classes.length > 0) {
      const selector = `${element.tagName.toLowerCase()}.${classes.map((c) => CSS.escape(c)).join('.')}`;
      if (document.querySelectorAll(selector).length === 1) {
        return selector;
      }
    }
  }

  // Try name attribute
  if (element.name) {
    const selector = `${element.tagName.toLowerCase()}[name="${CSS.escape(element.name)}"]`;
    if (document.querySelectorAll(selector).length === 1) {
      return selector;
    }
  }

  // Try data-testid or other test attributes
  for (const attr of ['data-testid', 'data-test-id', 'data-cy', 'data-test']) {
    const value = element.getAttribute(attr);
    if (value) {
      return `[${attr}="${CSS.escape(value)}"]`;
    }
  }

  // Fallback: nth-child path
  const path = [];
  let current = element;
  while (current && current !== document.body) {
    const parent = current.parentElement;
    if (!parent) break;

    const siblings = Array.from(parent.children).filter((c) => c.tagName === current.tagName);
    const index = siblings.indexOf(current) + 1;

    if (siblings.length > 1) {
      path.unshift(`${current.tagName.toLowerCase()}:nth-of-type(${index})`);
    } else {
      path.unshift(current.tagName.toLowerCase());
    }

    current = parent;
    if (path.length > 5) break; // Limit depth
  }

  return path.join(' > ');
}

/**
 * Check if element has cursor:pointer style (looks clickable)
 * @param {Element} element
 * @returns {boolean}
 */
function hasClickableCursor(element) {
  try {
    const style = window.getComputedStyle(element);
    return style.cursor === 'pointer';
  } catch (e) {
    return false;
  }
}

/**
 * Find the nearest clickable ancestor (has click handler or is natively clickable)
 * @param {Element} element
 * @param {number} maxDepth
 * @returns {Element|null}
 */
function findClickableAncestor(element, maxDepth = 8) {
  let current = element.parentElement;
  let depth = 0;
  let listItemAncestor = null; // Track li/tr in case no other clickable found

  while (current && depth < maxDepth && current !== document.body) {
    const tagName = current.tagName.toLowerCase();

    // Check if ancestor is natively clickable
    if (tagName === 'a' || tagName === 'button') {
      return current;
    }

    // Check for onclick attribute
    if (current.hasAttribute('onclick')) {
      return current;
    }

    // Check for interactive role
    const role = current.getAttribute('role');
    if (
      role === 'button' ||
      role === 'link' ||
      role === 'menuitem' ||
      role === 'tab' ||
      role === 'option' ||
      role === 'listitem'
    ) {
      return current;
    }

    // Check for cursor:pointer
    if (hasClickableCursor(current)) {
      return current;
    }

    // Track list items - often have event handlers via addEventListener
    // These are common click targets in menus/lists
    if ((tagName === 'li' || tagName === 'tr' || tagName === 'label') && !listItemAncestor) {
      listItemAncestor = current;
    }

    // Check for data attributes commonly used for click handling
    if (
      current.hasAttribute('data-action') ||
      current.hasAttribute('data-click') ||
      current.hasAttribute('data-handler') ||
      current.hasAttribute('data-id') ||
      current.hasAttribute('data-value')
    ) {
      return current;
    }

    current = current.parentElement;
    depth++;
  }

  // If no explicitly clickable ancestor found, return list item if we found one
  // (li/tr often have click handlers attached via addEventListener)
  return listItemAncestor;
}

/**
 * Take a snapshot of all interactive elements on the page
 * @param {Object} params - {include_hidden?: boolean}
 * @returns {Object}
 */
function actionSnapshot(params = {}) {
  const { include_hidden = false } = params;

  // Clear previous snapshot
  snapshotElements.clear();
  snapshotCounter = 0;

  const interactiveSelectors = [
    // Clickable elements
    'a[href]',
    'button',
    "[role='button']",
    '[onclick]',
    // Form inputs
    "input:not([type='hidden'])",
    'textarea',
    'select',
    "[contenteditable='true']",
    // Interactive widgets
    "[role='link']",
    "[role='menuitem']",
    "[role='tab']",
    "[role='checkbox']",
    "[role='radio']",
    "[role='switch']",
    "[role='slider']",
    "[role='spinbutton']",
    "[role='combobox']",
    "[role='searchbox']",
    "[role='textbox']",
    // Clickable by common patterns
    "[tabindex]:not([tabindex='-1'])",
    'summary',
    'details',
  ];

  // Additional: Find elements with cursor:pointer that have text (likely clickable)
  // Check common text elements that might be styled as clickable
  const cursorPointerElements = [];
  const textTags = ['span', 'div', 'li', 'p', 'label', 'td', 'th'];
  for (const tag of textTags) {
    try {
      const tagElements = document.querySelectorAll(tag);
      for (const el of tagElements) {
        // Only include if has cursor:pointer and some text
        if (hasClickableCursor(el) && el.textContent?.trim()) {
          // Avoid duplicates with elements that have onclick or role
          if (!el.hasAttribute('onclick') && !el.getAttribute('role')) {
            cursorPointerElements.push(el);
          }
        }
      }
    } catch (e) {
      // Ignore errors
    }
  }

  const elements = [];
  const seen = new Set();

  // First add cursor:pointer elements (they're often the actual clickable targets)
  for (const el of cursorPointerElements) {
    if (seen.has(el)) continue;
    if (!include_hidden && !isInteractable(el)) continue;
    seen.add(el);

    const elementId = ++snapshotCounter;
    snapshotElements.set(elementId, el);
    el.setAttribute('data-nevoflux-id', elementId);

    const rect = el.getBoundingClientRect();
    const text = getAccessibleText(el);
    const tagName = el.tagName.toLowerCase();

    elements.push({
      id: elementId,
      tag: tagName,
      type: `${tagName}[clickable]`,
      text: text,
      selector: generateSelector(el),
      rect: {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      },
      attributes: {
        id: el.id || null,
        name: el.name || null,
        href: el.href || null,
        value: el.value || null,
        placeholder: el.placeholder || null,
        disabled: el.disabled || false,
        readonly: el.readOnly || false,
      },
    });
  }

  for (const selector of interactiveSelectors) {
    try {
      const matches = document.querySelectorAll(selector);
      for (const el of matches) {
        // Skip duplicates
        if (seen.has(el)) continue;
        seen.add(el);

        // Skip hidden unless requested
        if (!include_hidden && !isInteractable(el)) continue;

        // Generate element ID
        const elementId = ++snapshotCounter;
        snapshotElements.set(elementId, el);

        // Mark element with data attribute for visual debugging
        el.setAttribute('data-nevoflux-id', elementId);

        const rect = el.getBoundingClientRect();
        const text = getAccessibleText(el);
        const inputType = el.getAttribute('type') || '';
        const tagName = el.tagName.toLowerCase();

        // Determine element type for LLM
        let elementType = tagName;
        if (tagName === 'input') {
          elementType = `input[${inputType || 'text'}]`;
        } else if (el.getAttribute('role')) {
          elementType = `[role=${el.getAttribute('role')}]`;
        }

        elements.push({
          id: elementId,
          tag: tagName,
          type: elementType,
          text: text,
          selector: generateSelector(el),
          rect: {
            x: Math.round(rect.x),
            y: Math.round(rect.y),
            width: Math.round(rect.width),
            height: Math.round(rect.height),
          },
          attributes: {
            id: el.id || null,
            name: el.name || null,
            href: el.href || null,
            value: el.value || null,
            placeholder: el.placeholder || null,
            disabled: el.disabled || false,
            readonly: el.readOnly || false,
          },
        });
      }
    } catch (e) {
      console.warn(`[NevoFlux] Snapshot selector error: ${selector}`, e);
    }
  }

  // Sort by position (top to bottom, left to right)
  elements.sort((a, b) => {
    if (Math.abs(a.rect.y - b.rect.y) > 20) {
      return a.rect.y - b.rect.y;
    }
    return a.rect.x - b.rect.x;
  });

  // Format for LLM
  const formatted = elements
    .map((el) => {
      let desc = `[${el.id}] <${el.type}>`;
      if (el.text) desc += ` "${el.text}"`;
      if (el.attributes.placeholder) desc += ` (placeholder: ${el.attributes.placeholder})`;
      if (el.attributes.name) desc += ` name="${el.attributes.name}"`;
      if (el.attributes.disabled) desc += ' [disabled]';
      return desc;
    })
    .join('\n');

  return {
    success: true,
    result: {
      count: elements.length,
      elements: elements,
      formatted: formatted,
      url: window.location.href,
      title: document.title,
    },
  };
}

/**
 * Dispatch full mouse event sequence on an element
 * @param {Element} element
 * @param {number} centerX
 * @param {number} centerY
 */
function dispatchClickEvents(element, centerX, centerY) {
  const eventOptions = {
    bubbles: true,
    cancelable: true,
    view: window,
    clientX: centerX,
    clientY: centerY,
    screenX: centerX,
    screenY: centerY,
    button: 0,
    buttons: 1,
  };

  element.dispatchEvent(new MouseEvent('mouseenter', { ...eventOptions, bubbles: false }));
  element.dispatchEvent(new MouseEvent('mouseover', eventOptions));
  element.dispatchEvent(new MouseEvent('mousemove', eventOptions));
  element.dispatchEvent(new MouseEvent('mousedown', eventOptions));
  element.focus?.();
  element.dispatchEvent(new MouseEvent('mouseup', eventOptions));
  element.dispatchEvent(new MouseEvent('click', eventOptions));
  element.click();
}

/**
 * Click element by snapshot ID
 * @param {Object} params - {element_id: number}
 */
function actionClickById(params) {
  const { element_id } = params;
  if (!element_id) {
    return {
      success: false,
      error: { code: -1, message: 'element_id required', recoverable: false },
    };
  }

  const element = snapshotElements.get(element_id);
  if (!element) {
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} not found. Take a new snapshot first.`,
        recoverable: true,
      },
    };
  }

  // Check if still in DOM
  if (!document.contains(element)) {
    snapshotElements.delete(element_id);
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} no longer exists. Take a new snapshot.`,
        recoverable: true,
      },
    };
  }

  // Scroll into view
  element.scrollIntoView({ behavior: 'smooth', block: 'center' });

  // Get element center for realistic click coordinates
  const rect = element.getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;

  // First, try clicking the element directly
  dispatchClickEvents(element, centerX, centerY);

  // For text elements (span, div, p, etc.), also try clicking parent elements
  // because many frameworks use event delegation on parent containers
  const tagName = element.tagName.toLowerCase();
  const textTags = ['span', 'div', 'p', 'label', 'td', 'th', 'em', 'strong', 'i', 'b'];

  if (textTags.includes(tagName)) {
    // Find and click the nearest clickable ancestor
    const clickableAncestor = findClickableAncestor(element);
    if (clickableAncestor) {
      console.log(
        '[NevoFlux] Also clicking parent element:',
        clickableAncestor.tagName,
        clickableAncestor.className
      );
      const ancestorRect = clickableAncestor.getBoundingClientRect();
      const ancestorCenterX = ancestorRect.left + ancestorRect.width / 2;
      const ancestorCenterY = ancestorRect.top + ancestorRect.height / 2;
      dispatchClickEvents(clickableAncestor, ancestorCenterX, ancestorCenterY);
      return {
        success: true,
        result: {
          element_id,
          clicked: true,
          also_clicked_parent: clickableAncestor.tagName.toLowerCase(),
        },
      };
    }

    // Last resort: try clicking at the element's coordinates using document.elementFromPoint
    // This simulates a real click at that position
    console.log('[NevoFlux] Trying elementFromPoint click at', centerX, centerY);
    const elementAtPoint = document.elementFromPoint(centerX, centerY);
    if (elementAtPoint && elementAtPoint !== element) {
      console.log(
        '[NevoFlux] elementFromPoint found different element:',
        elementAtPoint.tagName,
        elementAtPoint.className
      );
      dispatchClickEvents(elementAtPoint, centerX, centerY);
      return {
        success: true,
        result: {
          element_id,
          clicked: true,
          clicked_element_at_point: elementAtPoint.tagName.toLowerCase(),
        },
      };
    }
  }

  return { success: true, result: { element_id, clicked: true } };
}

/**
 * Fill element by snapshot ID
 * @param {Object} params - {element_id: number, value: string, press_enter?: boolean}
 */
async function actionFillById(params) {
  const { element_id, value, press_enter = false } = params;
  if (!element_id || value === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'element_id and value required', recoverable: false },
    };
  }

  const element = snapshotElements.get(element_id);
  if (!element) {
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} not found. Take a new snapshot first.`,
        recoverable: true,
      },
    };
  }

  if (!document.contains(element)) {
    snapshotElements.delete(element_id);
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} no longer exists. Take a new snapshot.`,
        recoverable: true,
      },
    };
  }

  // Focus and fill
  element.scrollIntoView({ behavior: 'instant', block: 'center' });
  element.focus();
  element.dispatchEvent(new FocusEvent('focus', { bubbles: true }));

  // Clear and set value
  const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    'value'
  )?.set;
  const nativeTextAreaValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    'value'
  )?.set;

  if (element.tagName === 'TEXTAREA' && nativeTextAreaValueSetter) {
    nativeTextAreaValueSetter.call(element, value);
  } else if (nativeInputValueSetter) {
    nativeInputValueSetter.call(element, value);
  } else {
    element.value = value;
  }

  // Dispatch events
  element.dispatchEvent(
    new InputEvent('input', {
      bubbles: true,
      cancelable: true,
      inputType: 'insertText',
      data: value,
    })
  );
  element.dispatchEvent(new Event('change', { bubbles: true }));

  // Press Enter if requested
  if (press_enter) {
    // Re-focus element before pressing Enter (some pages lose focus after input events)
    element.focus();
    element.dispatchEvent(new FocusEvent('focus', { bubbles: true }));
    // Small delay to ensure focus is established
    await new Promise((r) => setTimeout(r, 50));
    pressEnterKey(element);
  }

  return { success: true, result: { element_id, filled: value, enter_pressed: press_enter } };
}

/**
 * Type text into element by snapshot ID (character by character)
 * @param {Object} params - {element_id: number, text: string, press_enter?: boolean}
 */
async function actionTypeById(params) {
  const { element_id, text, press_enter = false } = params;
  if (!element_id || text === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'element_id and text required', recoverable: false },
    };
  }

  const element = snapshotElements.get(element_id);
  if (!element) {
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} not found. Take a new snapshot first.`,
        recoverable: true,
      },
    };
  }

  if (!document.contains(element)) {
    snapshotElements.delete(element_id);
    return {
      success: false,
      error: {
        code: -1,
        message: `Element ID ${element_id} no longer exists. Take a new snapshot.`,
        recoverable: true,
      },
    };
  }

  element.scrollIntoView({ behavior: 'instant', block: 'center' });
  element.focus();

  const nativeSetter =
    element.tagName === 'TEXTAREA'
      ? Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value')?.set
      : Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set;

  for (const char of text) {
    element.dispatchEvent(new KeyboardEvent('keydown', { key: char, bubbles: true }));

    const newValue = element.value + char;
    if (nativeSetter) {
      nativeSetter.call(element, newValue);
    } else {
      element.value = newValue;
    }

    element.dispatchEvent(
      new InputEvent('input', {
        bubbles: true,
        inputType: 'insertText',
        data: char,
      })
    );
    element.dispatchEvent(new KeyboardEvent('keyup', { key: char, bubbles: true }));
  }

  element.dispatchEvent(new Event('change', { bubbles: true }));

  // Press Enter if requested
  if (press_enter) {
    // Re-focus element before pressing Enter (some pages lose focus after input events)
    element.focus();
    element.dispatchEvent(new FocusEvent('focus', { bubbles: true }));
    // Small delay to ensure focus is established
    await new Promise((r) => setTimeout(r, 50));
    pressEnterKey(element);
  }

  return { success: true, result: { element_id, typed: text, enter_pressed: press_enter } };
}

// =============================================================================
// Message Handler
// =============================================================================

/**
 * Handle browser tool action from background script
 */
async function handleBrowserToolAction(action, params) {
  console.log('[NevoFlux] Content script handling action:', action, params);

  switch (action) {
    case 'click':
      return actionClick(params);

    case 'type':
      return actionType(params);

    case 'fill':
      return actionFill(params);

    case 'get_content':
      return actionGetContent(params);

    case 'wait_for':
      return await actionWaitFor(params);

    case 'scroll':
      return actionScroll(params);

    case 'get_element':
      return actionGetElement(params);

    case 'query_all':
      return actionQueryAll(params);

    case 'snapshot':
      return actionSnapshot(params);

    case 'click_by_id':
      return actionClickById(params);

    case 'fill_by_id':
      return actionFillById(params);

    case 'type_by_id':
      return actionTypeById(params);

    default:
      return {
        success: false,
        error: { code: -1, message: `Unknown action: ${action}`, recoverable: false },
      };
  }
}

/**
 * Listen for messages from background script
 */
browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log('[NevoFlux] Content script received:', message);

  // Handle browser tool actions
  if (message.type === 'browser_tool_action') {
    const result = handleBrowserToolAction(message.action, message.params);

    // Handle async actions
    if (result instanceof Promise) {
      result.then(sendResponse);
      return true; // Keep message channel open for async response
    }

    sendResponse(result);
    return;
  }

  // Legacy message types for backwards compatibility
  switch (message.type) {
    case 'extract_content':
      sendResponse(actionGetContent({}));
      break;

    case 'click_element':
      sendResponse(actionClick({ selector: message.selector }));
      break;

    case 'fill_form':
      // Convert legacy format
      const results = [];
      for (const field of message.fields || []) {
        results.push(actionFill({ selector: field.selector, value: field.value }));
      }
      sendResponse({ success: true, results });
      break;

    default:
      console.warn('[NevoFlux] Unknown message type:', message.type);
      sendResponse({
        success: false,
        error: { code: -1, message: 'Unknown message type', recoverable: false },
      });
  }
});

console.log('[NevoFlux] Agent content script loaded');
