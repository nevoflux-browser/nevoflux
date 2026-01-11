/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * NevoFlux Agent Background Script
 * Manages communication between the sidebar UI, content scripts, and native messaging host
 */

// Native messaging port to Rust agent
let nativePort = null;
let portListeners = null;

/**
 * Ensure native messaging connection is established (lazy connection)
 * @returns {Port|null} Native messaging port
 */
function ensureNativeConnection() {
  if (!nativePort) {
    connectNativeMessaging();
  }
  return nativePort;
}

/**
 * Initialize native messaging connection
 */
function connectNativeMessaging() {
  if (nativePort) {
    console.log("Native messaging already connected");
    return;
  }

  try {
    nativePort = browser.runtime.connectNative("com.nevoflux.agent");

    portListeners = {
      onMessage: (message) => {
        console.log("Received from native:", message);
        // Forward to sidebar or content script as needed
        browser.runtime.sendMessage({
          type: "native_response",
          data: message
        }).catch((err) => {
          console.warn("Failed to forward native message:", err);
        });
      },
      onDisconnect: (port) => {
        console.error("Native messaging disconnected:", port.error);
        nativePort = null;
        portListeners = null;
        // Auto-reconnect on next message
      }
    };

    nativePort.onMessage.addListener(portListeners.onMessage);
    nativePort.onDisconnect.addListener(portListeners.onDisconnect);

    console.log("Native messaging connected");
  } catch (error) {
    console.error("Failed to connect native messaging:", error);
    nativePort = null;
  }
}

/**
 * Handle messages from sidebar or content scripts
 */
browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log("Background received message:", message);

  switch (message.type) {
    case "agent_request":
      // Forward to native agent (lazy connection)
      const port = ensureNativeConnection();
      if (port) {
        port.postMessage(message.data);
      } else {
        console.error("Native port not connected");
        sendResponse({ error: "Native messaging not available" });
      }
      break;

    case "get_page_content":
      // Get content from active tab
      browser.tabs.query({ active: true, currentWindow: true }).then((tabs) => {
        if (tabs[0]) {
          browser.tabs.sendMessage(tabs[0].id, { type: "extract_content" })
            .then(sendResponse)
            .catch((error) => sendResponse({ error: error.message }));
        }
      });
      return true; // Keep channel open for async response

    case "browser_action":
      // Execute browser actions (navigate, click, etc.)
      handleBrowserAction(message.data)
        .then(sendResponse)
        .catch((error) => sendResponse({ error: error.message }));
      return true;

    default:
      console.warn("Unknown message type:", message.type);
  }
});

/**
 * Handle browser automation actions
 */
async function handleBrowserAction(action) {
  const { command, params } = action;

  switch (command) {
    case "navigate":
      const tab = await browser.tabs.create({ url: params.url });
      return { success: true, tabId: tab.id };

    case "click":
      // Send to content script
      const tabs = await browser.tabs.query({ active: true, currentWindow: true });
      if (tabs[0]) {
        return await browser.tabs.sendMessage(tabs[0].id, {
          type: "click_element",
          selector: params.selector
        });
      }
      throw new Error("No active tab");

    case "fill_form":
      // Send to content script
      const activeTabs = await browser.tabs.query({ active: true, currentWindow: true });
      if (activeTabs[0]) {
        return await browser.tabs.sendMessage(activeTabs[0].id, {
          type: "fill_form",
          fields: params.fields
        });
      }
      throw new Error("No active tab");

    default:
      throw new Error(`Unknown browser action: ${command}`);
  }
}

// Manifest V3: Don't connect on startup - use lazy connection
// Connection will be established on first message via ensureNativeConnection()

console.log("NevoFlux Agent background script loaded (Manifest V3)");
