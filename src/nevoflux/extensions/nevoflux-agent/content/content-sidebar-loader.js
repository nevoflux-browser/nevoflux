/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * NevoFlux Content Sidebar Loader
 *
 * This script is injected into web pages to load and initialize the
 * Content Sidebar Dioxus WASM application within a Shadow DOM container.
 *
 * Injection timing: document_start (to prevent FOUC)
 */

(function () {
  // Prevent double initialization
  if (window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__) {
    console.log("[NevoFlux] Content Sidebar already loaded");
    return;
  }
  window.__NEVOFLUX_CONTENT_SIDEBAR_LOADED__ = true;

  const SHADOW_HOST_ID = "nevoflux-content-sidebar-host";
  // Paths are dynamically discovered from init.js which Trunk generates
  const INIT_JS_PATH = browser.runtime.getURL("wasm/content-sidebar/init.js");

  /**
   * Create and configure the Shadow DOM host element
   */
  function createShadowHost() {
    // Check if already exists
    if (document.getElementById(SHADOW_HOST_ID)) {
      console.log("[NevoFlux] Shadow host already exists");
      return document.getElementById(SHADOW_HOST_ID).shadowRoot;
    }

    // Create host element
    const host = document.createElement("div");
    host.id = SHADOW_HOST_ID;

    // Apply host styles for positioning
    Object.assign(host.style, {
      position: "fixed",
      top: "0",
      right: "0",
      width: "320px",
      height: "100vh",
      zIndex: "2147483647", // Maximum z-index
      pointerEvents: "auto",
      fontFamily: "system-ui, -apple-system, sans-serif",
      display: "block",
    });

    // Attach closed Shadow DOM for style isolation
    const shadowRoot = host.attachShadow({ mode: "closed" });

    // Wait for body to be available
    if (document.body) {
      document.body.appendChild(host);
    } else {
      // If body not ready (document_start timing), wait for it
      document.addEventListener("DOMContentLoaded", () => {
        document.body.appendChild(host);
      }, { once: true });
    }

    console.log("[NevoFlux] Shadow DOM host created");
    return shadowRoot;
  }

  /**
   * Inject styles into Shadow DOM
   */
  function injectStyles(shadowRoot) {
    const style = document.createElement("style");
    style.textContent = getContentSidebarStyles();
    shadowRoot.appendChild(style);
  }

  /**
   * Create mount point for Dioxus within Shadow DOM
   */
  function createMountPoint(shadowRoot) {
    const mountPoint = document.createElement("div");
    mountPoint.id = "dioxus-mount";
    mountPoint.className = "dioxus-root";
    shadowRoot.appendChild(mountPoint);
    return mountPoint;
  }

  /**
   * Load and initialize the WASM module via Trunk-generated init.js
   */
  async function loadWasmModule(shadowRoot, mountPoint) {
    try {
      // Import the init.js which Trunk generates - it handles WASM loading
      await import(INIT_JS_PATH);

      // The init.js will import the main JS and initialize WASM
      // Wait for the WASM bindings to be available
      await waitForWasmBindings();

      // Call the Content Sidebar initialization function if available
      if (window.wasmBindings && typeof window.wasmBindings.init_content_sidebar === "function") {
        window.wasmBindings.init_content_sidebar();
      } else {
        console.log("[NevoFlux] WASM initialized via Dioxus auto-launch");
      }

      console.log("[NevoFlux] Content Sidebar WASM loaded successfully");

      // Notify background script that we're ready
      notifyReady();

    } catch (error) {
      console.error("[NevoFlux] Failed to load Content Sidebar WASM:", error);

      // Show error UI in shadow DOM
      showErrorUI(shadowRoot, error.message);
    }
  }

  /**
   * Wait for WASM bindings to be available (with timeout)
   */
  function waitForWasmBindings(timeout = 5000) {
    return new Promise((resolve, reject) => {
      const startTime = Date.now();

      function check() {
        if (window.wasmBindings) {
          resolve(window.wasmBindings);
        } else if (Date.now() - startTime > timeout) {
          // Timeout is OK - Dioxus may auto-launch without bindings
          resolve(null);
        } else {
          setTimeout(check, 50);
        }
      }

      check();
    });
  }

  /**
   * Fallback: Load without WASM (JavaScript-only mode)
   */
  function loadFallbackUI(shadowRoot) {
    const container = document.createElement("div");
    container.className = "content-sidebar fallback";
    container.innerHTML = `
      <div class="fallback-content">
        <div class="url-display">
          <span class="protocol-badge ${location.protocol === "https:" ? "secure" : ""}">
            ${location.protocol}//
          </span>
          <span class="domain">${location.hostname}</span>
        </div>
        <p class="page-title">${document.title || "Untitled"}</p>
        <div class="status-section">
          <div class="status-indicator active"></div>
          <span class="status-text">Monitoring page (Fallback Mode)</span>
        </div>
        <div class="branding">NevoFlux</div>
      </div>
    `;
    shadowRoot.appendChild(container);

    // Notify ready even in fallback mode
    notifyReady();
  }

  /**
   * Show error UI
   */
  function showErrorUI(shadowRoot, errorMessage) {
    const container = document.createElement("div");
    container.className = "content-sidebar error";
    container.innerHTML = `
      <div class="error-content">
        <div class="error-icon">!</div>
        <p class="error-title">Failed to Load</p>
        <p class="error-message">${escapeHtml(errorMessage)}</p>
        <button class="retry-btn" id="nevoflux-retry">Retry</button>
      </div>
    `;
    shadowRoot.appendChild(container);

    // Add retry handler
    const retryBtn = shadowRoot.getElementById("nevoflux-retry");
    if (retryBtn) {
      retryBtn.addEventListener("click", () => {
        shadowRoot.innerHTML = "";
        init();
      });
    }
  }

  /**
   * Notify background script that Content Sidebar is ready
   */
  function notifyReady() {
    browser.runtime.sendMessage({
      type: "content_sidebar_ready",
      payload: {
        tab_id: 0, // Will be filled by background script from sender
        url: location.href,
        title: document.title
      }
    }).catch(err => {
      console.debug("[NevoFlux] Could not notify ready:", err.message);
    });
  }

  /**
   * Set up message listener for Content Sidebar
   */
  function setupMessageListener() {
    browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
      // Only handle messages targeted at Content Sidebar
      if (message.target !== "content-sidebar") {
        return;
      }

      console.log("[NevoFlux Content] Received:", message.type);

      // Forward to WASM module if available
      if (window.__NEVOFLUX_CONTENT_SIDEBAR_HANDLER__) {
        window.__NEVOFLUX_CONTENT_SIDEBAR_HANDLER__(message);
      }
    });
  }

  /**
   * Escape HTML for safe display
   */
  function escapeHtml(text) {
    const div = document.createElement("div");
    div.textContent = text;
    return div.innerHTML;
  }

  /**
   * Get Content Sidebar CSS styles (embedded for Shadow DOM)
   */
  function getContentSidebarStyles() {
    return `
      /* Reset */
      *, *::before, *::after {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }

      .dioxus-root, .content-sidebar {
        width: 100%;
        height: 100%;
        font-family: system-ui, -apple-system, sans-serif;
        font-size: 14px;
        color: #eaeaea;
        line-height: 1.5;
        background: #1a1a2e;
        border-left: 1px solid rgba(255, 255, 255, 0.1);
        box-shadow: -4px 0 16px rgba(0, 0, 0, 0.3);
      }

      /* Fallback mode styles */
      .fallback .fallback-content {
        padding: 16px;
        height: 100%;
        display: flex;
        flex-direction: column;
      }

      .url-display {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 12px;
        background: rgba(255, 255, 255, 0.05);
        border-radius: 8px;
        overflow: hidden;
      }

      .protocol-badge {
        font-size: 12px;
        color: #666;
      }

      .protocol-badge.secure {
        color: #22c55e;
      }

      .domain {
        font-weight: 500;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .page-title {
        margin-top: 12px;
        font-size: 12px;
        color: #888;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .status-section {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-top: 16px;
        padding: 8px;
      }

      .status-indicator {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: #666;
      }

      .status-indicator.active {
        background: #22c55e;
        animation: pulse 2s infinite;
      }

      .status-text {
        font-size: 12px;
        color: #888;
      }

      .branding {
        margin-top: auto;
        padding: 8px;
        text-align: center;
        font-size: 12px;
        color: #444;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
      }

      /* Error mode styles */
      .error .error-content {
        padding: 24px;
        height: 100%;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        text-align: center;
        gap: 12px;
      }

      .error-icon {
        width: 48px;
        height: 48px;
        border-radius: 50%;
        background: #ef4444;
        color: white;
        font-size: 24px;
        font-weight: bold;
        display: flex;
        align-items: center;
        justify-content: center;
      }

      .error-title {
        font-size: 16px;
        font-weight: 500;
      }

      .error-message {
        font-size: 12px;
        color: #888;
        max-width: 250px;
      }

      .retry-btn {
        margin-top: 16px;
        padding: 8px 24px;
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        color: #eaeaea;
        cursor: pointer;
        transition: all 0.15s ease;
      }

      .retry-btn:hover {
        background: rgba(255, 255, 255, 0.08);
        border-color: #6366f1;
      }

      @keyframes pulse {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.5; }
      }
    `;
  }

  /**
   * Main initialization function
   */
  async function init() {
    console.log("[NevoFlux] Initializing Content Sidebar...");

    // Set up message listener first
    setupMessageListener();

    // Create Shadow DOM structure
    const shadowRoot = createShadowHost();
    if (!shadowRoot) {
      console.error("[NevoFlux] Failed to create Shadow DOM");
      return;
    }

    // Inject styles
    injectStyles(shadowRoot);

    // Create mount point
    const mountPoint = createMountPoint(shadowRoot);

    // Try to load WASM module
    try {
      await loadWasmModule(shadowRoot, mountPoint);
    } catch (error) {
      console.warn("[NevoFlux] WASM load failed, using fallback UI:", error);
      loadFallbackUI(shadowRoot);
    }
  }

  // Initialize when the script loads
  init();

})();
