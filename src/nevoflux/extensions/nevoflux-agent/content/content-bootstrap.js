/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * NevoFlux Content Bootstrap
 *
 * Lightweight script that runs at document_start to prevent flashing.
 * Creates the Shadow DOM container immediately, before page renders.
 * WASM loading happens after DOM is ready.
 */

(async function() {
  // Skip non-http(s) pages
  if (!location.protocol.startsWith('http')) {
    return;
  }

  // Prevent double init
  if (window.__NEVOFLUX_BOOTSTRAP_RAN__) {
    return;
  }
  window.__NEVOFLUX_BOOTSTRAP_RAN__ = true;

  // Check storage for Content Sidebar settings
  let shouldInject = false;

  try {
    const settings = await browser.storage.local.get('contentSidebarSettings');
    const config = settings.contentSidebarSettings || {};

    // Check if auto-inject is enabled
    if (config.autoInject) {
      const domain = location.hostname;

      // Check blacklist
      if (config.blacklist && config.blacklist.includes(domain)) {
        shouldInject = false;
      }
      // Check whitelist (if exists, domain must be in it)
      else if (config.whitelist && config.whitelist.length > 0) {
        shouldInject = config.whitelist.includes(domain);
      }
      else {
        shouldInject = true;
      }
    }
  } catch (error) {
    // Storage access might fail, check for manual injection flag
    console.debug('[NevoFlux] Bootstrap storage check failed:', error.message);
  }

  // Listen for messages from background script
  browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    // Handle manual injection requests
    if (message.type === 'inject_content_sidebar' || message.target === 'content-bootstrap') {
      if (!window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__) {
        initContentSidebar();
      }
      sendResponse({ success: true });
      return;
    }

    // Forward messages to Content Sidebar WASM module via custom event
    // This bridges browser.tabs.sendMessage -> WASM module
    if (message.target === 'content-sidebar' && window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__) {
      console.log('[NevoFlux] Forwarding message to Content Sidebar WASM:', message.type);

      // Dispatch custom event that WASM module listens to
      window.dispatchEvent(new CustomEvent('nevoflux-message', {
        detail: message
      }));

      sendResponse({ received: true });
      return true; // Indicates async response
    }
  });

  if (shouldInject) {
    initContentSidebar();
  }

  /**
   * Initialize Content Sidebar
   * Note: Using regular DOM instead of Shadow DOM for Dioxus compatibility.
   * Dioxus auto-launches and looks for #main element in the document.
   */
  function initContentSidebar() {
    if (window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__) {
      return;
    }
    window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__ = true;

    console.log('[NevoFlux] Bootstrap: Initializing Content Sidebar at document_start');

    // Create container element for Dioxus (uses #main for auto-launch)
    const container = createContainer();
    if (!container) {
      console.error('[NevoFlux] Failed to create container');
      return;
    }

    // Store for later use
    window.__NEVOFLUX_CONTAINER__ = container;

    // Load WASM when DOM is ready
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', loadWasm, { once: true });
    } else {
      loadWasm();
    }
  }

  /**
   * Create hidden container element for Dioxus Content Sidebar
   * The container is invisible - Content Sidebar operates as a background agent
   * Uses regular DOM with id="main" for Dioxus auto-launch compatibility
   */
  function createContainer() {
    const containerId = 'nevoflux-content-sidebar-host';

    // Check if already exists
    let container = document.getElementById(containerId);
    if (container) {
      return container;
    }

    // Create container element - HIDDEN, no visible UI
    container = document.createElement('div');
    container.id = containerId;

    // Hidden container - no visual presence on page
    Object.assign(container.style, {
      display: 'none',
      position: 'absolute',
      width: '0',
      height: '0',
      overflow: 'hidden',
      visibility: 'hidden',
      pointerEvents: 'none',
    });

    // Create the #main element that Dioxus expects
    const main = document.createElement('div');
    main.id = 'main';
    container.appendChild(main);

    // Append to document (or wait for body)
    const appendContainer = () => {
      if (document.body) {
        document.body.appendChild(container);
      } else if (document.documentElement) {
        document.documentElement.appendChild(container);
      }
    };

    if (document.body || document.documentElement) {
      appendContainer();
    } else {
      document.addEventListener('DOMContentLoaded', appendContainer, { once: true });
    }

    console.log('[NevoFlux] Hidden container created for background agent');
    return container;
  }

  /**
   * Load WASM module
   */
  async function loadWasm() {
    try {
      const initPath = browser.runtime.getURL('wasm/content-sidebar/init.js');
      await import(initPath);

      console.log('[NevoFlux] Content Sidebar WASM loaded');

      // Notify background
      browser.runtime.sendMessage({
        type: 'content_sidebar_ready',
        payload: {
          tab_id: 0,
          url: location.href,
          title: document.title
        }
      }).catch(() => {});

    } catch (error) {
      console.error('[NevoFlux] Failed to load WASM:', error);
      // No fallback UI - Content Sidebar operates invisibly
    }
  }

})();
